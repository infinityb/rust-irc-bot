#![crate_name = "irc"]
#![crate_type = "dylib"]
#![license = "MIT"]

extern crate time;
extern crate serialize;
extern crate debug;
extern crate http;
extern crate url;


pub use self::message::{
    IrcMessage,
    IrcProtocolMessage
};


pub use self::connection::{
    IrcConnection,
    IrcEvent,
    IrcEventBundle,
    IrcEventMessage,
    IrcEventWatcherResponse,
    IrcBundleEventInterface,
};

pub use self::watchers::{
    MessageWatcher,
    JoinMessageWatcher,
    RegisterError,
    RegisterErrorType,
    JoinResult,
    JoinError,
};

pub use self::command_mapper::{
    CommandMapperDispatch,
    CommandMapperRecord,
    PluginContainer,
    IrcBotConfigurator
};

pub mod plugins;
pub mod numerics;
pub mod connection;
pub mod message;
pub mod state;
pub mod watchers;
pub mod command_mapper;
pub mod core_plugins;