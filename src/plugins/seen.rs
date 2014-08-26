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


fn is_message_interesting(message: &IrcMessage) -> bool {
    match message.command() {
        "PRIVMSG" => true,
        "QUIT" => true,
        _ => false
    }
}


fn format_activity(nick: &str, records: &Vec<SeenRecord>) -> String {
    let mut user_has_quit: Option<Timespec> = None;
    let mut prev_message: Option<&SeenRecord> = None;

    for record in records.iter().rev() {
        if record.message.command() == "QUIT" {
            user_has_quit = Some(record.when.clone());
        }
        if record.message.is_privmsg() {
            prev_message = Some(record);
            break;
        }
    }
    let now = get_time();
    match (user_has_quit, prev_message) {
        (Some(when_quit), Some(record)) => {
            let seconds = (now - record.when).sec;
            let quit_seconds = (when_quit - record.when).sec;
            format!("{} said ``{}'' {} seconds ago before quitting {} seconds later",
                nick, record.message.get_arg(1), seconds, quit_seconds)
        },
        (None, Some(record)) => {
            // let message_sent = format_message_sent(message);
            let seconds = (now - record.when).sec;
            format!("{} said ``{}'' {} seconds ago", nick, record.message.get_arg(1), seconds)
        },
        (Some(when_quit), None) => {
            format!("{} quit {} seconds ago", nick, (now - when_quit).sec)
        },
        (None, None) => {
            format!("Sorry, I am very confused about {}", nick)
        }
    }
}


impl RustBotPlugin for SeenPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("seen");
    }

    fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        if !is_message_interesting(message) {
            return;
        }
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
            return;
        }

        let args: Vec<&str> = args[1].as_slice().splitn(1, ' ').collect();
        if args.len() != 2 {
            // Invalid command
            return;
        }

        let target_nick = String::from_str(args[1]);

        let activity = match self.map.find(&target_nick) {
            Some(val) => val,
            None => {
                m.reply(format!("{} is unknown", args[1]));
                return
            }
        };
        let activity_formatted = format_activity(target_nick.as_slice(), activity);
        m.reply(activity_formatted);
    }
}
