#![crate_name = "irc"]
#![crate_type = "dylib"]
#![license = "MIT"]

pub use self::message::{
	IrcMessage,
	IrcNumeric
};

pub mod message;
