use std::fmt;
use message::{IrcMessage, IrcProtocolMessage};

use numerics;


pub trait MessageWatcher {
    fn accept(&mut self, message: &IrcMessage);
    fn finished(&self) -> bool;
    fn pretty_print(&self) -> String;
}


#[deriving(Clone)]
pub struct RegisterError {
    pub errtype: RegisterErrorType,
    pub message: IrcMessage
}


impl RegisterError {
    pub fn should_pick_new_nickname(&self) -> bool {
        match *self.message.get_message() {
            IrcProtocolMessage::Numeric(num, _) => {
                numerics::ERR_NICKNAMEINUSE == (num as i32)
            },
            _ => false
        }
    }
}


pub type RegisterErrorType = self::RegisterErrorType::RegisterErrorType;
pub mod RegisterErrorType {
    use numerics;

    #[deriving(Clone, Send)]
    pub enum RegisterErrorType {
        NoNicknameGiven,
        NicknameInUse,
        UnavailableResource,
        ErroneousNickname,
        NicknameCollision,
        Restricted,
        Unknown(i32)
    }

    pub fn is_known_error(result: i32) -> bool {
        match from_ord(result) {
            Unknown(_) => false,
            _ => true
        }
    }

    pub fn from_ord_known(result: i32) -> Option<RegisterErrorType> {
        match result {
            numerics::ERR_NONICKNAMEGIVEN => Some(NoNicknameGiven),
            numerics::ERR_NICKNAMEINUSE => Some(NicknameInUse),
            numerics::ERR_UNAVAILRESOURCE => Some(UnavailableResource),
            numerics::ERR_ERRONEUSNICKNAME => Some(ErroneousNickname),
            numerics::ERR_NICKCOLLISION => Some(NicknameCollision),
            numerics::ERR_RESTRICTED => Some(Restricted),
            _ => None
        }
    }

    pub fn from_ord(result: i32) -> RegisterErrorType {
        match from_ord_known(result) {
            Some(retval) => retval,
            None => Unknown(result)
        }
    }
}


pub struct RegisterMessageWatcher {
    rx_connect: bool,
    result: Option<Result<(), RegisterError>>,
    monitors: Vec<SyncSender<Result<(), RegisterError>>>
}


impl RegisterMessageWatcher {
    pub fn new() -> RegisterMessageWatcher {
        RegisterMessageWatcher {
            rx_connect: false,
            result: None,
            monitors: Vec::new()
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

    fn add_monitor(&mut self, monitor: SyncSender<Result<(), RegisterError>>) {
        let result = self.result.clone();

        match (self.rx_connect, result) {
            (true, Some(result)) => {
                monitor.send(result.clone());
            },
            (true, None) => {
                fail!("rx_connect without result");
            },
            (false, Some(_)) => {
                fail!("result without rx_connect");
            },
            (false, None) => {
                self.monitors.push(monitor);
            }
        };
    }

    pub fn get_monitor(&mut self) -> Receiver<Result<(), RegisterError>> {
        let (tx, rx) = sync_channel(1);
        self.add_monitor(tx);
        rx
    }
}


impl MessageWatcher for RegisterMessageWatcher {
    fn accept(&mut self, message: &IrcMessage) {     
        let (interested, err) = match *message.get_message() {
            IrcProtocolMessage::Numeric(1, _) => {
                (true, None)
            },
            IrcProtocolMessage::Numeric(other, _) => {
                let res = RegisterErrorType::from_ord_known(other as i32);
                (res.is_some(), res)
            },
            _ => (false, None)
        };

        if interested {
            self.rx_connect = true;
            self.result = Some(match err {
                None => Ok(()),
                Some(err) => Err(RegisterError {
                    errtype: err,
                    message: message.clone()
                })
            });
            self.dispatch_monitors();
        }
    }

    fn finished(&self) -> bool {
       self.rx_connect
    }

    fn pretty_print(&self) -> String {
        format!("RegisterMessageWatcher()")
    }
}


impl fmt::Show for RegisterMessageWatcher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RegisterMessageWatcher()")
    }
}


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


pub struct JoinMessageWatcher {
    channel: String,
    nicks: Vec<String>,
    state: i16,
    result: Option<JoinResult>,
    monitors: Vec<SyncSender<JoinResult>>
    // finished: bool,
    // trans: Vec<(i16, i16)>
}


impl JoinMessageWatcher {
    pub fn new(channel: &str) -> JoinMessageWatcher {
        JoinMessageWatcher {
            channel: String::from_str(channel),
            nicks: Vec::new(),
            state: 0,
            result: None,
            monitors: Vec::new()
            // trans: vec![(0, 1), (0, -1),
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

    fn is_finished_state(state: i16) -> bool {
        state == -1 || state == 2
    }

    fn accept_state0(&mut self, message: &IrcMessage) -> Option<i16> {
        println!("JoinMessageWatcher#0 RX: {}", message);

        let success = message.get_command().as_slice() == "JOIN" &&
            *message.get_arg(0) == self.channel;

        let failure = message.get_command().as_slice() == "475" &&
            *message.get_arg(1) == self.channel;

        if failure {
            self.result = Some(Err(JoinError {
                errcode: 0,
                message: String::from_str("")
            }));
            self.dispatch_monitors();
        }

        match (success, failure) {
            (false, false) => None,
            (false, true) => Some(-1),
            (true, false) => Some(1),
            _ => fail!("invariant invalid")
        }
    }

    fn accept_state1(&mut self, message: &IrcMessage) -> Option<i16> {
        println!("JoinMessageWatcher#1 RX: {}", message);
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
            self.dispatch_monitors();
        }

        match is_eon {
            true => Some(2),
            false => None
        }
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


impl MessageWatcher for JoinMessageWatcher {
    fn accept(&mut self, message: &IrcMessage) {
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
    }

    fn finished(&self) -> bool {
       JoinMessageWatcher::is_finished_state(self.state)
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
