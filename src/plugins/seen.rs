use std::collections::TreeMap;
use std::time::Duration;

use time::{get_time, Timespec};
use irc::IrcMessage;
use irc::message_types::server;


use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
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

    pub fn get_plugin_name() -> &'static str {
        "seen"
    }
}


fn trim_vec<T>(vec: Vec<T>) -> Vec<T> {
    if vec.len() <= MAX_USER_RECORDS_KEPT {
        return vec;
    }
    let excess_elem = vec.len() - MAX_USER_RECORDS_KEPT;
    vec.into_iter().skip(excess_elem).collect()
}

enum SeenCommandType {
    Seen(String)
}


fn duration_to_string(dur: Duration) -> String {
    let days = dur.num_days();
    let hours = dur.num_hours() % 24;
    let minutes = dur.num_minutes() % 60;
    let seconds = dur.num_seconds() % 60;

    let mut string = String::new();
    if days > 0 {
        string.push_str(format!("{}d", days).as_slice());
    }
    if hours > 0 {
        string.push_str(format!("{}h", hours).as_slice());
    }
    if minutes > 0 {
        string.push_str(format!("{}m", minutes).as_slice());
    }
    if string.len() == 0 || seconds > 0 {
        string.push_str(format!("{}s", seconds).as_slice());
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
                record.message.get_args()[1],
                duration_to_string(now - record.when),
                duration_to_string(when_quit - record.when))
        },
        (None, Some(record)) => {
            // let message_sent = format_message_sent(message);
            format!(
                "{} said ``{}'' {} ago",
                nick,
                record.message.get_args()[1],
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
        conf.map_format(Format::from_str("seen {nick:s}").unwrap());
    }

    fn on_message(&mut self, message: &IrcMessage) {
        let privmsg = match *message.get_typed_message() {
            server::IncomingMsg::Privmsg(ref privmsg) => privmsg,
            _ => return
        };

        match self.map.remove(&privmsg.get_nick().to_string()) {
            Some(mut records) => {
                records.push(SeenRecord::with_now(message.clone()));
                self.map.insert(privmsg.get_nick().to_string(), trim_vec(records));
            },
            None => {
                let records = vec![SeenRecord::with_now(message.clone())];
                self.map.insert(privmsg.get_nick().to_string(), records);
            }
        }
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let privmsg = match *message.get_typed_message() {
            server::IncomingMsg::Privmsg(ref privmsg) => privmsg,
            _ => return
        };

        // Hacky is-channel
        if !privmsg.get_target().starts_with("#") {
            return
        }
        let source_nick = privmsg.get_nick();

        let command_phrase = m.command();
        
        let parsed_command = match command_phrase.command[] {
            "seen" => match command_phrase.get("nick") {
                Some(nick) => Some(SeenCommandType::Seen(nick)),
                None => None
            },
            _ => None
        };

        match parsed_command {
            Some(SeenCommandType::Seen(target_nick)) => {
                if source_nick[] == target_nick.as_slice() {
                    m.reply(format!("Looking for yourself, {}?", source_nick));
                    return;
                }

                if m.current_nick() == target_nick[] {
                    m.reply(format!("You found me, {}!", source_nick));
                    return;
                }
                let activity = match self.map.get(&target_nick) {
                    Some(val) => val,
                    None => {
                        m.reply(format!("{} is unknown", target_nick));
                        return
                    }
                };
                m.reply(format_activity(target_nick[], activity));
            },
            None => return
        }
    }
}
