use std::string;

use message::{
    IrcMessage
};
pub use self::format::{
    InvalidAtom,
    Unspecified,
    WholeNumeric,
    Atom,
    LiteralAtom,
    FormattedAtom,
    Value,
    StringValue,
    WholeNumericValue,
    Format,
    FormatKind,
    FormatResult,
    EmptyFormat,
    CommandPhrase
};

mod format;

pub struct RustBotPluginApi {
    raw_tx: SyncSender<string::String>
}


/// Defines the public API the bot exposes to plugins, valid for 
/// the lifetime of the plugin instance.
impl RustBotPluginApi {
    pub fn send_raw(&mut self, string: string::String) {
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
    mapped: Vec<Format>
}

/// Defines the public API the bot exposes to plugins for configuration
// TODO: move to `plugin' module
impl IrcBotConfigurator {
    pub fn new() -> IrcBotConfigurator {
        IrcBotConfigurator {
            mapped: Vec::new(),
        }
    }

    pub fn map_format(&mut self, format: Format) {
        self.mapped.push(format);
    }
}


/// Defines the public API the bot exposes to plugins, valid while
/// the plugins dispatch_cmd method is called
#[deriving(Clone)]
pub struct CommandMapperDispatch {
    bot_nick: string::String,
    pub command: Option<CommandPhrase>,
    sender:  SyncSender<string::String>,
    pub channel: Option<string::String>
}


impl CommandMapperDispatch {
    /// The current nickname held by the IRC client
    pub fn current_nick(&self) -> &str {
        self.bot_nick.as_slice()
    }

    /// The current command name, as set by the call to `map` when
    /// `configure` is called on the `RustBotPlugin`.
    pub fn command(&self) -> Option<&CommandPhrase> {
        self.command.as_ref()
    }

    /// Reply with a message to the channel/nick which sent the message being dispatched
    pub fn reply(&self, message: string::String) {
        match self.channel {
            Some(ref channel) => {
                self.sender.send(format!("PRIVMSG {} :{}", channel.as_slice(), message.as_slice()));
            },
            None => ()
        }
    }

    /// Send a raw IRC message to the IRC server
    pub fn reply_raw(&self, message: string::String) {
        self.sender.send(message);
    }
}


pub struct PluginContainer {
    cmd_prefix: string::String,
    plugins: Vec<(Box<RustBotPlugin+'static>, Vec<Format>)>,
}


impl PluginContainer {
    pub fn new(prefix: string::String) -> PluginContainer {
        PluginContainer {
            cmd_prefix: prefix,
            plugins: Vec::new()
        }
    }

    /// Register a plugin instance.  This will configure and start the plugin.
    pub fn register(&mut self, plugin: Box<RustBotPlugin+'static>) {
        let mut plugin = plugin;
        let mut configurator = IrcBotConfigurator::new();
        plugin.configure(&mut configurator);
        plugin.start();
        self.plugins.push((plugin, configurator.mapped));
    }

    /// Dispatches messages to plugins, if they have expressed interest in the message.
    /// Interest is expressed via calling map during the configuration phase.
    pub fn dispatch(&mut self, bot_nick: &str, raw_tx: &SyncSender<string::String>, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => Some(string::String::from_str(channel)),
            None => None
        };

        let mut dispatch = CommandMapperDispatch {
            command: None,
            bot_nick: string::String::from_str(bot_nick),
            sender: raw_tx.clone(),
            channel: match channel {
                Some(ref channel) => Some(channel.clone()),
                None => None
            }
        };

        for pair in self.plugins.iter_mut() {
            let (ref mut plugin, ref mut mappers) = *pair;
            plugin.accept(&dispatch, message);
            for mapper_format in mappers.iter() {
                if is_command_message(message, self.cmd_prefix[]) {
                    let message_body = message.get_arg(1)[self.cmd_prefix.len()..];
                    match mapper_format.parse(message_body) {
                        Ok(Some(command_phrase)) => {
                            dispatch.command = Some(command_phrase);
                            plugin.dispatch_cmd(&dispatch, message);
                            dispatch.command = None;
                        },
                        Ok(None) => (),
                        Err(_) => ()
                    } 
                }
            }
        }
    }
}

fn is_command_message(message: &IrcMessage, prefix: &str) -> bool {
    if message.get_args().len() < 2 {
        return false;
    }
    return message.get_arg(1).as_slice().starts_with(prefix);
}
