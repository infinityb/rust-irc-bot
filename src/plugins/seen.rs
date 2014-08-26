use std::collections::TreeMap;
use time::{get_time, Timespec};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};

static MAX_USER_RECORDS_KEPT: uint = 5;

pub struct SeenRecord {
    when: Timespec,
    message: IrcMessage
}


impl SeenRecord {
    fn new(when: Timespec, message: IrcMessage) -> SeenRecord {
        SeenRecord {
            when: when,
            message: message
        }
    }

    fn with_now(message: IrcMessage) -> SeenRecord {
        SeenRecord::new(get_time(), message)
    }
}


pub struct SeenPlugin {
    map: TreeMap<String, Vec<SeenRecord>>,
}


impl SeenPlugin {
    pub fn new() -> SeenPlugin {
        SeenPlugin {
            map: TreeMap::new()
        }
    }
}


fn trim_vec<T>(vec: Vec<T>) -> Vec<T> {
    if vec.len() <= MAX_USER_RECORDS_KEPT {
        return vec;
    }
    let excess_elem = vec.len() - MAX_USER_RECORDS_KEPT;
    vec.move_iter().skip(excess_elem).collect()
}


impl RustBotPlugin for SeenPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("seen");
    }

    fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let source_nick = match message.source_nick() {
            Some(source_nick) => source_nick,
            None => return
        };
        match self.map.pop(&source_nick) {
            Some(mut records) => {
                records.push(SeenRecord::with_now(message.clone()));
                self.map.insert(source_nick, trim_vec(records));
            },
            None => {
                let mut records = vec![SeenRecord::with_now(message.clone())];
                self.map.insert(source_nick, records);
            }
        }
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        if !message.target_is_channel() {
            return;
        }
        if !message.is_privmsg() {
            return;
        }
        
        let args = message.get_args();
        if args.len() != 2 {
            // Invalid PRIVMSG
        }

        let args: Vec<&str> = args[1].as_slice().splitn(1, ' ').collect();
        if args.len() != 2 {
            // Invalid command
        }

        let target_nick = String::from_str(args[1]);
        match self.map.find(&target_nick) {
            Some(val) => {
                m.reply(format!("{} has {} activity records", args[1], val.len()));
            },
            None => {
                m.reply(format!("{} is unknown", args[1]));
            }
        }
        
    }
}
