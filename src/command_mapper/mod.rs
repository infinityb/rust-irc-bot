use message::{
    IrcMessage
};


pub struct RustBotPluginApi {
    raw_tx: SyncSender<String>
}


/// Defines the public API the bot exposes to plugins, valid for 
/// the lifetime of the plugin instance.
impl RustBotPluginApi {
    pub fn send_raw(&mut self, string: String) {
        self.raw_tx.send(string);
    }
}


/// Defines the API a plugin implements
// TODO: move to `plugin' module
pub trait RustBotPlugin {
    fn configure(&mut self, _: &mut IrcBotConfigurator) {}
    fn start(&mut self) {}
    fn accept(&mut self, _: &CommandMapperDispatch, _: &IrcMessage) {}
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


/// Defines the public API the bot exposes to plugins, valid while
/// the plugins dispatch_cmd method is called
pub struct CommandMapperDispatch<'a> {
    bot_nick: &'a str,
    sender:  &'a SyncSender<String>,
    channel: Option<&'a str>
}


pub struct CommandMapperDispatchAlloc {
    sender:  SyncSender<String>,
    channel: Option<String>
}


impl<'a> CommandMapperDispatch<'a> {
    pub fn current_nick(&self) -> &'a str {
        self.bot_nick
    }

    pub fn reply(&self, message: String) {
        match self.channel {
            Some(channel) => {
                self.sender.send(format!("PRIVMSG {} :{}", channel, message.as_slice()));
            },
            None => ()
        }
    }

    pub fn reply_raw(&self, message: String) {
        self.sender.send(message);
    }

    /// Get a long-lived version of the dispatcher.
    pub fn acquire(&self) -> CommandMapperDispatchAlloc {
        CommandMapperDispatchAlloc {
            sender: self.sender.clone(),
            channel: match self.channel {
                Some(channel) => Some(String::from_str(channel)),
                None => None
            }
        }
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

    pub fn dispatch(&mut self, bot_nick: &str, raw_tx: &SyncSender<String>, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => Some(String::from_str(channel)),
            None => None
        };
        let dispatch = CommandMapperDispatch {
            bot_nick: bot_nick,
            sender: raw_tx,
            channel: match channel {
                Some(ref channel) => Some(channel.as_slice()),
                None => None
            }
        };
        for pair in self.plugins.mut_iter() {
            let (ref mut plugin, ref mut mappers) = *pair;
            plugin.accept(&dispatch, message);

            for mapper in mappers.iter() {
                let mut prefix_matcher = String::new();
                prefix_matcher = prefix_matcher.append(self.cmd_prefix.as_slice());
                prefix_matcher = prefix_matcher.append(mapper.cmd_word.as_slice());
                if message.get_args().len() > 1 {
                    if message.get_arg(1).as_slice().starts_with(prefix_matcher.as_slice()) {
                        plugin.dispatch_cmd(&dispatch, message);
                    }
                }   
            }
        }
    }
}
