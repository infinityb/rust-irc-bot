use irc::{IrcMsg, server};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};


const CMD_WHOAMI: Token = Token(0);
const CMD_WHEREAMI: Token = Token(1);

pub struct WhoAmIPlugin;

impl WhoAmIPlugin {
    pub fn new() -> WhoAmIPlugin {
        WhoAmIPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "whoami"
    }
}

enum WhoAmICommandType {
    WhoAmI,
    WhereAmI
}


fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<WhoAmICommandType> {
    match m.command().token {
        CMD_WHOAMI => Some(WhoAmICommandType::WhoAmI),
        CMD_WHEREAMI => Some(WhoAmICommandType::WhereAmI),
        _ => None
    }
}


impl RustBotPlugin for WhoAmIPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_WHOAMI, Format::from_str("whoami").unwrap());
        conf.map_format(CMD_WHEREAMI, Format::from_str("whereami").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        let privmsg;
        match msg.as_tymsg::<&server::Privmsg>() {
            Ok(p) => privmsg = p,
            Err(_) => return,
        }

        match parse_command(m) {
            Some(WhoAmICommandType::WhoAmI) => {
                m.reply(&format!("{}: you are {:?}", privmsg.source_nick(), m.source));
            },
            Some(WhoAmICommandType::WhereAmI) => {
                m.reply(&format!("{}: you are in {:?}", privmsg.source_nick(), m.target));
            },
            None => ()
        }
    }
}