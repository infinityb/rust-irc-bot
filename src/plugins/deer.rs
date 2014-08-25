use command_mapper::{
    RustBotPluginApi,
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


pub struct DeerPlugin {
    x: int
}


impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        DeerPlugin { x: 0 }
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("deer");
    }

    fn start(&mut self) {
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match message.channel() {
            Some(channel) => {
                println!("DDER GOT COMMAND FROM {}", channel);
            },
            None => {
                println!("DDER GOT COMMAND somehow!");
            }
        }
    }
}
