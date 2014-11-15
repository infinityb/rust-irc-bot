use std::task::TaskBuilder;
use std::io::IoError;

use url::{Url, ParseError};
use hyper::client::request::Request;
use hyper::header::common::server::Server;
use hyper::HttpError;
use irc::IrcMessage;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};


#[deriving(Show)]
enum WserverFailure {
    NoServerFound,
    BadUrl(ParseError),
    RequestError(HttpError)
}


fn get_wserver_result(urlstr: &str) -> Result<String, WserverFailure> {
    let url = match Url::parse(urlstr) {
        Ok(url) => url,
        Err(_) => {
            let http_url = format!("http://{}", urlstr);
            match Url::parse(http_url[]) {
                Ok(url) => url,
                Err(err) => return Err(BadUrl(err))
            }
        }
    };
    let resp = match Request::head(url) {
        Ok(req) => match req.start() {
            Ok(req) => match req.send() {
                Ok(resp) => resp,
                Err(err) => return Err(RequestError(err))
            },
            Err(err) => return Err(RequestError(err))
        },
        Err(err) => return Err(RequestError(err))
    };
    // let resp = match resp_res {
    //     Ok(resp) => resp,
    //     Err(err) => return Err(RequestError(err))
    // };
    match resp.headers.get::<Server>() {
        Some(&Server(ref server)) => Ok(server.clone()),
        None => Err(NoServerFound)
    }
}


fn format_wserver_response(resp: String) -> String {
    format!("running: {}", resp[])
}

struct WserverInternalState;


impl WserverInternalState {
    fn new() -> WserverInternalState {
        WserverInternalState
    }

    fn handle_wserver(&mut self, m: &CommandMapperDispatch) {
        let command = match m.command {
            Some(ref command) => command,
            None => return
        };
        let host = match command.get::<String>("host") {
            Some(host) => host,
            None => return
        };
        match get_wserver_result(host[]) {
            Ok(res) => {
                m.reply(format_wserver_response(res));
            }
            Err(err) => {
                m.reply(format!("Error: {}", err));
            }
        }
    }

    fn start(&mut self, rx: Receiver<CommandMapperDispatch>) {
        for m in rx.iter() {
            let command_phrase = match m.command() {
                Some(command_phrase) => command_phrase,
                None => continue
            };
            match command_phrase.command[] {
                "wserver" => self.handle_wserver(&m),
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
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("wserver {host:s}").unwrap());
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        TaskBuilder::new().named("plugin-wserver").spawn(proc() {
            let mut internal_state = WserverInternalState::new();
            internal_state.start(rx);
        });
        self.sender = Some(tx);
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send(m.clone()),
            None => ()
        }
    }
}
