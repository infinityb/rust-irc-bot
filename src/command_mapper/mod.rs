use message::{
    IrcMessage
};


pub struct RustBotPluginApi {
    raw_tx: SyncSender<String>
}


/// Defines the public API the bot exposes to plugins
impl RustBotPluginApi {
    pub fn send_raw(&mut self, string: String) {
        self.raw_tx.send(string);
    }
}


/// Defines the API a plugin implements
// TODO: move to `plugin' module
pub trait RustBotPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator);
    fn start(&mut self);
    fn accept(&mut self, _: &IrcMessage) {}
    fn dispatch_cmd(&mut self, _: &CommandMapperDispatch, _: &IrcMessage) {}
}


pub struct IrcBotConfigurator {
    mapped: Vec<CommandMapperRecord>,
}

/// Defines the public API the bot exposes to plugins for configuration
// TODO: move to `plugin' module
impl IrcBotConfigurator {
    pub fn new() -> IrcBotConfigurator {
        IrcBotConfigurator {
            mapped: Vec::new()
        }
    }

    pub fn map(&mut self, command_word: &str) {
        self.mapped.push(CommandMapperRecord {
            cmd_word: String::from_str(command_word)
        });
    }
}


pub struct CommandMapperDispatch<'a> {
    sender:  &'a SyncSender<String>,
    channel: Option<&'a str>
}


impl<'a> CommandMapperDispatch<'a> {
    pub fn reply(&self, message: String) {
        let mut sender = self.sender.clone();
        sender.send(format!("PRIVMSG #dicks :{}", message.as_slice()));
        // println!("WOULD REPLY WITH: {}", message.as_slice());
    }
}

pub struct CommandMapperRecord {
    cmd_word: String,
    // sender: SyncSender<(CommandMapperDispatch, IrcMessage)>,
}


#[deriving(Send)]
pub struct PluginContainer {
    cmd_prefix: String,
    plugins: Vec<(Box<RustBotPlugin>, Vec<CommandMapperRecord>)>,
}


impl PluginContainer {
    pub fn new(prefix: String) -> PluginContainer {
        PluginContainer {
            cmd_prefix: prefix,
            plugins: Vec::new()
        }
    }

    pub fn register(&mut self, plugin: Box<RustBotPlugin>) {
        let mut plugin = plugin;
        let mut configurator = IrcBotConfigurator::new();
        plugin.configure(&mut configurator);
        self.plugins.push((plugin, configurator.mapped));
    }

    pub fn dispatch(&mut self, raw_tx: &SyncSender<String>, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => Some(String::from_str(channel)),
            None => None
        };
        let dispatch = CommandMapperDispatch {
            sender: raw_tx,
            channel: match channel {
                Some(ref channel) => Some(channel.as_slice()),
                None => None
            }
        };
        for pair in self.plugins.mut_iter() {
            let (ref mut plugin, ref mut mappers) = *pair;
            println!("PluginContainer dispatching for {:?}", plugin);
            for mapper in mappers.iter() {
                let mut prefix_matcher = String::new();
                prefix_matcher = prefix_matcher.append(self.cmd_prefix.as_slice());
                prefix_matcher = prefix_matcher.append(mapper.cmd_word.as_slice());
                if message.get_args().len() > 1 {
                    println!("PluginContainer dispatching for {:?}/{}.starts_with({})",
                        plugin, message.get_arg(1).as_slice(), prefix_matcher.as_slice());
                    if message.get_arg(1).as_slice().starts_with(prefix_matcher.as_slice()) {
                        plugin.dispatch_cmd(&dispatch, message);
                    }
                }   
            }
        }
    }
}
