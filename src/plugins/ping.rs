use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


pub struct PingPlugin;


impl PingPlugin {
    pub fn new() -> PingPlugin {
        PingPlugin
    }
}

enum PingCommandType {
    Ping
}


fn parse_command<'a>(m: &CommandMapperDispatch, _message: &'a IrcMessage) -> Option<PingCommandType> {
    match m.command() {
        Some("ping") => Some(Ping),
        Some(_) => None,
        None => None
    }
}


impl RustBotPlugin for PingPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("ping");
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match parse_command(m, message) {
            Some(Ping) => m.reply(format!("pong")),
            None => return
        }
    }
}
