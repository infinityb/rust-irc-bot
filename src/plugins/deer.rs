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
    tx: SyncSender<IrcMessage>,
    deferred_proc: Option<proc():Send>
}


impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        let (tx, rx) = sync_channel(100);

        let deferred_proc = Some(proc() {
            for message in rx.iter() {
                println!("{}", message);
            }
        });

        DeerPlugin {
            tx: tx,
            deferred_proc: deferred_proc
        }
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("deer");
    }

    fn start(&mut self) {
        match self.deferred_proc.take() {
            Some(deferred_proc) => spawn(deferred_proc),
            None => ()
        };
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
