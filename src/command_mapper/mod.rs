use std::sync::Arc;

use mio::Sender;
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::FrozenState;
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

#[derive(Copy, Clone, Debug)]
pub struct Token(pub u64);


pub struct Replier(Sender<IrcMsg>);

impl Replier {
    pub fn reply(&mut self, msg: IrcMsg) -> Result<(), ()> {
        println!("Replier::reply EMITTING: {:?}", ::botcore::MaybeString::new(msg.as_bytes()));
        match self.0.send(msg) {
            Ok(_) => Ok(()),
            Err(err) => {
                panic!("Error during send: {:?}", err);
                Err(())
            }
        }
    }
}

/// Defines the API a plugin implements
// TODO: move to `plugin' module
pub trait RustBotPlugin {
    fn configure(&mut self, _: &mut IrcBotConfigurator) {}
    fn start(&mut self) {}
    fn on_message(&mut self, _: &mut Replier, _: &IrcMsg) {}
    // fn on_timer(&mut self, _: &mut Replier, _: TimerToken) {}
    fn dispatch_cmd(&mut self, _: &CommandMapperDispatch, _: &IrcMsg) {}
}


pub struct IrcBotConfigurator {
    mapped: Vec<(Token, Format)>
}

/// Defines the public API the bot exposes to plugins for configuration
// TODO: move to `plugin' module
impl IrcBotConfigurator {
    pub fn new() -> IrcBotConfigurator {
        IrcBotConfigurator {
            mapped: Vec::new(),
        }
    }

    pub fn map_format(&mut self, token: Token, format: Format) {
        self.mapped.push((token, format));
    }
}

struct DispatchBuilder {
    state: Arc<FrozenState>,
    sender: Sender<IrcMsg>,
    reply_target: String,
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
    state: Arc<FrozenState>,
    command: CommandPhrase,
    sender: Sender<IrcMsg>,
    reply_target: String,
    pub source: MessageEndpoint,
    pub target: MessageEndpoint,
}


impl CommandMapperDispatch {
    pub fn get_state(&self) -> Arc<FrozenState> {
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
    pub fn reply(&self, message: &str) {
        let privmsg = client::Privmsg::new(&self.reply_target, message.as_bytes()).into_irc_msg();
        println!("CommandMapperDispatch::reply EMITTING: {:?}", ::botcore::MaybeString::new(privmsg.as_bytes()));
        self.sender.send(privmsg).ok().expect("Failed to send to IRC socket");
    }

    // /// Reply with a message to the channel/nick which sent the message being dispatched
    // pub fn reply_bin(&self, message: Vec<u8>) {
    //     let privmsg = client::Privmsg::new(self.reply_target.as_slice(), message.as_slice());
    //     self.sender.send(privmsg.into_irc_msg()).ok().expect("Failed to send to IRC socket");
    // }
}


pub struct PluginContainer {
    cmd_prefixes: Vec<String>,
    plugins: Vec<(Box<RustBotPlugin+'static>, Vec<(Token, Format)>)>,
}


impl PluginContainer {
    pub fn new(prefixes: Vec<String>) -> PluginContainer {
        PluginContainer {
            cmd_prefixes: prefixes,
            plugins: Vec::new()
        }
    }

    /// Register a plugin instance.  This will configure and start the plugin.
    pub fn register<P>(&mut self, plugin: P) where P: RustBotPlugin+'static {
        let mut plugin = Box::new(plugin) as Box<RustBotPlugin+'static>;
        let mut configurator = IrcBotConfigurator::new();
        plugin.configure(&mut configurator);
        plugin.start();
        self.plugins.push((plugin, configurator.mapped));
    }

    /// Dispatches messages to plugins, if they have expressed interest in the message.
    /// Interest is expressed via calling map during the configuration phase.
    pub fn dispatch(&mut self, state: Arc<FrozenState>, raw_tx: &Sender<IrcMsg>, msg: &IrcMsg) {
        let mut replier = Replier(raw_tx.clone());
        for &mut (ref mut plugin, _) in self.plugins.iter_mut() {
            plugin.on_message(&mut replier, msg);
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

        let nick_cmd = format!("{}: ", state.get_self_nick());
        let mut prefix = get_prefix(privmsg.to_irc_msg(), &self.cmd_prefixes);

        if privmsg.get_body_raw().starts_with(nick_cmd.as_bytes()) {
            prefix = prefix.or(Some(&nick_cmd));
        }
        
        if let Some(prefix) = prefix {
            let mut vec = Vec::new();

            let body_raw = privmsg.get_body_raw();
            vec.push_all(&body_raw[prefix.len()..]);

            let message_body = match String::from_utf8(vec) {
                Ok(string) => string,
                Err(_) => return,
            };

            for &mut (ref mut plugin, ref mappers) in self.plugins.iter_mut() {
                for &(token, ref mapper_format) in mappers.iter() {
                    if let Ok(command_phrase) = mapper_format.parse(token, &message_body) {
                        let dispatch = builder.build(command_phrase);
                        plugin.dispatch_cmd(&dispatch, privmsg.to_irc_msg());
                    }
                }
            }
        }
    }
}

fn get_prefix<'a>(msg: &IrcMsg, prefixes: &'a [String]) -> Option<&'a str> {
    if let server::IncomingMsg::Privmsg(ref privmsg) = server::IncomingMsg::from_msg(msg.clone()) {
        for prefix in prefixes.iter() {
            if privmsg.get_body_raw().starts_with(prefix.as_bytes()) {
                return Some(&prefix);
            }
        }
    }
    None
}
