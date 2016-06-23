use irc::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const IRC_COLOR_MAX: u8 = 16;
const IRC_COLOR_MORE_MAX: u8 = 100;

const CMD_IRC_COLORS: Token = Token(0);
const CMD_IRC_COLORS_MORE: Token = Token(1);


pub struct IrcColorsPlugin;


impl IrcColorsPlugin {
    pub fn new() -> IrcColorsPlugin {
        IrcColorsPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "irc-colors"
    }
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Result<u8, ()> {
    match m.command().token {
        CMD_IRC_COLORS => Ok(IRC_COLOR_MAX),
        CMD_IRC_COLORS_MORE => Ok(IRC_COLOR_MORE_MAX),
        _ => Err(())
    }
}


impl RustBotPlugin for IrcColorsPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_IRC_COLORS, Format::from_str("irc-colors").ok().unwrap());
        conf.map_format(CMD_IRC_COLORS, Format::from_str("irc-colours").ok().unwrap());
        conf.map_format(CMD_IRC_COLORS_MORE, Format::from_str("irc-colors more").ok().unwrap());
        conf.map_format(CMD_IRC_COLORS_MORE, Format::from_str("irc-colours more").ok().unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMsg) {
        if let Ok(colors) = parse_command(m) {
            for line in render(colors).split('\n') {
                m.reply(line);
            }
        }
    }
}

fn render(max: u8) -> String {
    use std::fmt::Write;

    let mut reply = String::new();
    for i in 0..max {
        if i % 16 == 0 && i > 0 {
            write!(&mut reply, "\n").unwrap();
        }
        write!(&mut reply, "\x03{:02}{:02}\x0F ", i, i).unwrap();
    }
    reply
}