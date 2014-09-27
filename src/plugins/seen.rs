use std::collections::TreeMap;
use std::time::Duration;
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
    vec.into_iter().skip(excess_elem).collect()
}


fn is_message_interesting(message: &IrcMessage) -> bool {
    match message.command() {
        // "JOIN" => true,
        // "ACTION" => true,
        "PRIVMSG" => true,
        "QUIT" => true,
        _ => false
    }
}

enum SeenCommandType<'a> {
    Seen(&'a str)
}


fn parse_seen<'a>(message: &'a IrcMessage) -> Option<SeenCommandType<'a>> {
    let message_body = message.get_arg(1).as_slice();
    match message_body.find(' ') {
        Some(idx) => Some(Seen(message_body.slice_from(idx + 1))),
        None => None
    }
}


fn parse_command<'a>(m: &CommandMapperDispatch, message: &'a IrcMessage) -> Option<SeenCommandType<'a>> {
    match m.command() {
        Some("seen") => parse_seen(message),
        Some(_) => None,
        None => None
    }
}

fn duration_to_string(dur: Duration) -> String {
    let days = dur.num_days();
    let hours = dur.num_hours() % 24;
    let minutes = dur.num_minutes() % 60;
    let seconds = dur.num_seconds() % 60;

    let mut string = String::new();
    if days > 0 {
        string.push_str(format!("{:d}d", days).as_slice());
    }
    if hours > 0 {
        string.push_str(format!("{:02d}h", hours).as_slice());
    }
    if minutes > 0 {
        string.push_str(format!("{:02d}m", minutes).as_slice());
    }
    if string.len() == 0 || seconds > 0 {
        string.push_str(format!("{:02d}s", seconds).as_slice());
    }
    string
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
            format!(
                "{} said ``{}'' {} ago before quitting {} later",
                nick,
                record.message.get_arg(1),
                duration_to_string(now - record.when),
                duration_to_string(when_quit - record.when))
        },
        (None, Some(record)) => {
            // let message_sent = format_message_sent(message);
            format!(
                "{} said ``{}'' {} ago",
                nick,
                record.message.get_arg(1),
                duration_to_string(now - record.when))
        },
        (Some(when_quit), None) => {
            format!(
                "{} quit {} seconds ago",
                nick,
                duration_to_string(now - when_quit))
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

    fn accept(&mut self, _m: &CommandMapperDispatch, message: &IrcMessage) {
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
                let records = vec![SeenRecord::with_now(message.clone())];
                self.map.insert(source_nick, records);
            }
        }
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let source_nick = match message.source_nick() {
            Some(source_nick) => source_nick,
            None => return
        };

        if !message.target_is_channel() {
            return;
        }

        if !message.is_privmsg() {
            return;
        }
        
        match parse_command(m, message) {
            Some(Seen(target_nick)) => {
                if source_nick.as_slice() == target_nick {
                    m.reply(format!("Looking for yourself, {}?", source_nick));
                    return;
                }

                if m.current_nick() == target_nick {
                    m.reply(format!("You found me, {}!", source_nick));
                    return;
                }
                let target_nick_str = String::from_str(target_nick);
                let activity = match self.map.find(&target_nick_str) {
                    Some(val) => val,
                    None => {
                        m.reply(format!("{} is unknown", target_nick));
                        return
                    }
                };
                m.reply(format_activity(target_nick.as_slice(), activity));
            },
            None => return
        }
    }
}
