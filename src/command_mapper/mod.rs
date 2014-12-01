use std::string;
use std::sync::Arc;

use irc::IrcMessage;

use irc::State;
use irc::MessageEndpoint::{
    mod,
    KnownUser,
    KnownChannel,
    AnonymousUser,
};

pub use self::format::{
    Format,
    FormatResult,
    CommandPhrase
};
pub use self::format::FormatParseError::EmptyFormat;

mod format;


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
    state: Arc<State>,
    bot_nick: string::String,
    pub command: Option<CommandPhrase>,
    sender: SyncSender<string::String>,
    pub channel: Option<string::String>,
    pub source: Option<MessageEndpoint>,
    pub target: Option<MessageEndpoint>
}


impl CommandMapperDispatch {
    pub fn get_state(&self) -> Arc<State> {
        self.state.clone()
    }

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
    pub fn dispatch(&mut self, state: Arc<State>, raw_tx: &SyncSender<string::String>, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => Some(string::String::from_str(channel)),
            None => None
        };

        let mut source: Option<MessageEndpoint> = None;
        let mut target: Option<MessageEndpoint> = None;

        if let Some(source_nick) = message.source_nick() {
            source = match state.identify_nick(source_nick[]) {
                Some(bot_user) => Some(KnownUser(bot_user)),
                None => Some(AnonymousUser)
            };
        }

        // We don't really support incoming PMs atm...
        if let Some(ref target_chan) = channel {
            target = match state.identify_channel(target_chan.as_slice()) {
                Some(channel_id) => Some(KnownChannel(channel_id)),
                None => {
                    warn!("message from unknown channel {}", target_chan);
                    None
                }
            };
        }
        let self_nick = state.get_self_nick().to_string();
        let mut dispatch = CommandMapperDispatch {
            state: state,
            command: None,
            bot_nick: self_nick,
            sender: raw_tx.clone(),
            channel: match channel {
                Some(ref channel) => Some(channel.clone()),
                None => None
            },
            source: source,
            target: target,
        };

        for pair in self.plugins.iter_mut() {
            let (ref mut plugin, ref mut mappers) = *pair;
            plugin.accept(&dispatch, message);
            
            for mapper_format in mappers.iter() {
                if is_command_message(message, self.cmd_prefix[]) {
                    let message_body = message.get_args()[1][self.cmd_prefix.len()..];
                    match mapper_format.parse(message_body) {
                        Ok(command_phrase) => {
                            dispatch.command = Some(command_phrase);
                            plugin.dispatch_cmd(&dispatch, message);
                            dispatch.command = None;
                        },
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
    return message.get_args()[1].starts_with(prefix);
}
