use std::collections::HashMap;

use time::{get_time, Timespec};

use irc::IrcMsg;
use irc::legacy::UserId;
use irc::legacy::MessageEndpoint::{self, KnownUser};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_DEER: Token = Token(1);
const CMD_REED: Token = Token(2);
const CMD_DEERMAN: Token = Token(3);
const CMD_NAMREED: Token = Token(4);
const CMD_DEER_STATS: Token = Token(5);

static DEER: &'static str = concat!(
    "        #  # \n",
    "        #  # \n",
    "         ##  \n",
    "        ###  \n",
    "         ##  \n",
    "  #########  \n",
    " ##########  \n",
    " ##########  \n",
    " # #    # #  \n",
    " # #    # #  \n",
    " # #    # #  ");

static REED: &'static str = concat!(
    " #  #        \n",
    " #  #        \n",
    "  ##         \n",
    "  ###        \n",
    "  ##         \n",
    "  #########  \n",
    "  ########## \n",
    "  ########## \n",
    "  # #    # # \n",
    "  # #    # # \n",
    "  # #    # # ");

static DEERMAN: &'static str = concat!(
    "  #  #  \n",
    "  #  #  \n",
    "   ##   \n",
    "  ###   \n",
    "   ##   \n",
    "  ####  \n",
    "########\n",
    "# #### #\n",
    "# #### #\n",
    "  ####  \n",
    "  #  #  \n",
    "  #  #  \n",
    "  #  #  ");

static NAMREED: &'static str = concat!(
    "  #  #  \n",
    "  #  #  \n",
    "   ##   \n",
    "   ###  \n",
    "   ##   \n",
    "  ####  \n",
    "########\n",
    "# #### #\n",
    "# #### #\n",
    "  ####  \n",
    "  #  #  \n",
    "  #  #  \n",
    "  #  #  ");

fn render_deer(format: &str) -> Vec<String> {
    let mut out = Vec::new();
    for format_line in format.split('\n') {
        let mut output_line = String::new();
        for ch in format_line.chars() {
            match ch {
                '#' => output_line.push_str("\u{0003}00,00@"),
                ' ' => output_line.push_str("\u{0003}01,01@"),
                any => output_line.push(any),
            }
        }
        out.push(output_line);
    }
    out
}

pub struct DeerPlugin {
    lines_sent: u64,
    throttle_map: HashMap<(UserId, MessageEndpoint), Timespec>,
}

impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_DEER, Format::from_str("deer").unwrap());
        conf.map_format(CMD_REED, Format::from_str("reed").unwrap());
        conf.map_format(CMD_DEERMAN, Format::from_str("deerman").unwrap());
        conf.map_format(CMD_NAMREED, Format::from_str("namreed").unwrap());
        conf.map_format(CMD_DEER_STATS, Format::from_str("deer-stats").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _msg: &IrcMsg) {
        match parse_command(&m) {
            Some(command) => self.handle_command(&m, &command),
            None => ()
        }
    }
}

impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        DeerPlugin {
            lines_sent: 0,
            throttle_map: HashMap::new(),
        }
    }

    pub fn get_plugin_name() -> &'static str {
        "deer"
    }

    fn throttle_ok(&mut self, uid: UserId, endpoint: MessageEndpoint) -> bool {
        match self.throttle_map.get(&(uid, endpoint)) {
            Some(entry) => 60 < (get_time() - *entry).num_seconds(),
            None => true
        }
    }

    fn throttle_bump(&mut self, uid: UserId, endpoint: MessageEndpoint) {
        self.throttle_map.insert((uid, endpoint), get_time());
    }

    fn handle_command<'a>(&mut self, m: &CommandMapperDispatch, cmd: &'a DeerCommandType) {
        let source = match m.source {
            KnownUser(source) => source,
            _ => return
        };
        if should_throttle(cmd) {
            if !self.throttle_ok(source, m.target.clone()) {
                m.reply("2deer4plus");
                return;
            }
        }
        match *cmd {
            DeerCommandType::StaticDeer(data) => {
                for deer_line in render_deer(data).into_iter() {
                    m.reply(&deer_line);
                    self.lines_sent += 1;
                }
                self.throttle_bump(source, m.target.clone());
            },
            DeerCommandType::DeerStats => {
                m.reply(&format!("lines sent: {}", self.lines_sent));
            }
        };
    }
}

fn should_throttle(cmd: &DeerCommandType) -> bool {
    match *cmd {
        DeerCommandType::StaticDeer(_) => true,
        DeerCommandType::DeerStats => false,
    }
}

#[derive(Debug)]
enum DeerCommandType {
    StaticDeer(&'static str),
    DeerStats
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<DeerCommandType> {
    let command_phrase = m.command();
    println!("deer::parse_command ** token = {:?}", command_phrase.token);
    match command_phrase.token {
        CMD_DEER => Some(DeerCommandType::StaticDeer(DEER)),
        CMD_REED => Some(DeerCommandType::StaticDeer(REED)),
        CMD_DEERMAN => Some(DeerCommandType::StaticDeer(DEERMAN)),
        CMD_NAMREED => Some(DeerCommandType::StaticDeer(NAMREED)),
        CMD_DEER_STATS => Some(DeerCommandType::DeerStats),
        _ => None
    }
}
