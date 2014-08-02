#![allow(dead_code)]

extern crate irc;
extern crate debug;

use std::fmt;
use std::collections::{RingBuf, Deque};
use std::comm::channel;
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

use irc::IrcMessage;


trait MessageWatcher {
    fn accept(&mut self, message: &IrcMessage);
    fn finished(&self) -> bool;
    fn pretty_print(&self) -> String;
}

struct ConnectMessageWatcher {
    rx_connect: bool,
}

impl ConnectMessageWatcher {
    pub fn new() -> ConnectMessageWatcher {
        ConnectMessageWatcher { rx_connect: false }
    }
}

impl MessageWatcher for ConnectMessageWatcher {
    fn accept(&mut self, message: &IrcMessage) {
        if message.get_command().as_slice() == "001" {
            self.rx_connect = true;
        }
    }

    fn finished(&self) -> bool {
       self.rx_connect
    }

    fn pretty_print(&self) -> String {
        format!("ConnectMessageWatcher()")
    }
}

impl fmt::Show for ConnectMessageWatcher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ConnectMessageWatcher()")
    }
}


struct JoinMessageWatcher {
    channel: String,
    nicks: Vec<String>,
    state: u16
}

impl JoinMessageWatcher {
    pub fn new(channel: &str) -> JoinMessageWatcher {
        JoinMessageWatcher {
            channel: String::from_str(channel),
            nicks: Vec::new(),
            state: 0
        }
    }
}

impl MessageWatcher for JoinMessageWatcher {
    fn accept(&mut self, message: &IrcMessage) {
        let incr_state: bool = match self.state {
            0 => {
                message.get_command().as_slice() == "JOIN" &&
                    *message.get_arg(0) == self.channel
            },
            1 => {

                // 353 contains nicks
                // 366 is ``End of /NAMES list''
                if message.get_command().as_slice() == "353" &&
                        *message.get_arg(2) == self.channel {
                    for nick in message.get_arg(3).as_slice().split(' ') {
                        self.nicks.push(String::from_str(nick));
                    }
                };
                message.get_command().as_slice() == "366" &&
                    *message.get_arg(1) == self.channel
            },
            _ => false
        };
        if incr_state {
            self.state += 1;
        }
    }

    fn finished(&self) -> bool {
       self.state == 2
    }

    fn pretty_print(&self) -> String {
        format!("JoinMessageWatcher({:?} with {} nicks)",
            self.channel.as_slice(), self.nicks.len())
    }
}

impl fmt::Show for JoinMessageWatcher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "JoinMessageWatcher({:?})", self.channel.as_slice())
    }
}

enum IrcStateError {
    NotConnected,
    InvalidPhase(String),
}

enum IrcEvent {
    Message(Box<IrcMessage>),
    WatcherResponse(Box<MessageWatcher+Send>)
}


struct IrcConnection {
    // conn: TcpStream,
    writer: LineBufferedWriter<TcpStream>,
    event_queue: Receiver<IrcEvent>,
    watchers: SyncSender<Box<MessageWatcher+Send>>,
}


impl IrcConnection {
    fn new(host: &str, port: u16) -> IoResult<IrcConnection> {
        let stream = match TcpStream::connect(host, port) {
            Ok(stream) => stream,
            Err(err) => return Err(err)
        };

        let (watchers_tx, watchers_rx) = sync_channel(0);
        let (event_queue_tx, event_queue_rx) = sync_channel(10);
        let reader = BufferedReader::new(stream.clone());

        spawn(proc() {
            let mut watchers: RingBuf<Box<MessageWatcher+Send>> = RingBuf::new();
            let mut reader = reader;

            loop {
                let string = String::from_str(match reader.read_line() {
                    Ok(string) => string,
                    Err(err) => fail!("{}", err)
                }.as_slice().trim_right());

                loop {
                    match watchers_rx.try_recv() {
                        Ok(value) => watchers.push(value),
                        Err(_) => break
                    };
                }
                match IrcMessage::from_str(string.as_slice()) {
                    Ok(message) => {
                        let mut keep_watchers: RingBuf<Box<MessageWatcher+Send>> = RingBuf::new();
                        loop {
                            match watchers.pop_front() {
                                Some(mut watcher) => {
                                    watcher.accept(&message);
                                    if watcher.finished() {
                                        event_queue_tx.send(WatcherResponse(watcher));
                                    } else {
                                        keep_watchers.push(watcher);
                                    }
                                },
                                None => break
                            }
                        }
                        loop {
                            match keep_watchers.pop_front() {
                                Some(watcher) => watchers.push(watcher),
                                None => break
                            }
                        }
                        event_queue_tx.send(Message(box message));
                    },
                    Err(err) => {
                        println!("Invalid IRC message: {} for {}", err, string);
                    }
                };
            }
        });

        Ok(IrcConnection {
            // conn: stream.clone(),
            writer: LineBufferedWriter::new(stream.clone()),
            event_queue: event_queue_rx,
            watchers: watchers_tx
        })
    }

    fn register(&mut self, nick: &str) { // -> Box<MessageWatcher> {
        let watcher: Box<MessageWatcher+Send> = box ConnectMessageWatcher::new();
        self.watchers.send(watcher);

        match self.writer.write_str(format!("NICK {}\n", nick).as_slice()) {
            Ok(_) => (),
            Err(err) => fail!("Error writing to IRC server: {}", err)
        };

        match self.writer.write_str("USER rustbot 8 *: Rust Bot\n") {
            Ok(_) => (),
            Err(err) => fail!("Error writing to IRC server: {}", err)
        };
    }

    fn join(&mut self, channel: &str) -> () {
        let watcher: Box<MessageWatcher+Send> = box JoinMessageWatcher::new(channel);
        self.watchers.send(watcher);

        match self.writer.write_str(format!("JOIN {}\n", channel).as_slice()) {
            Ok(_) => (),
            Err(err) => fail!("Error writing to IRC server: {}", err)
        }
    }
}

fn main() {
    let mut conn = match IrcConnection::new("127.0.0.1", 6667) {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    conn.register("platy");

    loop {      
        match conn.event_queue.recv() {
            Message(message) => {
                println!("RX: {}", message);
                if message.get_command().as_slice() == "001" {
                    conn.join("#dicks");
                }
                if message.get_command().as_slice() == "PING" {
                    let response = format!("PONG :{}\n", message.get_arg(0));
                    println!("TX: {}", response.as_slice());
                    match conn.writer.write_str(response.as_slice()) {
                        Ok(_) => (),
                        Err(err) => fail!("Error writing to IRC server: {}", err)
                    }
                }
            },
            WatcherResponse(watcher) => {
                println!("got watcher back: {}", watcher.pretty_print());
            }
        }
    }
}
