use std::collections::{RingBuf, Deque};
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

use message::IrcMessage;
use watchers::{
    MessageWatcher,
    ConnectMessageWatcher,
    JoinMessageWatcher,
};


pub enum IrcEvent {
    IrcEventMessage(Box<IrcMessage>),
    IrcEventWatcherResponse(Box<MessageWatcher+Send>)
}


pub struct IrcConnection {
    // conn: TcpStream,
    writer: LineBufferedWriter<TcpStream>,
    event_queue: Receiver<IrcEvent>,
    watchers: SyncSender<Box<MessageWatcher+Send>>,
}


impl IrcConnection {
    pub fn new(host: &str, port: u16) -> IoResult<IrcConnection> {
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
                                        event_queue_tx.send(IrcEventWatcherResponse(watcher));
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
                        event_queue_tx.send(IrcEventMessage(box message));
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

    pub fn register(&mut self, nick: &str) { // -> Box<MessageWatcher> {
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

    pub fn join(&mut self, channel: &str) -> () {
        let watcher: Box<MessageWatcher+Send> = box JoinMessageWatcher::new(channel);
        self.watchers.send(watcher);

        match self.writer.write_str(format!("JOIN {}\n", channel).as_slice()) {
            Ok(_) => (),
            Err(err) => fail!("Error writing to IRC server: {}", err)
        }
    }

    pub fn recv(&self) -> IrcEvent {
        self.event_queue.recv()
    }

    pub fn write_str(&mut self, content: &str) -> IoResult<()> {
        self.writer.write_str(content)
    }
}