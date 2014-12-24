extern crate "dbus-rs" as dbus;

use dbus::{Connection, Message, BusType, MessageItem};

use irc::IrcMessage;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};


pub struct WaifuPlugin;

impl WaifuPlugin {
    pub fn new() -> WaifuPlugin {
        WaifuPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "waifu"
    }
}

enum WaifuCommand {
    Waifu,
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<WaifuCommand> {
    match m.command().command[] {
        "waifu" => Some(WaifuCommand::Waifu),
        _ => None
    }
}


impl RustBotPlugin for WaifuPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("waifu").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMessage) {
        match parse_command(m) {
            Some(WaifuCommand::Waifu) => {
                let c = Connection::get_private(BusType::Session).unwrap();
                let mut methcall = Message::new_method_call(
                     "org.yasashiisyndicate.waifuserv", "/org/yasashiisyndicate/waifuserv",
                     "org.yasashiisyndicate.waifuserv", "GetRandom").unwrap();
                methcall.append_items(&[MessageItem::Int32(5)]);
                m.reply(match c.send_with_reply_and_block(methcall, 2000) {
                    Ok(mut rr) => format!("DBus response: {}", rr.get_items()),
                    Err(_) => format!("Waifu service appears to be down"),
                });
                
            },
            None => return
        }
    }
}
