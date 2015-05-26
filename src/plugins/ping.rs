use irc::parse::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_PING: Token = Token(0);

pub struct PingPlugin;

impl PingPlugin {
    pub fn new() -> PingPlugin {
        PingPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "ping"
    }
}

enum PingCommandType {
    Ping
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<PingCommandType> {
    match m.command().token {
        CMD_PING => Some(PingCommandType::Ping),
        _ => None
    }
}


impl RustBotPlugin for PingPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_PING, Format::from_str("ping").ok().unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMsg) {
        match parse_command(m) {
            Some(PingCommandType::Ping) => m.reply("pong"),
            None => return
        }
    }
}
