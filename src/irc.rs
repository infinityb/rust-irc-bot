#![crate_name = "irc"]
#![crate_type = "dylib"]
#![license = "MIT"]


pub use self::message::{
    IrcMessage,
    IrcNumeric
};

pub use self::state::{
    IrcChannel,
    IrcState
};


pub use self::connection::{
    IrcConnection,
    IrcEvent,
    IrcEventMessage,
    IrcEventWatcherResponse,
};

pub use self::watchers::{
    MessageWatcher,
    JoinMessageWatcher,
};

pub mod connection;
pub mod message;
pub mod state;
pub mod watchers;