use std::string;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use irc::State;
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::MessageEndpoint::{
    self,
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
pub trait RustBotPlugin: Send {
    fn configure(&mut self, _: &mut IrcBotConfigurator) {}
    fn start(&mut self) {}
    fn on_irc_msg(&mut self, _: &IrcMsg) {}
    fn dispatch_cmd(&mut self, _: &CommandMapperDispatch, _: &IrcMsg) {}
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

struct DispatchBuilder {
    state: Arc<State>,
    sender: SyncSender<Vec<u8>>,
    reply_target: string::String,
    source: MessageEndpoint,
    target: MessageEndpoint,
}

impl DispatchBuilder {
    fn build(&self, phrase: CommandPhrase) -> CommandMapperDispatch {
        CommandMapperDispatch {
            state: self.state.clone(),
            command: phrase,
            sender: self.sender.clone(),
            reply_target: self.reply_target.clone(),
            source: self.source.clone(),
            target: self.target.clone(),
        }
    }
}


/// Defines the public API the bot exposes to plugins, valid while
/// the plugins dispatch_cmd method is called
#[derive(Clone)]
pub struct CommandMapperDispatch {
    state: Arc<State>,
    command: CommandPhrase,
    sender: SyncSender<Vec<u8>>,
    reply_target: string::String,
    pub source: MessageEndpoint,
    pub target: MessageEndpoint,
}


impl CommandMapperDispatch {
    pub fn get_state(&self) -> Arc<State> {
        self.state.clone()
    }

    /// The current nickname held by the IRC client
    pub fn current_nick(&self) -> &str {
        self.state.get_self_nick()
    }

    /// The current command name, as set by the call to `map` when
    /// `configure` is called on the `RustBotPlugin`.
    pub fn command(&self) -> &CommandPhrase {
        &self.command
    }

    /// Reply with a message to the channel/nick which sent the message being dispatched
    pub fn reply(&self, message: string::String) {
        let privmsg = client::Privmsg::new(self.reply_target.as_slice(), message.as_bytes());
        assert!(self.sender.send(privmsg.into_bytes()).is_ok());
    }
}


pub struct PluginContainer {
    cmd_prefix: string::String,
    plugins: Vec<(Box<RustBotPlugin+Send+'static>, Vec<Format>)>,
}


impl PluginContainer {
    pub fn new(prefix: string::String) -> PluginContainer {
        PluginContainer {
            cmd_prefix: prefix,
            plugins: Vec::new()
        }
    }

    /// Register a plugin instance.  This will configure and start the plugin.
    pub fn register<Plugin>(&mut self, plugin: Plugin) where Plugin: RustBotPlugin {
        let mut plugin = plugin;
        let mut configurator = IrcBotConfigurator::new();
        plugin.configure(&mut configurator);
        plugin.start();

        let boxed_plugin = Box::new(plugin) as Box<RustBotPlugin+Send+'static>;
        self.plugins.push((boxed_plugin, configurator.mapped));
    }

    /// Dispatches messages to plugins, if they have expressed interest in the message.
    /// Interest is expressed via calling map during the configuration phase.
    pub fn dispatch(&mut self, state: Arc<State>, raw_tx: &SyncSender<Vec<u8>>, msg: &IrcMsg) {
        for &mut (ref mut plugin, _) in self.plugins.iter_mut() {
            plugin.on_irc_msg(msg);
        }
        
        let privmsg = match server::IncomingMsg::from_msg(msg.clone()) {
            server::IncomingMsg::Privmsg(privmsg) => privmsg,
            _ => return
        };

        let reply_target = {
            if state.get_self_nick() == privmsg.get_target() {
                privmsg.get_nick().to_string()
            } else {
                privmsg.get_target().to_string()
            }
        };
        
        let source = match state.identify_nick(privmsg.get_nick()) {
            Some(bot_user) => KnownUser(bot_user),
            None => AnonymousUser
        };
        let target = match state.identify_channel(privmsg.get_target()) {
            Some(channel_id) => KnownChannel(channel_id),
            None => match state.identify_nick(privmsg.get_target()) {
                Some(user_id) => KnownUser(user_id),
                None => AnonymousUser
            }
        };

        let builder = DispatchBuilder {
            state: state.clone(),
            sender: raw_tx.clone(),
            reply_target: reply_target,
            source: source.clone(),
            target: target.clone(),
        };

        if let server::IncomingMsg::Privmsg(ref privmsg) = server::IncomingMsg::from_msg(msg.clone()) {
            if is_command_message(msg, &self.cmd_prefix[]) {
                let mut vec = Vec::new();
                let body_raw = privmsg.get_body_raw();
                vec.push_all(&body_raw[self.cmd_prefix.len()..]);
                let message_body = match String::from_utf8(vec) {
                    Ok(string) => string,
                    Err(_) => return,
                };            
                for &mut (ref mut plugin, ref mappers) in self.plugins.iter_mut() {
                    for mapper_format in mappers.iter() {
                        if let Ok(command_phrase) = mapper_format.parse(&message_body[]) {
                            let dispatch = builder.build(command_phrase);
                            plugin.dispatch_cmd(&dispatch, msg);
                        }
                    }
                }
            }
        }
    }
}


fn is_command_message(msg: &IrcMsg, prefix: &str) -> bool {
    if let server::IncomingMsg::Privmsg(ref privmsg) = server::IncomingMsg::from_msg(msg.clone()) {
        return privmsg.get_body_raw().starts_with(prefix.as_bytes());
    }
    false
}
