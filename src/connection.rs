use std::collections::{RingBuf, Deque};
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

use plugins::{
    DeerPlugin,
    GreedPlugin,
    SeenPlugin,
    RadioPlugin,
};

use core_plugins::CtcpVersionResponderPlugin;

use message::IrcMessage;
use command_mapper::{
    PluginContainer,
    RustBotPlugin,
};

use watchers::{
    RegisterError,
    MessageWatcher,
    RegisterMessageWatcher,
    JoinMessageWatcher,
    JoinResult
};



pub trait IrcBundleEventInterface {
    fn accept(&mut self, message: &IrcMessage) -> bool;
    fn pretty_print(&self) -> String;
}


pub struct IrcBundleJoinEvent {
    channel: String,
    nicks: Vec<String>,
    state: u16
}


impl IrcBundleJoinEvent {
    pub fn new(message: &IrcMessage) -> Option<IrcBundleJoinEvent> {
        if message.get_args().len() == 0 {
            return None;
        }
        let is_join: bool = message.get_command().as_slice() == "JOIN";
        match is_join {
            true => {
                let channel = message.get_arg(0).clone();
                Some(IrcBundleJoinEvent {
                    channel: channel,
                    nicks: Vec::new(),
                    state: 0
                })
            },
            false => None
        }
    }
}


impl IrcBundleEventInterface for IrcBundleJoinEvent {
    fn accept(&mut self, message: &IrcMessage) -> bool {
        let incr_state: bool = match self.state {
            0 => {
                // 353 contains nicks
                // 366 is ``End of /NAMES list''
                if message.get_command().as_slice() == "353" &&
                        *message.get_arg(2) == self.channel {
                    for nick in message.get_arg(3).as_slice().split(' ') {
                        self.nicks.push(String::from_str(nick));
                    }
                };
                message.get_args().len() >= 2 &&
                    message.get_command().as_slice() == "366" &&
                    *message.get_arg(1) == self.channel
            },
            _ => false
        };
        if incr_state {
            self.state += 1;
        }
        self.state == 1
    }

    fn pretty_print(&self) -> String {
        format!("IrcBundleJoinEvent({} with {} nicks)",
            self.channel.as_slice(), self.nicks.len())
    }
}

pub enum IrcEvent {
    IrcEventMessage(Box<IrcMessage>),
    IrcEventBundle(Box<IrcBundleEventInterface+Send>),
    IrcEventWatcherResponse(Box<MessageWatcher+Send>)
}


pub struct IrcConnection {
    raw_tx: SyncSender<String>,
    watchers: SyncSender<Box<MessageWatcher+Send>>,
    has_registered: bool
}


struct IrcConnectionInternalState {
    // The output stream towards the user
    event_queue_tx: SyncSender<IrcEvent>,

    // The output stream towards the server
    raw_sender: SyncSender<String>,

    // Unfinished watchers currently attached to the stream
    watchers: RingBuf<Box<MessageWatcher+Send>>,

    event_bundlers: RingBuf<Box<IrcBundleEventInterface+Send>>,

    command_mapper: PluginContainer,
    current_nick: Option<String>
}


fn watcher_accept_impl(buf: &mut RingBuf<Box<MessageWatcher+Send>>,
                       message: &IrcMessage
                      ) -> Vec<Box<MessageWatcher+Send>> {

    let mut keep_watchers: RingBuf<Box<MessageWatcher+Send>> = RingBuf::new();
    let mut finished_watchers: Vec<Box<MessageWatcher+Send>> = Vec::new();
    loop {
        match buf.pop_front() {
            Some(mut watcher) => {
                watcher.accept(message);
                if watcher.finished() {
                    finished_watchers.push(watcher);
                } else {
                    keep_watchers.push(watcher);
                }

            },
            None => break
        }
    }
    loop {
        match keep_watchers.pop_front() {
            Some(watcher) => buf.push(watcher),
            None => break
        }
    }
    finished_watchers
}


fn bundler_accept_impl(buf: &mut RingBuf<Box<IrcBundleEventInterface+Send>>,
                       message: &IrcMessage
                      ) -> Vec<Box<IrcBundleEventInterface+Send>> {
    let mut keep_watchers: RingBuf<Box<IrcBundleEventInterface+Send>> = RingBuf::new();
    let mut finished_watchers: Vec<Box<IrcBundleEventInterface+Send>> = Vec::new();

    loop {
        match buf.pop_front() {
            Some(mut watcher) => {
                if watcher.accept(message) {
                    finished_watchers.push(watcher);
                } else {
                    keep_watchers.push(watcher);
                }
            },
            None => break
        }
    }
    loop {
        match keep_watchers.pop_front() {
            Some(watcher) => buf.push(watcher),
            None => break
        }
    }
    finished_watchers
}


impl IrcConnectionInternalState {
    pub fn new(event_queue_tx: SyncSender<IrcEvent>,
               raw_sender: SyncSender<String>
              ) -> IrcConnectionInternalState {
        let watchers = RingBuf::new();
        let event_bundlers = RingBuf::new();
        let command_mapper = PluginContainer::new(String::from_str("!"));

        IrcConnectionInternalState {
            event_queue_tx: event_queue_tx,
            raw_sender: raw_sender,
            watchers: watchers,
            event_bundlers: event_bundlers,
            command_mapper: command_mapper,
            current_nick: None
        }
    }

    fn dispatch(&mut self, message: IrcMessage) {
        if message.command() == "PING" {
            let ping_body: &String = message.get_arg(0);
            self.raw_sender.send(format!("PONG :{}\n", ping_body));
        }

        if message.command() == "001" {
            let accepted_nick: &String = message.get_arg(0);
            self.current_nick = Some(accepted_nick.clone());
        }

        if message.command() == "NICK" {
            self.current_nick = match (message.source_nick(), self.current_nick.take()) {
                (Some(source_nick), Some(current_nick)) => {
                    if source_nick == current_nick {
                        Some(message.get_arg(0).clone())
                    } else {
                        Some(current_nick)
                    }
                },
                (_, any) => any
            };
        }

        match IrcBundleJoinEvent::new(&message) {
            Some(bundler) => self.event_bundlers.push(box bundler),
            None => ()
        }

        for resp in watcher_accept_impl(&mut self.watchers, &message).move_iter() {
            self.event_queue_tx.send(IrcEventWatcherResponse(resp));
        }

        for resp in bundler_accept_impl(&mut self.event_bundlers, &message).move_iter() {
            self.event_queue_tx.send(IrcEventBundle(resp));
        }

        match self.current_nick {
            Some(ref current_nick) => {
                self.command_mapper.dispatch(
                    current_nick.as_slice(), &self.raw_sender, &message);
            },
            None => ()
        }
        self.event_queue_tx.send(IrcEventMessage(box message));
    }
}


impl IrcConnection {
    pub fn new(host: &str, port: u16) -> IoResult<(IrcConnection, Receiver<IrcEvent>)> {
        let stream = match TcpStream::connect(host, port) {
            Ok(stream) => stream,
            Err(err) => return Err(err)
        };

        let (watchers_tx, watchers_rx) = sync_channel(10);
        let (event_queue_tx, event_queue_rx) = sync_channel(1024);
        let (raw_tx, raw_rx) = sync_channel::<String>(1024);
        let reader = BufferedReader::new(stream.clone());

        let tmp_stream = stream.clone();

        spawn(proc() {
            let mut writer = LineBufferedWriter::new(tmp_stream);
            for message in raw_rx.iter() {
                assert!(writer.write_str(message.append("\n").as_slice()).is_ok());
            }
        });

        let core_raw_tx = raw_tx.clone();
        
        spawn(proc() {
            let mut reader = reader;
            let mut state = IrcConnectionInternalState::new(event_queue_tx, core_raw_tx);

            state.command_mapper.register(box CtcpVersionResponderPlugin::new());
            state.command_mapper.register(box GreedPlugin::new());
            state.command_mapper.register(box SeenPlugin::new());
            state.command_mapper.register(box DeerPlugin::new());
            state.command_mapper.register(box RadioPlugin::new());

            loop {
                let string = String::from_str(match reader.read_line() {
                    Ok(string) => string,
                    Err(err) => fail!("{}", err)
                }.as_slice().trim_right());

                loop {
                    match watchers_rx.try_recv() {
                        Ok(value) => state.watchers.push(value),
                        Err(_) => break
                    };
                }

                state.dispatch(match IrcMessage::from_str(string.as_slice()) {
                    Ok(message) => message,
                    Err(err) => {
                        println!("Invalid IRC message: {} for {}", err, string);
                        continue;
                    }
                });
            }
        });

        let conn = IrcConnection {
            raw_tx: raw_tx,
            watchers: watchers_tx,
            has_registered: false
        };
        Ok((conn, event_queue_rx))
    }

    pub fn register(&mut self, nick: &str) -> Result<(), RegisterError> {
        let mut reg_watcher = RegisterMessageWatcher::new();        
        let result_rx = reg_watcher.get_monitor();
        let watcher: Box<MessageWatcher+Send> = box reg_watcher;
        self.watchers.send(watcher);

        self.write_str(format!("NICK {}", nick).as_slice());

        if !self.has_registered {
            self.write_str("USER rustbot 8 *: Rust Bot");
        }

        result_rx.recv()
    }

    pub fn join(&mut self, channel: &str) -> JoinResult {
        let mut join_watcher = JoinMessageWatcher::new(channel);
        let result_rx = join_watcher.get_monitor();
        let watcher: Box<MessageWatcher+Send> = box join_watcher;
        self.watchers.send(watcher);

        self.write_str(format!("JOIN {}", channel).as_slice());
        result_rx.recv()
    }

    pub fn write_str(&mut self, content: &str) {
        self.raw_tx.send(String::from_str(content))
    }
}
