use irc::IrcMsg;

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

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _msg: &IrcMsg) {

        match parse_command(m) {
            Some(WhoAmICommandType::WhoAmI) => match (msg.get_prefix().nick(), &m.source) {
                (Some(ref source_nick), uid) => {
                    m.reply(&format!("{}: you are {:?}", source_nick, uid));
                },
                (_, _) => ()
            },
            Some(WhoAmICommandType::WhereAmI) => match (msg.get_prefix().nick(), &m.target) {
                (Some(ref source_nick), cid) => {
                    m.reply(&format!("{}: you are in {:?}", source_nick, cid));
                },
                (_, _) => ()
            },
            None => ()
        }
    }
}