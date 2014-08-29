use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};
use std::io::TcpStream;
use std::str::replace;
use serialize::json;
use serialize::json::{DecodeResult, ApplicationError};
use time::{get_time, Timespec};


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


static HTTP_SERVER: &'static str = "deer.satf.se";


#[deriving(Decodable, Encodable)]
struct DeerApiResponse {
    irccode: String
}


fn get_deer(deer_name: &str) -> DecodeResult<DeerApiResponse> {
    let mut stream = match TcpStream::connect(HTTP_SERVER, 80) {
        Ok(stream) => stream,
        Err(_) => return Err(ApplicationError(format!("Host Connection Error")))
    };
    let deer_name = replace(deer_name, " ", "%20");

    assert!(stream.write(format!(
        "GET /deerlist.php?deer={} HTTP/1.0\r\nHost: {}\r\n\r\n",
        deer_name.as_slice(),
        HTTP_SERVER
    ).as_bytes()).is_ok());

    let http_doc = match stream.read_to_string() {
        Ok(string) => string,
        Err(_) => return Err(ApplicationError(format!("Read Error")))
    };
    drop(stream);

    let index = match http_doc.as_slice().find_str("\r\n\r\n") {
        Some(index) => index,
        None => return Err(ApplicationError(format!("HTTP Parse Error")))
    };
    let json_doc = http_doc.as_slice().slice_from(index);
    println!("json_doc = {}", json_doc);
  
    json::decode::<DeerApiResponse>(json_doc)
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
