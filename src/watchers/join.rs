use std::fmt;
use message::{IrcMessage, IrcProtocolMessage};
use watchers::base::{Bundler, BundlerTrigger, EventWatcher};
use watchers::event::{
    IrcEvent,
    IrcEventJoinBundle
};


pub type JoinResult = Result<JoinSuccess, JoinError>;


#[deriving(Clone)]
pub struct JoinSuccess {
    pub channel: String,
    pub nicks: Vec<String>,
}


#[deriving(Clone)]
pub struct JoinError {
    pub errcode: i16,
    pub message: String
}


pub struct JoinBundlerTrigger;


impl JoinBundlerTrigger {
    pub fn new() -> JoinBundlerTrigger {
        JoinBundlerTrigger
    }
}


impl BundlerTrigger for JoinBundlerTrigger {
    fn accept(&mut self, message: &IrcMessage) -> Vec<Box<Bundler+Send>> {
        let mut out = Vec::new();
        if message.command() == "JOIN" {
            let channel = message.get_arg(0);
            let bundler: Box<Bundler+Send> = box JoinBundler::new(channel.as_slice());
            out.push(bundler);
        }
        out
    }
}


pub struct JoinBundler {
    channel: String,
    nicks: Vec<String>,
    state: i16,
    result: Option<JoinResult>,
}


impl JoinBundler {
    pub fn new(channel: &str) -> JoinBundler {
        JoinBundler {
            channel: String::from_str(channel),
            nicks: Vec::new(),
            state: 0,
            result: None
        }
    }

    fn accept_state0(&mut self, message: &IrcMessage) -> Option<i16> {
        let success = message.get_command().as_slice() == "JOIN" &&
            *message.get_arg(0) == self.channel;

        let failure = message.get_command().as_slice() == "475" &&
            *message.get_arg(1) == self.channel;

        if failure {
            self.result = Some(Err(JoinError {
                errcode: 0,
                message: String::from_str("")
            }));
        }

        match (success, failure) {
            (false, false) => None,
            (false, true) => Some(-1),
            (true, false) => Some(1),
            _ => fail!("invariant invalid")
        }
    }

    fn accept_state1(&mut self, message: &IrcMessage) -> Option<i16> {
        println!("JoinBundler#1 RX: {}", message);
        // 353 contains nicks
        // 366 is ``End of /NAMES list''

        let is_nicklist = message.get_command().as_slice() == "353" &&
            *message.get_arg(2) == self.channel;

        if is_nicklist {
            for nick in message.get_arg(3).as_slice().split(' ') {
                self.nicks.push(String::from_str(nick));
            }
        }

        let is_eon = message.get_command().as_slice() == "366" && 
            *message.get_arg(1) == self.channel;

        if is_eon {
            self.result = Some(Ok(JoinSuccess {
                channel: self.channel.clone(),
                nicks: self.nicks.clone()
            }));
        }

        match is_eon {
            true => Some(2),
            false => None
        }
    }
}


impl Bundler for JoinBundler {
    fn accept(&mut self, message: &IrcMessage) -> Vec<IrcEvent> {
        let new_state = match self.state {
            0 => self.accept_state0(message),
            1 => self.accept_state1(message),
            _ => None
        };
        match new_state {
            Some(new_state) => {
                self.state = new_state;
            },
            None => ()
        }
        match self.result.take() {
            Some(result) => {
                vec![IrcEventJoinBundle(result)]
            },
            None => vec![]
        }
    }

    fn is_finished(&mut self) -> bool {
        self.state == -1 || self.state == 2
    }

}


impl fmt::Show for JoinBundler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "JoinBundler({})", self.channel.as_slice())
    }
}


/// Waits for target JoinBundleEvent and clones it down the monitor
pub struct JoinEventWatcher {
    channel: String,
    result: Option<JoinResult>,
    monitors: Vec<SyncSender<JoinResult>>,
    finished: bool
}


impl JoinEventWatcher {
    pub fn new(channel: &str) -> JoinEventWatcher {
        JoinEventWatcher {
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

    fn add_monitor(&mut self, monitor: SyncSender<JoinResult>) {
        let result = self.result.clone();

        match result {
            Some(result) => monitor.send(result.clone()),
            None => self.monitors.push(monitor)
        }
    }

    pub fn get_monitor(&mut self) -> Receiver<JoinResult> {
        let (tx, rx) = sync_channel(1);
        self.add_monitor(tx);
        rx
    }
}


impl EventWatcher for JoinEventWatcher {
    fn accept(&mut self, message: &IrcEvent) {
        match *message {
            IrcEventJoinBundle(ref result) => {
                self.result = Some(result.clone());
                self.dispatch_monitors();
            },
            _ => ()
        }
        self.finished = true;
    }

    fn is_finished(&self) -> bool {
        false
    }

    fn get_name(&self) -> &'static str {
        "JoinEventWatcher"
    }
}


