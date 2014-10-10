use std::task::TaskBuilder;
use std::io::IoError;

use url::{Url, ParseError};
use http::client::RequestWriter;
use http::method::Head;


use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    StringValue,
};
use message::{
    IrcMessage
};


#[deriving(Show)]
enum WserverFailure {
    BadUrl(ParseError),
    NoServerFound,
    WriterCreateError(IoError),
    NoResponseError(IoError),
    ResponseReadError(IoError),
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

    let request: RequestWriter = match RequestWriter::new(Head, url) {
        Ok(request) => request,
        Err(err) => return Err(WriterCreateError(err))
    };
    let mut response = match request.read_response() {
        Ok(response) => response,
        Err((_, io_error)) => return Err(NoResponseError(io_error))
    };
    match response.read_to_end() {
        Ok(_) => (),
        Err(io_error) => return Err(ResponseReadError(io_error))
    };
    match response.headers.server {
        Some(server) => Ok(server),
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
        let host = match command.args.find(&"host".to_string()) {
            Some(&StringValue(ref host)) => host,
            Some(_) => return,
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

    fn start(&mut self, rx: Receiver<WserverCommand>) {
        for comm in rx.iter() {
            match comm {
                Ping => (),
                Dispatch(m) => {
                    let command_phrase = match m.command() {
                        Some(command_phrase) => command_phrase,
                        None => continue
                    };
                    match command_phrase.command[] {
                        "wserver" => self.handle_wserver(&m),
                        _ => ()
                    };
                }
            }
        }
    }
}

pub struct WserverPlugin {
    sender: Option<SyncSender<WserverCommand>>
}


impl WserverPlugin {
    pub fn new() -> WserverPlugin {
        WserverPlugin {
            sender: None
        }
    }

    fn is_remote_up(&mut self) -> bool {
        let is_remote_up = match self.sender {
            Some(ref sender) => {
                sender.send_opt(Ping).is_ok()
            }
            None => false
        };
        if !is_remote_up && self.sender.is_some() {
            self.sender = None;
        }
        is_remote_up
    }
}

enum WserverCommand {
    Ping,
    Dispatch(CommandMapperDispatch)
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
        loop {
            let remote_went_down = {
                let sender = match self.sender {
                    Some(ref sender) => sender,
                    None => return
                };
                sender.send_opt(Dispatch(m.clone())).is_err()
            };
            if remote_went_down {
                m.reply("wserver plugin crashed. restarting...".to_string());
                self.start();
            } else {
                break;
            }
        }
    }
}
