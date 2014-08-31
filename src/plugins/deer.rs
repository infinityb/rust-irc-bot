use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};
use std::io::IoError;
use serialize::json;
use serialize::json::DecoderError;
use time::{get_time, Timespec};

use url::Url;
use url::form_urlencoded::serialize_owned;

use http::client::RequestWriter;
use http::method::Get;


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


#[deriving(Decodable, Encodable)]
struct DeerApiResponse {
    irccode: String
}


#[deriving(Show)]
enum DeerApiFailure {
    NoResponseError(IoError),
    ResponseReadError(IoError),
    ResponseDecodeError,
    ResponseDeserializeError(DecoderError)
}


fn get_deer(deer_name: &str) -> Result<DeerApiResponse, DeerApiFailure> {
    let mut url = match Url::parse(BASE_URL) {
        Ok(url) => url,
        Err(_err) => unreachable!()
    };
    url.query = Some(serialize_owned(vec![
        (String::from_str("deer"), String::from_str(deer_name)),
    ].as_slice()));

    let request: RequestWriter = RequestWriter::new(Get, url).unwrap();

    let mut response = match request.read_response() {
        Ok(response) => response,
        Err((_, io_error)) => return Err(NoResponseError(io_error))
    };
    let body = match response.read_to_end() {
        Ok(body) => body,
        Err(io_error) => return Err(ResponseReadError(io_error))
    };
    let body = match String::from_utf8(body) {
        Ok(body) => body,
        Err(_err) => return Err(ResponseDecodeError)
    };

    match json::decode::<DeerApiResponse>(body.as_slice()) {
        Ok(result) => Ok(result),
        Err(error) => Err(ResponseDeserializeError(error))
    }
}


pub struct DeerPlugin {
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMessage)>>
}


impl DeerPlugin {
    pub fn new() -> DeerPlugin {
        DeerPlugin { sender: None }
    }
}

struct DeerInternalState {
    lines_sent: u64,
    last_request: Option<Timespec>
}


impl DeerInternalState {
    fn new() -> DeerInternalState {
        DeerInternalState {
            lines_sent: 0,
            last_request: None
        }
    }

    fn throttle(&mut self) -> bool {
        let now = get_time();

        let (new_last_request, throttle_ok) = match self.last_request {
            Some(last_request) => {
                if (now - last_request).sec < 60 {
                    (Some(last_request), true)
                } else {
                    (Some(now), false)
                }                
            },
            None => (Some(now), false)
        };
        self.last_request = new_last_request;
        throttle_ok
    }

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMessage)>) {
        for (m, message) in rx.iter() {
            if message.get_args().len() < 2 {
                continue
            }
            let command_data = match parse_command(&m, &message) {
                Some(DeerStats) => Some(DeerStats),
                Some(command_data) => {
                    if self.throttle() {
                        m.reply(String::from_str("2deer4plus"));
                        None
                    } else {
                        Some(command_data)
                    }
                },
                None => None
            };
            match command_data {
                Some(Deer) => {
                    for deer_line in DEER.split('\n') {
                        m.reply(String::from_str(deer_line));
                        self.lines_sent += 1;
                    }
                },
                Some(Deerkins(deer_name)) => {
                    match get_deer(deer_name) {
                        Ok(deer_data) => {
                            for deer_line in deer_data.irccode.as_slice().split('\n') {
                                m.reply(String::from_str(deer_line));
                                self.lines_sent += 1;
                            }
                        },
                        Err(err) => {
                            m.reply(format!("error: {}", err));
                        }
                    } 
                },
                Some(DeerStats) => {
                    m.reply(format!("lines sent: {}", self.lines_sent));
                }
                None => continue
            };
        }
    }
}


enum DeerCommandType<'a> {
    Deer,
    Deerkins(&'a str),
    DeerStats
}


fn parse_deerkins<'a>(message: &'a IrcMessage) -> Option<DeerCommandType<'a>> {
    let message_body = message.get_arg(1).as_slice();
    match message_body.find(' ') {
        Some(idx) => Some(Deerkins(message_body.slice_from(idx + 1))),
        None => None
    }
}


fn parse_command<'a>(m: &CommandMapperDispatch, message: &'a IrcMessage) -> Option<DeerCommandType<'a>> {
    match m.command() {
        Some("deer") => Some(Deer),
        Some("deerkins") => parse_deerkins(message),
        Some("deerstats") => Some(DeerStats),
        Some(_) => None,
        None => None
    }
}


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("deer");
        conf.map("deerstats");
        conf.map("deerkins");
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        spawn(proc() {
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
