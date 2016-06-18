use irc::{IrcMsg, server};
use rand::{thread_rng, Rng};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_8BALL: Token = Token(1);

const ANSWERS: &'static [&'static str] = &[
    "It is certain",
    "It is decidedly so",
    "Without a doubt",
    "Yes, definitely",
    "You may rely on it",
    "As I see it, yes",
    "Most likely",
    "Outlook good",
    "Yes",
    "Signs point to yes",
    "Reply hazy try again",
    "Ask again later",
    "Better not tell you now",
    "Cannot predict now",
    "Concentrate and ask again",
    "Don't count on it",
    "My reply is no",
    "My sources say no",
    "Outlook not so good",
    "Very doubtful",
];

pub struct EightBallPlugin;

impl EightBallPlugin {
    pub fn new() -> EightBallPlugin {
        EightBallPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "8ball"
    }
}

enum EightBallCommandType {
    EightBall,
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<EightBallCommandType> {
    match m.command().token {
        CMD_8BALL => Some(EightBallCommandType::EightBall),
        _ => None
    }
}


impl RustBotPlugin for EightBallPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_8BALL, Format::from_str("8ball {*query}").ok().unwrap());
        conf.map_format(CMD_8BALL, Format::from_str("8ball").ok().unwrap());
        conf.map_format(CMD_8BALL, Format::from_str("\u{1f3b1} {*query}").ok().unwrap());
        conf.map_format(CMD_8BALL, Format::from_str("\u{1f3b1}").ok().unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        match parse_command(m) {
            Some(EightBallCommandType::EightBall) => {
                if let Ok(privmsg) = msg.as_tymsg::<&server::Privmsg>() {
                     let answer = thread_rng().choose(ANSWERS).unwrap();
                     m.reply(&format!("{}: {}", msg.source_nick(), answer));          
                }
            },
            None => return
        }
    }
}
