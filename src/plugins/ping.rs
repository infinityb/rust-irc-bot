use irc::IrcMessage;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};


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
    match m.command().command[] {
        "ping" => Some(PingCommandType::Ping),
        _ => None
    }
}


impl RustBotPlugin for PingPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("ping").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMessage) {
        match parse_command(m) {
            Some(PingCommandType::Ping) => m.reply(format!("pong")),
            None => return
        }
    }
}
