use std::slice::SliceConcatExt;

use irc::{IrcMsg, server};

mod data;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_UNICODE_NAME: Token = Token(0);

#[derive(Debug)]
enum UnicodeNameType {
    DecodeValue(String)
}

pub struct UnicodeNamePlugin;

impl UnicodeNamePlugin {
    pub fn get_plugin_name() -> &'static str {
        "unicode-name"
    }

    fn dispatch_cmd_unicode_name(&mut self, m: &CommandMapperDispatch, message: &str) {
        let mut output: Vec<&'static str> = Vec::new();
        for ch in message.chars() {
            output.push(data::NAMES.get(&ch).map(|x| *x).unwrap_or("[UNKNOWN]"));
        }
        m.reply(&output.join(", "));
    }
}

impl RustBotPlugin for UnicodeNamePlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_UNICODE_NAME, Format::from_str("u {*string}").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        if msg.as_tymsg::<&server::Privmsg>().is_err() {
            // only PRIVMSG
            return;
        }
        
        let command_phrase = m.command();
        let parsed_command = match command_phrase.token {
            CMD_UNICODE_NAME => match command_phrase.get("string") {
                Some(nick) => Some(UnicodeNameType::DecodeValue(nick)),
                None => None
            },
            _ => None
        };
        if let Some(UnicodeNameType::DecodeValue(ref value)) = parsed_command {
        	self.dispatch_cmd_unicode_name(m, value);
        }
    }
}
