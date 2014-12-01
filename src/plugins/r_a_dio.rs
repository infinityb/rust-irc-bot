use std::task::TaskBuilder;
use std::io::IoError;
use std::error::FromError;

use serialize::json;
use serialize::json::DecoderError;
use url::Url;
use hyper::client::request::Request;
use hyper::HttpError;

use irc::IrcMessage;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};


static API_URL: &'static str = "http://r-a-d.io/api/";


#[deriving(Decodable, Encodable, Clone)]
struct RadioApiResponse {
    main: RadioStreamApiResponse
}


#[deriving(Decodable, Encodable, Clone)]
struct RadioStreamApiResponse {
    np: String,
    listeners: uint,
    djname: String
}


#[deriving(Show)]
enum RadioApiFailure {
    ResponseDecodeError,
    RequestError(HttpError),
    ResponseReadError(IoError),
    ResponseDeserializeError(DecoderError)
}


impl FromError<HttpError> for RadioApiFailure {
    fn from_error(err: HttpError) -> RadioApiFailure {
        RadioApiFailure::RequestError(err)
    }
}

impl FromError<IoError> for RadioApiFailure {
    fn from_error(err: IoError) -> RadioApiFailure {
        RadioApiFailure::ResponseReadError(err)
    }
}

impl FromError<DecoderError> for RadioApiFailure {
    fn from_error(err: DecoderError) -> RadioApiFailure {
        RadioApiFailure::ResponseDeserializeError(err)
    }
}


fn get_radio_api_result() -> Result<RadioApiResponse, RadioApiFailure> {
    info!("Making r/a/dio API request");
    let url = Url::parse(API_URL).ok().expect("Invalid URL :-(");
    let mut resp = try!(try!(try!(Request::get(url)).start()).send());
    let body = match String::from_utf8(try!(resp.read_to_end())) {
        Ok(body) => body,
        Err(_err) => return Err(RadioApiFailure::ResponseDecodeError)
    };
    info!("r/a/dio result: ``{}''", body.as_slice());
    Ok(try!(json::decode::<RadioApiResponse>(body.as_slice())))
}


fn format_radio_stream_response(resp: RadioStreamApiResponse) -> String {
    format!("{} \u2014 np: {}", resp.djname.as_slice(), resp.np.as_slice())
}

struct RadioInternalState {
    requests_made: uint,
    requests_failed: uint,
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
                m.reply(format!("Error: {}", err));
            }
        }
    }
    fn start(&mut self, rx: Receiver<EventType>) {
        for event in rx.iter() {
            match event {
                EventType::Dispatch(dispatch) => {
                    match dispatch.command().command[] {
                        "dj" => self.handle_dj(&dispatch),
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
        conf.map_format(Format::from_str("dj").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        
        TaskBuilder::new().named("plugin-radio").spawn(proc() {
            let mut internal_state = RadioInternalState::new();
            internal_state.start(rx);
        });

        self.sender = Some(tx);
    }
    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send(EventType::Dispatch(m.clone())),
            None => ()
        };
    }
}
