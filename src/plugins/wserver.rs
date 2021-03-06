use std::convert::From;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};

use hyper;
use hyper::header::Server;

use irc::IrcMsg;

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
    RequestError(hyper::Error)
}

impl From<hyper::Error> for WserverFailure {
    fn from(err: hyper::Error) -> WserverFailure {
        WserverFailure::RequestError(err)
    }
}

fn get_wserver_result(urlstr: &str) -> Result<String, WserverFailure> {
    let mut url = urlstr.to_string();
    if !urlstr.starts_with("http") {
        url = format!("http://{}", urlstr);
    }

    let client = hyper::Client::new();
    let resp = try!(client.head(&url).send());

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
