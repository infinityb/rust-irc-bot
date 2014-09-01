use std::io::IoError;
use std::collections::hashmap::HashSet;
use std::default::Default;

use serialize::json;
use serialize::json::DecoderError;
use url::Url;
use http::client::RequestWriter;
use http::method::Get;
use time::{get_time, Timespec};


use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
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


enum RadioCommandType {
    Dj,
    DjWatch
}


fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<RadioCommandType> {
    match m.command() {
        Some("dj") => Some(Dj),
        Some("djwatch") => Some(DjWatch),
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

struct RadioMonitorState {
    last_update: Timespec,
    cur_state: Option<RadioStreamApiResponse>
}

impl RadioMonitorState {
    fn with_state(dj: RadioStreamApiResponse) -> RadioMonitorState {
        RadioMonitorState {
            last_update: get_time(),
            cur_state: Some(dj)
        }
    }

    fn update(&mut self, res: RadioStreamApiResponse)
             -> Option<(RadioStreamApiResponse,
                        RadioStreamApiResponse)> {
        let prev = self.cur_state.take();
        self.cur_state = Some(res.clone());

        match prev {
            Some(prev_state) => {
                if prev_state.djname == res.djname {
                    None
                } else {
                    Some((prev_state, res))
                }
            },
            None => None
        }
    }

    #[inline]
    fn age(&self) -> Timespec {
        get_time() - self.last_update
    }

    #[inline]
    fn is_old(&self) -> bool {
        300 < self.age().sec
    }
}


struct RadioInternalState {
    requests_made: uint,
    requests_failed: uint,
    monitor_state: Option<RadioMonitorState>,
    subscribed_channels: HashSet<String>
}


impl RadioInternalState {
    fn new() -> RadioInternalState {
        RadioInternalState {
            requests_made: 0,
            requests_failed: 0,
            monitor_state: None,
            subscribed_channels: Default::default()
        }
    }

    fn should_update_monitor(&self) -> bool {
        self.subscribed_channels.len() > 1 && match self.monitor_state {
            Some(ref monitor_state) => {
                monitor_state.is_old()
            },
            None => true
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

    fn handle_watcher(&mut self, m: &CommandMapperDispatch) {
        if self.should_update_monitor() {
            let res = match get_radio_api_result() {
                Ok(res) => res.main,
                Err(_) => return
            };
            let now = get_time();
            println!("{} current DJ is: {}", now.sec, res.djname);
            let old_monitor_state = self.monitor_state.take();
            self.monitor_state = Some(RadioMonitorState::with_state(res.clone()));
            let mut old_monitor_state = match old_monitor_state {
                Some(monitor_state) => monitor_state,
                None => return
            };
            match old_monitor_state.update(res) {
                Some((old_state, cur_state)) => {
                    for chan in self.subscribed_channels.iter() {
                        m.reply_raw(format!(
                            "PRIVMSG {} :dj changed from {} to {}",
                            chan,
                            old_state.djname,
                            cur_state.djname));
                    }
                },
                None => ()
            }
        }
    }

    fn handle_subscribe(&mut self, m: &CommandMapperDispatch, msg: &IrcMessage) {
        if self.subscribed_channels.len() == 0 {
            self.monitor_state = None;
        }
        let channel_name = match msg.channel() {
            Some(channel_name) => String::from_str(channel_name),
            None => return
        };
        let is_removed = self.subscribed_channels.remove(&channel_name);
        if !is_removed {
            self.subscribed_channels.insert(channel_name.clone());
            m.reply(format!("{} has been subscribed", channel_name.as_slice()));
        } else {
            m.reply(format!("{} has been unsubscribed", channel_name.as_slice()));
        }
    }

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMessage)>) {
        for (m, message) in rx.iter() {
            match parse_command(&m) {
                Some(Dj) => self.handle_dj(&m),
                Some(DjWatch) => self.handle_subscribe(&m, &message),
                None => self.handle_watcher(&m)
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
        conf.map("djwatch");
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        spawn(proc() {
            let mut internal_state = RadioInternalState::new();
            internal_state.start(rx);
        });
    }

    fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send((m.clone(), message.clone())),
            None => ()
        };
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send((m.clone(), message.clone())),
            None => ()
        };
    }
}
