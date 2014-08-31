use std::io::IoError;

use serialize::json;
use serialize::json::DecoderError;
use url::Url;
use http::client::RequestWriter;
use http::method::Get;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


static API_URL: &'static str = "http://r-a-d.io/api/";


#[deriving(Decodable, Encodable)]
struct RadioApiResponse {
    main: RadioStreamApiResponse
}


#[deriving(Decodable, Encodable)]
struct RadioStreamApiResponse {
    np: String,
    listeners: uint,
    djname: String
}


enum RadioCommandType {
    Dj,
}


fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<RadioCommandType> {
    match m.command() {
        Some("dj") => Some(Dj),
        Some(_) => None,
        None => None
    }
}


#[deriving(Show)]
enum RadioApiFailure {
    NoResponseError(IoError),
    ResponseReadError(IoError),
    ResponseDecodeError,
    ResponseDeserializeError(DecoderError)
}


fn get_radio_api_result() -> Result<RadioApiResponse, RadioApiFailure> {
    let url = Url::parse(API_URL).ok().expect("Invalid URL :-(");
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

    match json::decode::<RadioApiResponse>(body.as_slice()) {
        Ok(result) => Ok(result),
        Err(error) => Err(ResponseDeserializeError(error))
    }
}


fn format_radio_stream_response(resp: RadioStreamApiResponse) -> String {
    format!("{} \u2014 np: {}", resp.djname.as_slice(), resp.np.as_slice())
}


struct RadioInternalState {
    requests_made: uint,
    requests_failed: uint
}


impl RadioInternalState {
    fn new() -> RadioInternalState {
        RadioInternalState {
            requests_made: 0,
            requests_failed: 0
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

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMessage)>) {
        for (m, message) in rx.iter() {
            match parse_command(&m) {
                Some(Dj) => self.handle_dj(&m),
                None => ()
            }
        }
    }
}


pub struct RadioPlugin {
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMessage)>>
}


impl RadioPlugin {
    pub fn new() -> RadioPlugin {
        RadioPlugin {
            sender: None
        }
    }
}


impl RustBotPlugin for RadioPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("dj");
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        spawn(proc() {
            let mut internal_state = RadioInternalState::new();
            internal_state.start(rx);
        });
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send((m.clone(), message.clone())),
            None => ()
        };
    }
}
