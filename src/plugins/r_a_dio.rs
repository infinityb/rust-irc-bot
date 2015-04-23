use std::io::{self, Read};
use std::convert::From;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};

use rustc_serialize::json::{self, DecoderError};
use url::Url;
use hyper::client::request::Request;
use hyper::HttpError;
use hyper::method::Method::Get;

use irc::parse::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};


static API_URL: &'static str = "https://r-a-d.io/api/";

const CMD_DJ: Token = Token(0);

#[derive(RustcDecodable, RustcEncodable, Clone)]
struct RadioApiResponse {
    main: RadioStreamApiResponse
}


#[derive(RustcDecodable, RustcEncodable, Clone)]
struct RadioStreamApiResponse {
    np: String,
    listeners: u32,
    djname: String
}


#[derive(Debug)]
enum RadioApiFailure {
    RequestError(HttpError),
    ResponseReadError(io::Error),
    ResponseDeserializeError(DecoderError)
}


impl From<HttpError> for RadioApiFailure {
    fn from(err: HttpError) -> RadioApiFailure {
        RadioApiFailure::RequestError(err)
    }
}

impl From<io::Error> for RadioApiFailure {
    fn from(err: io::Error) -> RadioApiFailure {
        RadioApiFailure::ResponseReadError(err)
    }
}

impl From<DecoderError> for RadioApiFailure {
    fn from(err: DecoderError) -> RadioApiFailure {
        RadioApiFailure::ResponseDeserializeError(err)
    }
}


fn get_radio_api_result() -> Result<RadioApiResponse, RadioApiFailure> {
    let url = Url::parse(API_URL).ok().expect("Invalid URL :-(");
    let mut resp = try!(try!(try!(Request::new(Get, url)).start()).send());

    let mut body = String::new();
    try!(resp.read_to_string(&mut body));
    Ok(try!(json::decode::<RadioApiResponse>(&body)))
}


fn format_radio_stream_response(resp: RadioStreamApiResponse) -> String {
    format!("{} \u{2014} np: {}", resp.djname, resp.np)
}

struct RadioInternalState {
    requests_made: u32,
    requests_failed: u32,
}


impl RadioInternalState {
    fn new() -> RadioInternalState {
        RadioInternalState {
            requests_made: 0,
            requests_failed: 0,
        }
    }

    fn handle_dj(&mut self, m: &CommandMapperDispatch) {
        self.requests_made += 1;

        match get_radio_api_result() {
            Ok(res) => {
                m.reply(format_radio_stream_response(res.main));
            }
            Err(err) => {
                self.requests_failed += 1;
                m.reply(format!("Error: {:?}", err));
            }
        }
    }
    fn start(&mut self, rx: Receiver<EventType>) {
        for event in rx.iter() {
            match event {
                EventType::Dispatch(dispatch) => {
                    match dispatch.command().token {
                        CMD_DJ => self.handle_dj(&dispatch),
                        _ => ()
                    }
                }
            }
        }
    }
}

pub struct RadioPlugin {
    sender: Option<SyncSender<EventType>>,
}

pub enum EventType {
    Dispatch(CommandMapperDispatch),
} 


impl RadioPlugin {
    pub fn new() -> RadioPlugin {
        RadioPlugin {
            sender: None
        }
    }

    pub fn get_plugin_name() -> &'static str {
        "r/a/dio"
    }
}


impl RustBotPlugin for RadioPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_DJ, Format::from_str("dj").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        let _ = ::std::thread::Builder::new().name("plugin-radio".to_string()).spawn(move || {
            let mut internal_state = RadioInternalState::new();
            internal_state.start(rx);
        });

        self.sender = Some(tx);
    }
    
    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _message: &IrcMsg) {
        match self.sender {
            Some(ref sender) => {
                if let Err(err) = sender.send(EventType::Dispatch(m.clone())) {
                    m.reply(format!("Service ``r-a-dio'' unavailable: {:?}", err));
                }
            },
            None => ()
        };
    }
}
