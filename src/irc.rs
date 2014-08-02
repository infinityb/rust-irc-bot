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

pub mod message;
pub mod state;