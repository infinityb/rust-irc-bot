use std::convert::From;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};

use url::{Url, ParseError};
use hyper::client::request::Request;
use hyper::header::Server;
use hyper::HttpError;
use hyper::method::Method::Head;
use irc::parse::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_WSERVER: Token = Token(0);

#[derive(Debug)]
enum WserverFailure {
    NoServerFound,
    BadUrl(ParseError),
    RequestError(HttpError)
}

impl From<ParseError> for WserverFailure {
    fn from(err: ParseError) -> WserverFailure {
        WserverFailure::BadUrl(err)
    }
}

impl From<HttpError> for WserverFailure {
    fn from(err: HttpError) -> WserverFailure {
        WserverFailure::RequestError(err)
    }
}

fn get_wserver_result(urlstr: &str) -> Result<String, WserverFailure> {
    let url = match Url::parse(urlstr) {
        Ok(url) => url,
        Err(_) => {
            let http_url = format!("http://{}", urlstr);
            try!(Url::parse(&http_url))
        }
    };
    let resp = try!(try!(try!(Request::new(Head, url)).start()).send());

    match resp.headers.get::<Server>() {
        Some(&Server(ref server)) => Ok(server.clone()),
        None => Err(WserverFailure::NoServerFound)
    }
}

fn format_wserver_response(resp: String) -> String {
    format!("running: {}", resp)
}

struct WserverInternalState;


impl WserverInternalState {
    fn new() -> WserverInternalState {
        WserverInternalState
    }

    fn handle_wserver(&mut self, m: &CommandMapperDispatch) {
        let host = match m.command().get::<String>("host") {
            Some(host) => host,
            None => return
        };
        match get_wserver_result(&host) {
            Ok(res) => {
                m.reply(&format_wserver_response(res));
            }
            Err(err) => {
                m.reply(&format!("Error: {:?}", err));
            }
        }
    }

    fn start(&mut self, rx: Receiver<CommandMapperDispatch>) {
        for m in rx.iter() {
            match m.command().token {
                CMD_WSERVER => self.handle_wserver(&m),
                _ => ()
            }
        }
    }
}

pub struct WserverPlugin {
    sender: Option<SyncSender<CommandMapperDispatch>>
}


impl WserverPlugin {
    pub fn new() -> WserverPlugin {
        WserverPlugin {
            sender: None
        }
    }

    pub fn get_plugin_name() -> &'static str {
        "wserver"
    }
}

impl RustBotPlugin for WserverPlugin {
    fn configure(&mut self, configurator: &mut IrcBotConfigurator) {
        configurator.map_format(CMD_WSERVER, Format::from_str("wserver {host:s}").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        let _ = ::std::thread::Builder::new().name("plugin-wserver".to_string()).spawn(move || {
            let mut internal_state = WserverInternalState::new();
            internal_state.start(rx);
        });
        self.sender = Some(tx);
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _message: &IrcMsg) {
        match self.sender {
            Some(ref sender) => {
                if let Err(err) = sender.send(m.clone()) {
                    m.reply(&format!("Service ``wserver'' unavailable: {:?}", err));
                }
            }
            None => ()
        }
    }
}
