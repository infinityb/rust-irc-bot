use std::convert::From;
use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::io::{self, Read};

use rustc_serialize::json::{self, DecoderError};
use time::{get_time, Timespec};
use url::Url;
use url::form_urlencoded;
use hyper;
use hyper::client::request::Request;
use hyper::method::Method::Get;

use irc::parse::IrcMsg;
use irc::MessageEndpoint::{self, KnownUser};
use irc::UserId;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_DEER_NAMED: Token = Token(0);
const CMD_DEER: Token = Token(1);
const CMD_REED: Token = Token(2);
const CMD_DEERMAN: Token = Token(3);
const CMD_NAMREED: Token = Token(4);
const CMD_DEER_STATS: Token = Token(5);

static DEER: &'static str = concat!(
    "1111111101101\n",
    "1111111101101\n",
    "1111111110011\n",
    "1111111100011\n",
    "1111111110011\n",
    "1100000000011\n",
    "1000000000011\n",
    "1000000000011\n",
    "1010111101011\n",
    "1010111101011\n",
    "1010111101011");

static REED: &'static str = concat!(
    "1011011111111\n",
    "1011011111111\n",
    "1100111111111\n",
    "1100011111111\n",
    "1100111111111\n",
    "1100000000011\n",
    "1100000000001\n",
    "1100000000001\n",
    "1101011110101\n",
    "1101011110101\n",
    "1101011110101");

static DEERMAN: &'static str = concat!(
    "00100100\n",
    "00100100\n",
    "00011000\n",
    "00111000\n",
    "00011000\n",
    "00111100\n",
    "11111111\n",
    "10111101\n",
    "10111101\n",
    "00111100\n",
    "00100100\n",
    "00100100\n",
    "00100100");

static NAMREED: &'static str = concat!(
    "00100100\n",
    "00100100\n",
    "00011000\n",
    "00011100\n",
    "00011000\n",
    "00111100\n",
    "11111111\n",
    "10111101\n",
    "10111101\n",
    "00111100\n",
    "00100100\n",
    "00100100\n",
    "00100100");

fn render_deer(format: &str) -> Vec<String> {
    let mut out = Vec::new();
    for format_line in format.split('\n') {
        let mut output_line = String::new();
        for ch in format_line.chars() {
            match ch {
                '0' => output_line.push_str("\u{0003}00,00@"),
                '1' => output_line.push_str("\u{0003}01,01@"),
                any => output_line.push(any),
            }
        }
        out.push(output_line);
    }
    out
}

static BASE_URL: &'static str = "http://deer.satf.se/deerlist.php";

#[derive(RustcDecodable, RustcEncodable, Clone)]
struct DeerApiResponse {
    irccode: String
}


#[derive(Debug)]
enum DeerApiFailure {
    RequestError(hyper::Error),
    ResponseReadError(io::Error),
    ResponseDeserializeError(DecoderError)
}

impl From<hyper::Error> for DeerApiFailure {
    fn from(err: hyper::Error) -> DeerApiFailure {
        DeerApiFailure::RequestError(err)
    }
}

impl From<io::Error> for DeerApiFailure {
    fn from(err: io::Error) -> DeerApiFailure {
        DeerApiFailure::ResponseReadError(err)
    }
}

impl From<DecoderError> for DeerApiFailure {
    fn from(err: DecoderError) -> DeerApiFailure {
        DeerApiFailure::ResponseDeserializeError(err)
    }
}

fn get_deer_nocache(deer_name: &str) -> Result<DeerApiResponse, DeerApiFailure> {
    let mut url = match Url::parse(BASE_URL) {
        Ok(url) => url,
        Err(_err) => unreachable!()
    };
    url.query = Some(form_urlencoded::serialize(&[("deer", deer_name)]));

    let mut resp = try!(try!(try!(Request::new(Get, url)).start()).send());

    let mut body = String::new();
    try!(resp.read_to_string(&mut body));

    // let body = match String::from_utf8() {
    //     Ok(body) => body,
    //     Err(_err) => return Err(DeerApiFailure::ResponseDecodeError)
    // };

    Ok(try!(json::decode::<DeerApiResponse>(&body)))
}


fn get_deer(state: &mut DeerInternalState, deer_name: &str) -> Result<DeerApiResponse, DeerApiFailure> {
    let deer_name_alloc = String::from_str(deer_name);

    match state.cache.get(&deer_name_alloc) {
        Some(result) => return Ok(result.clone()),
        None => ()
    }

    match get_deer_nocache(deer_name) {
        Ok(response) => {
            state.cache.insert(
                String::from_str(deer_name),
                response.clone());
            Ok(response)
        },
        Err(err) => Err(err)
    }
}


pub struct DeerPlugin {
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMsg)>>
}


impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        DeerPlugin { sender: None }
    }

    pub fn get_plugin_name() -> &'static str {
        "deer"
    }
}

struct DeerInternalState {
    lines_sent: u64,
    cache: HashMap<String, DeerApiResponse>,
    throttle_map: HashMap<(UserId, MessageEndpoint), Timespec>,
}


impl DeerInternalState {
    fn new() -> DeerInternalState {
        DeerInternalState {
            lines_sent: 0,
            cache: HashMap::new(),
            throttle_map: HashMap::new(),
        }
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

        if let DeerCommandType::Deer(_) = *cmd {
            if !self.throttle_ok(source, m.target.clone()) {
                m.reply("2deer4plus");
                return;
            }
        }
        match *cmd {
            DeerCommandType::Deer(ref deer_name) => {
                match get_deer(self, &deer_name) {
                    Ok(deer_data) => {
                        for deer_line in deer_data.irccode.split('\n') {
                            m.reply(deer_line);
                            self.lines_sent += 1;
                        }
                        self.throttle_bump(source, m.target.clone());
                    },
                    Err(err) => {
                        m.reply(&format!("error: {:?}", err));
                    }
                } 
            },
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

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMsg)>) {
        for (m, _) in rx.iter() {
            match parse_command(&m) {
                Some(command) => {
                    println!("handle_command(..., {:?})", command);
                    self.handle_command(&m, &command);
                }
                None => ()
            }
        }
    }
}

#[derive(Debug)]
enum DeerCommandType {
    Deer(String),
    StaticDeer(&'static str),
    DeerStats
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<DeerCommandType> {
    let command_phrase = m.command();
    println!("deer::parse_command ** token = {:?}", command_phrase.token);
    match command_phrase.token {
        CMD_DEER_NAMED => Some(match command_phrase.get("deername") {
            Some(deername) => DeerCommandType::Deer(deername),
            None => DeerCommandType::StaticDeer(DEER)
        }),
        CMD_DEER => Some(DeerCommandType::StaticDeer(DEER)),
        CMD_REED => Some(DeerCommandType::StaticDeer(REED)),
        CMD_DEERMAN => Some(DeerCommandType::StaticDeer(DEERMAN)),
        CMD_NAMREED => Some(DeerCommandType::StaticDeer(NAMREED)),
        CMD_DEER_STATS => Some(DeerCommandType::DeerStats),
        _ => None
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_DEER_NAMED, Format::from_str("deer {*deername}").unwrap());
        conf.map_format(CMD_DEER, Format::from_str("deer").unwrap());
        conf.map_format(CMD_REED, Format::from_str("reed").unwrap());
        conf.map_format(CMD_DEERMAN, Format::from_str("deerman").unwrap());
        conf.map_format(CMD_NAMREED, Format::from_str("namreed").unwrap());
        conf.map_format(CMD_DEER_STATS, Format::from_str("deer-stats").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        let _ = ::std::thread::Builder::new().name("plugin-deer".to_string()).spawn(move || {
            let mut deer_internal_state = DeerInternalState::new();
            deer_internal_state.start(rx);
        });
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMsg) {
        match self.sender {
            Some(ref sender) => {
                if let Err(err) = sender.send((m.clone(), message.clone())) {
                    m.reply(&format!("Service ``deer'' unavailable: {:?}", err));
                }
            }
            None => ()
        };
    }
}


