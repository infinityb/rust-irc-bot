use std::error::FromError;
use std::task::TaskBuilder;
use std::io::IoError;
use std::collections::HashMap;

use serialize::json;
use serialize::json::DecoderError;
use time::{get_time, Timespec};
use url::Url;
use url::form_urlencoded::serialize_owned;
use hyper::client::request::Request;
use hyper::HttpError;

use irc::{
    IrcMessage,
    UserId,
    ChannelId,
};
use irc::MessageEndpoint::{
    KnownUser,
    KnownChannel,
};
use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};


static DEER: &'static str = concat!(
    "\u000301,01@@@@@@@@\u000300,00@\u000301,01@@\u000300,00@\u000301,01@\n",
    "\u000301,01@@@@@@@@\u000300,00@\u000301,01@@\u000300,00@\u000301,01@\n",
    "\u000301,01@@@@@@@@@\u000300,00@@\u000301,01@@\n",
    "\u000301,01@@@@@@@@\u000300,00@@@\u000301,01@@\n",
    "\u000301,01@@@@@@@@@\u000300,00@@\u000301,01@@\n",
    "\u000301,01@@\u000300,00@@@@@@@@@\u000301,01@@\n",
    "\u000301,01@\u000300,00@@@@@@@@@@\u000301,01@@\n",
    "\u000301,01@\u000300,00@@@@@@@@@@\u000301,01@@\n",
    "\u000301,01@\u000300,00@\u000301,01@\u000300,00@",
    "\u000301,01@@@@\u000300,00@\u000301,01@\u000300,00@\u000301,01@@\n",
    "\u000301,01@\u000300,00@\u000301,01@\u000300,00@",
    "\u000301,01@@@@\u000300,00@\u000301,01@\u000300,00@\u000301,01@@\n",
    "\u000301,01@\u000300,00@\u000301,01@\u000300,00@",
    "\u000301,01@@@@\u000300,00@\u000301,01@\u000300,00@\u000301,01@@");


static BASE_URL: &'static str = "http://deer.satf.se/deerlist.php";


#[deriving(Decodable, Encodable, Clone)]
struct DeerApiResponse {
    irccode: String
}


#[deriving(Show)]
enum DeerApiFailure {
    ResponseDecodeError,
    RequestError(HttpError),
    ResponseReadError(IoError),
    ResponseDeserializeError(DecoderError)
}

impl FromError<HttpError> for DeerApiFailure {
    fn from_error(err: HttpError) -> DeerApiFailure {
        DeerApiFailure::RequestError(err)
    }
}

impl FromError<IoError> for DeerApiFailure {
    fn from_error(err: IoError) -> DeerApiFailure {
        DeerApiFailure::ResponseReadError(err)
    }
}

impl FromError<DecoderError> for DeerApiFailure {
    fn from_error(err: DecoderError) -> DeerApiFailure {
        DeerApiFailure::ResponseDeserializeError(err)
    }
}


fn get_deer_nocache(deer_name: &str) -> Result<DeerApiResponse, DeerApiFailure> {
    let mut url = match Url::parse(BASE_URL) {
        Ok(url) => url,
        Err(_err) => unreachable!()
    };
    url.query = Some(serialize_owned(vec![
        (String::from_str("deer"), String::from_str(deer_name)),
    ][]));
    
    let mut resp = try!(try!(try!(Request::get(url)).start()).send());
    let body = match String::from_utf8(try!(resp.read_to_end())) {
        Ok(body) => body,
        Err(_err) => return Err(DeerApiFailure::ResponseDecodeError)
    };

    Ok(try!(json::decode::<DeerApiResponse>(body[])))
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
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMessage)>>
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
    throttle_map: HashMap<(UserId, ChannelId), Timespec>,
}


impl DeerInternalState {
    fn new() -> DeerInternalState {
        DeerInternalState {
            lines_sent: 0,
            cache: HashMap::new(),
            throttle_map: HashMap::new(),
        }
    }

    fn throttle_ok(&mut self, uid: UserId, cid: ChannelId) -> bool {
        match self.throttle_map.get(&(uid, cid)) {
            Some(entry) => 60 < (get_time() - *entry).num_seconds(),
            None => true
        }
    }

    fn throttle_bump(&mut self, uid: UserId, cid: ChannelId) {
        self.throttle_map.insert((uid, cid), get_time());
    }

    fn handle_command<'a>(&mut self, m: &CommandMapperDispatch, cmd: &'a DeerCommandType) {
        let (source, target) = match (m.source.clone(), m.target.clone()) {
            (Some(KnownUser(source)), Some(KnownChannel(target))) => (source, target),
            _ => return
        };

        if let DeerCommandType::Deer(_) = *cmd {
            if !self.throttle_ok(source, target) {
                m.reply(String::from_str("2deer4plus"));
                return;
            }
        }
        match *cmd {
            DeerCommandType::Deer(Some(ref deer_name)) => {
                match get_deer(self, deer_name[]) {
                    Ok(deer_data) => {
                        for deer_line in deer_data.irccode[].split('\n') {
                            m.reply(String::from_str(deer_line));
                            self.lines_sent += 1;
                        }
                        self.throttle_bump(source, target);
                    },
                    Err(err) => {
                        m.reply(format!("error: {}", err));
                    }
                } 
            },
            DeerCommandType::Deer(None) => {
                for deer_line in DEER.split('\n') {
                    m.reply(String::from_str(deer_line));
                    self.lines_sent += 1;
                }
                self.throttle_bump(source, target);
            },
            DeerCommandType::DeerStats => {
                m.reply(format!("lines sent: {}", self.lines_sent));
            }
        };
    }

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMessage)>) {
        for (m, _) in rx.iter() {
            match parse_command(&m) {
                Some(ref command) => self.handle_command(&m, command),
                None => ()
            }
        }
    }
}

enum DeerCommandType {
    Deer(Option<String>),
    DeerStats
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<DeerCommandType> {
    let command_phrase = match m.command() {
        Some(command_phrase) => command_phrase,
        None => return None
    };
    match command_phrase.command[] {
        "deer" => Some(DeerCommandType::Deer(command_phrase.get("deername"))),
        "deerstats" => Some(DeerCommandType::DeerStats),
        _ => None
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("deer {*deername}").unwrap());
        conf.map_format(Format::from_str("deer").unwrap());
        conf.map_format(Format::from_str("deer-stats").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        TaskBuilder::new().named("plugin-deer").spawn(proc() {
            let mut deer_internal_state = DeerInternalState::new();
            deer_internal_state.start(rx);
        });
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send((m.clone(), message.clone())),
            None => ()
        };
    }
}
