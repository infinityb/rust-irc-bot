use std::collections::BTreeMap;

use time::{get_time, Timespec};

use irc::{IrcMsg, server};

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

enum Message {
    Privmsg(server::PrivmsgBuf),
    Quit(server::QuitBuf),
}

pub struct SeenRecord {
    when: Timespec,
    message: Message,
}

impl SeenRecord {
    fn new_privmsg(when: Timespec, message: server::PrivmsgBuf) -> SeenRecord {
        SeenRecord {
            when: when,
            message: Message::Privmsg(message)
        }
    }

    fn new_quit(when: Timespec, message: server::QuitBuf) -> SeenRecord {
        SeenRecord {
            when: when,
            message: Message::Quit(message)
        }
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


fn trim_vec<T>(vec: &mut Vec<T>) {
    if vec.len() <= MAX_USER_RECORDS_KEPT {
        return;
    }

    let before = ::std::mem::replace(vec, Vec::new());
    let excess_elem = before.len() - MAX_USER_RECORDS_KEPT;
    vec.extend(before.into_iter().skip(excess_elem));
}

enum SeenCommandType {
    Seen(String)
}

fn format_activity(nick: &str, records: &Vec<SeenRecord>) -> String {
    use std::str;
    use std::fmt::Write;

    let mut priv_msg: Option<(Timespec, Option<String>)> = None;
    let mut quit_msg: Option<Timespec> = None;
    
    for record in records.iter() {
        match record.message {
            Message::Privmsg(ref pmsg) => {
                let said_what = str::from_utf8(pmsg.get_body_raw())
                    .ok().map(ToOwned::to_owned);
                priv_msg = Some((record.when, said_what));
            }
            Message::Quit(_) => quit_msg = Some(record.when),
        }
    }

    let now = get_time();

    let mut out = String::new();
    match priv_msg {
        Some((when, Some(ref said_what))) => {
            write!(&mut out, "{} said ``{}'' {} ago",
                nick, said_what, duration_to_string(now - when)).unwrap();
        },
        Some((when, None)) => {
            write!(&mut out, "{} said something I dare not repeat {} ago",
                nick, duration_to_string(now - when)).unwrap();
        },
        None => (),
    }
    match (quit_msg, priv_msg.is_some()) {
        (Some(when), true) => {
            write!(&mut out, " before quitting {} later", duration_to_string(now - when)).unwrap();
        },
        (Some(when), false) => {
            write!(&mut out, "{} quit {} ago", nick, duration_to_string(now - when)).unwrap();
        },
        (None, true) => (),
        (None, false) => {
            write!(&mut out, "Sorry, I am very confused about {}", nick).unwrap();
        }
    }

    out
}


impl RustBotPlugin for SeenPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_SEEN, Format::from_str("seen {nick:s}").unwrap());
    }

    fn on_message(&mut self, _: &mut Replier, msg: &IrcMsg) {
        if let Ok(privmsg) = msg.as_tymsg::<&server::Privmsg>() {
            // FIXME: dedup this code? source_nick could be on IrcMsg
            let source = privmsg.source_nick().to_owned();
            let records: &mut Vec<SeenRecord> = self.map.entry(source).or_insert(Vec::new());
            records.push(SeenRecord::new_privmsg(get_time(), privmsg.to_owned()));
            trim_vec(records);
        }
        if let Ok(quitmsg) = msg.as_tymsg::<&server::Quit>() {
            let source = quitmsg.source_nick().to_owned();
            let records: &mut Vec<SeenRecord> = self.map.entry(source).or_insert(Vec::new());
            records.push(SeenRecord::new_quit(get_time(), quitmsg.to_owned()));
            trim_vec(records);
        }
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        let privmsg;
        match msg.as_tymsg::<&server::Privmsg>() {
            Ok(p) => privmsg = p,
            Err(_) => return,
        }

        // FIXME: Hacky is-channel
        if !privmsg.get_target().starts_with(b"#") {
            return
        }
        let source_nick = privmsg.source_nick();

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
