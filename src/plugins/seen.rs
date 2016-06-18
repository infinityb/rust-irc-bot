use std::collections::BTreeMap;

use time::{get_time, Timespec};
use time::Duration;

use irc::{IrcMsg, IrcMsgBuf, server};
use irc::legacy::IrcMsg as IrcMsgLegacy;

use utils::formatting::duration_to_string;
use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
    Replier,
};

const CMD_SEEN: Token = Token(0);

static MAX_USER_RECORDS_KEPT: usize = 5;

pub struct SeenRecord {
    when: Timespec,
    message: IrcMsgBuf,
}


impl SeenRecord {
    fn new(when: Timespec, message: IrcMsgBuf) -> SeenRecord {
        SeenRecord {
            when: when,
            message: message
        }
    }

    fn with_now(message: IrcMsg) -> SeenRecord {
        SeenRecord::new(get_time(), message)
    }
}


pub struct SeenPlugin {
    map: BTreeMap<String, Vec<SeenRecord>>,
}


impl SeenPlugin {
    pub fn new() -> SeenPlugin {
        SeenPlugin {
            map: BTreeMap::new()
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

fn format_activity(nick: &str, records: &Vec<SeenRecord>) -> String {
    let mut user_has_quit: Option<Timespec> = None;
    let mut prev_message: Option<&SeenRecord> = None;



    for record in records.iter().rev() {
        if record.message.get_command() == "QUIT" {
            user_has_quit = Some(record.when.clone());
        }
        if record.message.get_command() == "PRIVMSG" {
            prev_message = Some(record);
            break;
        }
    }
    let now = get_time();
    match (user_has_quit, prev_message) {
        (Some(when_quit), Some(record)) => {
            match ::std::str::from_utf8(record.message.get_args()[1]) {
                Ok(said_what) => format!(
                    "{} said ``{}'' {} ago before quitting {} later",
                    nick,
                    said_what,
                    duration_to_string(now - record.when),
                    duration_to_string(when_quit - record.when)),
                Err(_) => format!("{} said something I dare not repeat {} ago before quitting {} later",
                    nick,
                    duration_to_string(now - record.when),
                    duration_to_string(when_quit - record.when)),
            }
        },
        (None, Some(record)) => {
            match ::std::str::from_utf8(record.message.get_args()[1]) {
                Ok(said_what) => format!(
                    "{} said ``{}'' {} ago",
                    nick,
                    said_what,
                    duration_to_string(now - record.when)),
                Err(_) => format!(
                    "{} said something I dare not repeat {} ago",
                    nick, duration_to_string(now - record.when))
            }
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
        conf.map_format(CMD_SEEN, Format::from_str("seen {nick:s}").unwrap());
    }

    fn on_message(&mut self, _: &mut Replier, msg: &IrcMsg) {
        let privmsg;
        match msg.as_tymsg::<&server::Privmsg>() {
            Ok(p) => privmsg = p,
            Err(_) => return,
        }

        match self.map.remove(&privmsg.source_nick().to_string()) {
            Some(mut records) => {
                records.push(SeenRecord::with_now(privmsg.to_irc_msg().to_owned()));
                self.map.insert(privmsg.get_nick().to_string(), trim_vec(records));
            },
            None => {
                let records = vec![SeenRecord::with_now(privmsg.to_irc_msg().to_owned())];
                self.map.insert(privmsg.get_nick().to_string(), records);
            }
        }
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        let privmsg;
        match msg.as_tymsg::<&server::Privmsg>() {
            Ok(p) => privmsg = p,
            Err(_) => return,
        }

        // Hacky is-channel
        if !privmsg.get_target().starts_with(b"#") {
            return
        }
        let source_nick = privmsg.get_nick();

        let command_phrase = m.command();

        let parsed_command = match command_phrase.token {
            CMD_SEEN => match command_phrase.get("nick") {
                Some(nick) => Some(SeenCommandType::Seen(nick)),
                None => None
            },
            _ => None
        };

        match parsed_command {
            Some(SeenCommandType::Seen(ref target_nick)) => {
                if source_nick == target_nick {
                    m.reply(&format!("Looking for yourself, {}?", source_nick));
                    return;
                }

                if m.current_nick() == target_nick {
                    m.reply(&format!("You found me, {}!", source_nick));
                    return;
                }
                let activity = match self.map.get(target_nick) {
                    Some(val) => val,
                    None => {
                        m.reply(&format!("{} is unknown", target_nick));
                        return
                    }
                };
                m.reply(&format_activity(target_nick, activity));
            },
            None => return
        }
    }
}
