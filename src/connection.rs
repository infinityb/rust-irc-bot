use std::collections::{RingBuf, Deque};
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};
use std::default::Default;
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
    Bundler,
    RegisterError,
    RegisterEventWatcher,
    JoinBundlerTrigger,
    JoinBundler,
    JoinResult,
    JoinEventWatcher,
    WhoBundler,
    WhoResult,
    WhoEventWatcher,
    EventWatcher,
    BundlerTrigger,
    IrcEvent,
    IrcEventMessage,
    IrcEventWatcherResponse
};


pub struct IrcConnection {
    raw_tx: SyncSender<String>,
    command_queue: SyncSender<IrcConnectionCommand>,
    has_registered: bool
}


struct IrcConnectionInternalState {
    // The output stream towards the user
    event_queue_tx: SyncSender<IrcEvent>,

    // The output stream towards the server
    raw_sender: SyncSender<String>,

    // Handles plugins and their command registrations
    command_mapper: PluginContainer,

    // Unfinished watchers currently attached to the stream
    event_watchers: RingBuf<Box<EventWatcher+Send>>,

    // Active event bundlers.
    event_bundlers: RingBuf<Box<Bundler+Send>>,

    // Bundler triggers.  They create Bundlers.
    bundler_triggers: Vec<Box<BundlerTrigger+Send>>,

    // Current nickname held by the client
    current_nick: Option<String>
}


fn bundler_trigger_impl(triggers: &mut Vec<Box<BundlerTrigger+Send>>,
                       message: &IrcMessage
                      ) -> Vec<Box<Bundler+Send>> {

    let mut activating: Vec<Box<Bundler+Send>> = Vec::new();
    for trigger in triggers.mut_iter() {
        let mut new_bundlers = trigger.on_message(message);

        activating.reserve_additional(new_bundlers.len());
        for bundler in new_bundlers.move_iter() {
            activating.push(bundler);
        }
    }
    activating
}


fn watcher_accept_impl(buf: &mut RingBuf<Box<EventWatcher+Send>>,
                       event: &IrcEvent
                      ) -> Vec<Box<EventWatcher+Send>> {
    let mut keep_watchers: RingBuf<Box<EventWatcher+Send>> = RingBuf::new();
    let mut finished_watchers: Vec<Box<EventWatcher+Send>> = Vec::new();

    loop {
        match buf.pop_front() {
            Some(mut watcher) => {
                watcher.on_event(event);
                if watcher.is_finished() {
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


fn bundler_accept_impl(buf: &mut RingBuf<Box<Bundler+Send>>,
                       message: &IrcMessage
                      ) -> Vec<IrcEvent> {

    let mut keep_bundlers: RingBuf<Box<Bundler+Send>> = RingBuf::new();
    let mut emit_events: Vec<IrcEvent> = Vec::new();

    loop {
        match buf.pop_front() {
            Some(mut bundler) => {
                for event in bundler.on_message(message).move_iter() {
                    emit_events.push(event);
                }
                if !bundler.is_finished() {
                    keep_bundlers.push(bundler);
                }
            },
            None => break
        }
    }
    loop {
        match keep_bundlers.pop_front() {
            Some(watcher) => buf.push(watcher),
            None => break
        }
    }
    emit_events
}


impl IrcConnectionInternalState {
    pub fn new(event_queue_tx: SyncSender<IrcEvent>,
               raw_sender: SyncSender<String>
              ) -> IrcConnectionInternalState {

        IrcConnectionInternalState {
            event_queue_tx: event_queue_tx,
            raw_sender: raw_sender,
            command_mapper: PluginContainer::new(String::from_str("!")),

            event_watchers: Default::default(),
            event_bundlers: Default::default(),
            current_nick: Default::default(),
            bundler_triggers: Default::default()
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

        let mut outgoing_events: Vec<IrcEvent> = Vec::new();

        for new_bundler in bundler_trigger_impl(&mut self.bundler_triggers, &message).move_iter() {
            self.event_bundlers.push(new_bundler);
        }

        for event in bundler_accept_impl(&mut self.event_bundlers, &message).move_iter() {
            outgoing_events.push(event);
        }

        outgoing_events.push(IrcEventMessage(message.clone()));

        for event in outgoing_events.iter() {
            for watcher in watcher_accept_impl(&mut self.event_watchers, event).move_iter() {
                drop(watcher);
            }
        }

        match self.current_nick {
            Some(ref current_nick) => {
                self.command_mapper.dispatch(
                    current_nick.as_slice(), &self.raw_sender, &message);
            },
            None => ()
        }

        for event in outgoing_events.move_iter() {
            self.event_queue_tx.send(event);
        }
    }
}


pub enum IrcConnectionCommand {
    AddWatcher(Box<EventWatcher+Send>),
    AddBundler(Box<Bundler+Send>),
}


impl IrcConnection {
    pub fn new(host: &str, port: u16) -> IoResult<(IrcConnection, Receiver<IrcEvent>)> {
        let stream = match TcpStream::connect(host, port) {
            Ok(stream) => stream,
            Err(err) => return Err(err)
        };

        let (command_queue_tx, command_queue_rx) = sync_channel::<IrcConnectionCommand>(10);
        let (event_queue_tx, event_queue_rx) = sync_channel(1024);
        let (raw_tx, raw_rx) = sync_channel::<String>(1024);
        let reader = BufferedReader::new(stream.clone());

        let tmp_stream = stream.clone();

        spawn(proc() {
            let mut writer = LineBufferedWriter::new(tmp_stream);
            for message in raw_rx.iter() {
                // println!("TX: {}", message.as_slice());
                assert!(writer.write_str(message.append("\n").as_slice()).is_ok());
            }
        });

        let core_raw_tx = raw_tx.clone();
        
        spawn(proc() {
            let mut reader = reader;
            let mut state = IrcConnectionInternalState::new(event_queue_tx, core_raw_tx);

            state.bundler_triggers.push(box JoinBundlerTrigger::new());

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
                // println!("RX: {}", string.as_slice());

                loop {
                    match command_queue_rx.try_recv() {
                        Ok(AddWatcher(value)) => {
                            state.event_watchers.push(value);
                        },
                        Ok(AddBundler(value)) => {
                            state.event_bundlers.push(value);
                        },
                        Err(_) => break
                    }
                }
                // let bundler: Box<Bundler+Send> = box WhoBundler::new(target);
                // self.event_bundlers.send(bundler);

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
            command_queue: command_queue_tx,
            has_registered: false
        };
        Ok((conn, event_queue_rx))
    }

    pub fn register(&mut self, nick: &str) -> Result<(), RegisterError> {
        let mut reg_watcher = RegisterEventWatcher::new();        
        let result_rx = reg_watcher.get_monitor();
        let watcher: Box<EventWatcher+Send> = box reg_watcher;
        self.command_queue.send(AddWatcher(watcher));
        self.write_str(format!("NICK {}", nick).as_slice());
        if !self.has_registered {
            self.write_str("USER rustbot 8 *: Rust Bot");
        }
        result_rx.recv()
    }

    pub fn join(&mut self, channel: &str) -> JoinResult {
        let mut join_watcher = JoinEventWatcher::new(channel);
        let result_rx = join_watcher.get_monitor();
        let watcher: Box<EventWatcher+Send> = box join_watcher;
        self.command_queue.send(AddWatcher(watcher));
        self.write_str(format!("JOIN {}", channel).as_slice());
        result_rx.recv()
    }

    pub fn who(&mut self, target: &str) -> WhoResult {
        let mut who_watcher = WhoEventWatcher::new(target);
        let result_rx = who_watcher.get_monitor();
        let watcher: Box<EventWatcher+Send> = box who_watcher;
        self.command_queue.send(AddBundler(box WhoBundler::new(target)));
        self.command_queue.send(AddWatcher(watcher));
        self.write_str(format!("WHO {}", target).as_slice());
        result_rx.recv()
    }

    pub fn write_str(&mut self, content: &str) {
        self.raw_tx.send(String::from_str(content))
    }
}
