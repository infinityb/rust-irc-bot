use irc::parse::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_DUCK: Token = Token(1);

static DUCK_CONTENT: &'static str = include_str!("asciiart_asset/duck.txt");

pub struct AsciiArtPlugin;

impl AsciiArtPlugin {
    pub fn new() -> AsciiArtPlugin {
        AsciiArtPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "asciiart"
    }
}

impl RustBotPlugin for AsciiArtPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_DUCK, Format::from_str("duck").ok().unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMsg) {
        match m.command().token {
            CMD_DUCK => {
                for line in DUCK_CONTENT.split('\n') {
                    m.reply(line);
                }
            },
            _ => (),
        }
    }
}
