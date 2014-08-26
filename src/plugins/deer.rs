use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


pub struct DeerPlugin;


impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        DeerPlugin
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("deer");
    }

    fn start(&mut self) {
        // "http://deer.satf.se/deerlist.php?deer={}";
    }

    fn dispatch_cmd(&mut self, _m: &CommandMapperDispatch, message: &IrcMessage) {
        // println!("{:?}", m.acquire());
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
