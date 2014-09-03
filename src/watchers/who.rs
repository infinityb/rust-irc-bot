use std::fmt;
use std::collections::{TreeMap, RingBuf, Deque};

use watchers::base::{Bundler, BundlerTrigger, EventWatcher};
use watchers::event::{IrcEvent, IrcEventMessage, IrcEventWhoBundle};

use message::{
    IrcMessage,
    IrcProtocolMessage,
    IrcHostmaskPrefix,
    IrcOtherPrefix,
};


pub type WhoResult = Result<WhoSuccess, WhoError>;


#[deriving(Clone)]
pub struct WhoSuccess {
    pub channel: String,
    pub who_records: Vec<WhoRecord>,
}

impl WhoSuccess {
    fn from_bundler(bundler: WhoBundler) -> WhoSuccess {
        WhoSuccess {
            channel: bundler.target_channel,
            who_records: bundler.who_records
        }
    }
}


// Does /WHO even error? 
#[deriving(Clone)]
pub struct WhoError;


#[deriving(Clone)]
struct WhoRecord {
    hostname: String,
    server: String,
    nick: String,
    rest: String
}


impl WhoRecord {
    fn new(args: &Vec<String>) -> Option<WhoRecord> {
        match args.as_slice() {
            [ref _self_nick, ref _channel, ref hostname,
             ref server, ref nick, ref _unk, ref rest
            ] => {
                Some(WhoRecord {
                    hostname: hostname.clone(),
                    server: server.clone(),
                    nick: nick.clone(),
                    rest: rest.clone()
                })
            },
            _ => None
        }
    }
}


#[deriving(Clone)]
pub struct WhoBundler {
    target_channel: String,
    who_records: Vec<WhoRecord>,
    finished: bool
}


impl WhoBundler {
    pub fn new(channel: &str) -> WhoBundler {
        WhoBundler {
            target_channel: String::from_str(channel),
            who_records: vec![],
            finished: false
        }
    }

    fn add_record(&mut self, args: &Vec<String>) {
        match WhoRecord::new(args) {
            Some(who_rec) => {
                self.who_records.push(who_rec);
            },
            None => ()
        }
    }
}


impl Bundler for WhoBundler {
    fn on_message(&mut self, message: &IrcMessage) -> Vec<IrcEvent> {
        if message.get_args().len() < 2 {
            return Vec::new();
        }
        if message.get_arg(1).as_slice() != self.target_channel.as_slice() {
            return Vec::new();
        }
        match *message.get_message() {
            IrcProtocolMessage::Numeric(352, ref message) => {
                self.add_record(message);
                Vec::new()
            },
            IrcProtocolMessage::Numeric(315, ref message) => {
                self.finished = true;
                let mut out = Vec::new();
                out.push(IrcEventWhoBundle(Ok(WhoSuccess::from_bundler(self.clone()))));
                out
            },
            _ => Vec::new()
        }
    }

    fn is_finished(&mut self) -> bool {
        self.finished
    }
}


/// Waits for target WhoBundleEvent and clones it down the monitor
pub struct WhoEventWatcher {
    channel: String,
    result: Option<WhoResult>,
    monitors: Vec<SyncSender<WhoResult>>,
    finished: bool
}


impl WhoEventWatcher {
    pub fn new(channel: &str) -> WhoEventWatcher {
        WhoEventWatcher {
            channel: String::from_str(channel),
            monitors: Vec::new(),
            result: None,
            finished: false
        }
    }

    fn dispatch_monitors(&mut self) {
        let result = self.result.clone().unwrap();
        for monitor in self.monitors.iter() {
            match monitor.try_send(result.clone()) {
                Ok(_) => (),
                Err(_) => fail!("sending failed")
            }
        }
        self.monitors = Vec::new();
    }

    fn add_monitor(&mut self, monitor: SyncSender<WhoResult>) {
        let result = self.result.clone();

        match result {
            Some(result) => monitor.send(result.clone()),
            None => self.monitors.push(monitor)
        }
    }

    pub fn get_monitor(&mut self) -> Receiver<WhoResult> {
        let (tx, rx) = sync_channel(1);
        self.add_monitor(tx);
        rx
    }
}

impl EventWatcher for WhoEventWatcher {
    fn on_event(&mut self, message: &IrcEvent) {
        match message {
            &IrcEventWhoBundle(ref result) => {
                self.result = Some(result.clone());
                self.dispatch_monitors();
                self.finished = true;
            },
            _ => ()
        }
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn get_name(&self) -> &'static str {
        "WhoEventWatcher"
    }
}



// pub struct IrcUser {
//     nick: String,
//     username: String,
//     hostname: String,
// }


// pub struct IrcChannel {
//     name: String,
//     users: Vec<IrcUser>
// }


// pub struct IrcStatePlugin {
//     channels: TreeMap<String, IrcChannel>,
//     users: TreeMap<String, IrcUser>,
//     who_bundlers: RingBuf<WhoBundler>
// }


// impl IrcStatePlugin {
//     pub fn new() -> IrcStatePlugin {
//         IrcStatePlugin {
//             channels: TreeMap::new(),
//             users: TreeMap::new(),
//             who_bundlers: RingBuf::new()
//         }
//     }

//     fn update(&mut self, message: &IrcMessage) {
//         match message.get_prefix() {
//             Some(&IrcHostmaskPrefix(ref mask)) => {
//                 println!("{}", mask);
//             },
//             Some(&IrcOtherPrefix(ref other)) => {
//                 println!("{}", other);
//             },
//             None => ()
//         }
//     }

//     fn add_channel(&mut self, channel: IrcChannel) {
//         self.channels.insert(channel.name.clone(), channel);
//     }
// }


// impl RustBotPlugin for IrcStatePlugin {
//     fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
//         // If we find a JOIN message:
//         //   * Attach a WhoBundler to our listener buffer
//         //   * send a WHO to that channel
//         //   * on completion of the WhoBundler, update states.
//     }
// }
