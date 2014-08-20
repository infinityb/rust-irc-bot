use connection::{
    RustBotPluginApi,
    RustBotPlugin,
};
use message::{
    IrcMessage
};


pub struct DeerPlugin;


impl DeerPlugin {
    pub fn new(sender: RustBotPluginApi) -> DeerPlugin {
        DeerPlugin
    }
}


impl RustBotPlugin for DeerPlugin {
    fn accept(&mut self, message: &IrcMessage) {
    }
}
