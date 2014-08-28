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
#[deriving(Clone)]
pub struct CommandMapperDispatch {
    bot_nick: String,
    command: Option<String>,
    sender:  SyncSender<String>,
    channel: Option<String>
}


impl CommandMapperDispatch {
    pub fn current_nick(&self) -> &str {
        self.bot_nick.as_slice()
    }

    pub fn command(&self) -> Option<&str> {
        match self.command {
            Some(ref command) => Some(command.as_slice()),
            None => None
        }
    }

    pub fn reply(&self, message: String) {
        match self.channel {
            Some(ref channel) => {
                self.sender.send(format!("PRIVMSG {} :{}", channel.as_slice(), message.as_slice()));
            },
            None => ()
        }
    }

    pub fn reply_raw(&self, message: String) {
        self.sender.send(message);
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
        plugin.start();
        self.plugins.push((plugin, configurator.mapped));
    }

    pub fn dispatch(&mut self, bot_nick: &str, raw_tx: &SyncSender<String>, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => Some(String::from_str(channel)),
            None => None
        };

        let mut dispatch = CommandMapperDispatch {
            command: None,
            bot_nick: String::from_str(bot_nick),
            sender: raw_tx.clone(),
            channel: match channel {
                Some(ref channel) => Some(channel.clone()),
                None => None
            }
        };

        for pair in self.plugins.mut_iter() {
            let (ref mut plugin, ref mut mappers) = *pair;
            plugin.accept(&dispatch, message);

            for mapper in mappers.iter() {
                if message.get_args().len() > 1 {
                    let first_word = extract_first_word(message.get_arg(1).as_slice());
                    match decompose_command(self.cmd_prefix.as_slice(), first_word) {
                        Some(command) => {
                            if command == mapper.cmd_word.as_slice() {
                                dispatch.command = Some(mapper.cmd_word.clone());
                                println!("dispatching {} -> {}", dispatch.command, command);
                                plugin.dispatch_cmd(&dispatch, message);
                                dispatch.command = None
                            }
                        }
                        None => ()
                    };
                }   
            }
        }
    }
}


fn extract_first_word(privmsg_text: &str) -> &str {
    match privmsg_text.find(' ') {
        Some(idx) => privmsg_text.slice_to(idx),
        None => privmsg_text
    }
}


fn decompose_command<'a>(prefix: &str, first_word: &'a str) -> Option<&'a str> {
    if first_word.len() < prefix.len() {
        return None;
    }
    if prefix != first_word.slice_to(prefix.len()) {
        return None;
    }
    Some(first_word.slice_from(prefix.len()))
}


#[test]
fn test_extract_first_word() {
    assert_eq!(extract_first_word("!deer"), "!deer");
    assert_eq!(extract_first_word("!deerkins foobar"), "!deerkins");
    assert_eq!(extract_first_word(""), "");
}

#[test]
fn test_decompose_command() {
    assert_eq!(decompose_command("!", "!deer"), Some("deer"));
    assert_eq!(decompose_command("@", "!deer"), None);
    assert_eq!(decompose_command("", "deer"), Some("deer"));
    assert_eq!(decompose_command("!", ""), None);
}