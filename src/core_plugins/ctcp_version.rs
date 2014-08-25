use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch
};

use message::IrcMessage;


pub struct CtcpVersionResponderPlugin;
static VERSION: &'static str = "rust-irc 0.1.0 https://github.com/infinityb/rust-irc";


impl CtcpVersionResponderPlugin {
    pub fn new() -> CtcpVersionResponderPlugin {
        CtcpVersionResponderPlugin
    }
}


impl RustBotPlugin for CtcpVersionResponderPlugin {
    fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        if message.command() == "PRIVMSG" && message.get_args().len() >= 2 {
            match (message.get_arg(1).as_slice(), message.source_nick()) {
                ("\x01VERSION\x01", Some(source_nick)) => {
                    println!("send version response to {}", source_nick);
                    m.reply_raw(format!("NOTICE {} :\x01VERSION {}\x01", source_nick, VERSION));
                },
                _ => ()
            }
        }
    }
}
