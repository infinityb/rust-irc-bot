#![crate_name = "irc"]
#![crate_type = "dylib"]
#![license = "MIT"]
#![feature(slicing_syntax)]

extern crate time;
extern crate serialize;
extern crate http;
extern crate url;
extern crate irc;


pub use irc::message::{
    IrcMessage,
    IrcProtocolMessage
};

pub use self::botcore::BotConnection;

pub use irc::watchers::{
    MessageWatcher,
    JoinBundler,
    RegisterError,
    RegisterErrorType,
    JoinResult,
    JoinError,
};

pub use self::command_mapper::{
    CommandMapperDispatch,
    PluginContainer,
    IrcBotConfigurator
};

pub mod plugins;
pub mod connection;
pub mod command_mapper;
pub mod botcore;