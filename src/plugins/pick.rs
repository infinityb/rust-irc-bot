use irc::parse::IrcMsg;
use rand::{Rng, thread_rng};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_PICK_BARE: Token = Token(0);
const CMD_PICK: Token = Token(1);

pub struct PickPlugin;

impl PickPlugin {
    pub fn new() -> PickPlugin {
        PickPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "pick"
    }
}

enum PickCommandType {
    PickBare,
    Pick(String),
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<PickCommandType> {
    let command_phrase = m.command();
    match command_phrase.token {
        CMD_PICK_BARE => Some(PickCommandType::PickBare),
        CMD_PICK => {
            match command_phrase.get::<String>(&"rest") {
                Some(rest) => Some(PickCommandType::Pick(rest)),
                None => None
            }
        },
        _ => None
    }
}


impl RustBotPlugin for PickPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_PICK_BARE, Format::from_str("pick").ok().unwrap());
        conf.map_format(CMD_PICK, Format::from_str("pick {*rest}").ok().unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMsg) {
        match parse_command(m) {
            Some(PickCommandType::PickBare) => m.reply("pock"),
            Some(PickCommandType::Pick(options)) => {
                let mut rng = thread_rng();
                let options: Vec<&str> = options.split(",").map(|x| x.trim()).collect();
                let picked = rng.choose(&options).unwrap();
                m.reply(picked);
            },
            None => return
        }
    }
}
