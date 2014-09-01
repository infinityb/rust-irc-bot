use std::fmt;

use numerics;
use message::{IrcMessage, IrcProtocolMessage};
use watchers::base::{
    MessageWatcher,
    EventWatcher,
    Bundler,
    BundlerTrigger
};
use watchers::event::{
    IrcEvent,
    IrcEventMessage,
};


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
    }

    pub fn is_known_error(result: i32) -> bool {
        from_ord_known(result).is_some()
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
}


pub struct RegisterEventWatcher {
    rx_connect: bool,
    result: Option<Result<(), RegisterError>>,
    monitors: Vec<SyncSender<Result<(), RegisterError>>>
}


impl RegisterEventWatcher {
    pub fn new() -> RegisterEventWatcher {
        RegisterEventWatcher {
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

    fn accept_ircmessage(&mut self, message: &IrcMessage) {
        println!("RegisterEventWatcher: RX {}", message);
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
}


impl EventWatcher for RegisterEventWatcher {
    fn accept(&mut self, event: &IrcEvent) {
        match *event {
            IrcEventMessage(ref message) => {
                self.accept_ircmessage(message);
            },
            _ => ()
        }
    }

    fn is_finished(&self) -> bool {
       self.rx_connect
    }

    fn get_name(&self) -> &'static str {
        "RegisterEventWatcher"
    }
}


impl fmt::Show for RegisterEventWatcher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RegisterEventWatcher()")
    }
}


pub enum IrcStateError {
    NotConnected,
    InvalidPhase(String),
}
