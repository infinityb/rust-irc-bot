use std::fmt;
use message::{IrcMessage};


pub trait MessageWatcher {
    fn accept(&mut self, message: &IrcMessage);
    fn finished(&self) -> bool;
    fn pretty_print(&self) -> String;
}

pub struct ConnectMessageWatcher {
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
        println!("connect watcher ok");
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


pub struct JoinMessageWatcher {
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
        format!("JoinMessageWatcher({} with {} nicks)",
            self.channel.as_slice(), self.nicks.len())
    }
}

impl fmt::Show for JoinMessageWatcher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "JoinMessageWatcher({})", self.channel.as_slice())
    }
}

pub enum IrcStateError {
    NotConnected,
    InvalidPhase(String),
}
