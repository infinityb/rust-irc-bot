use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};
use std::io::TcpStream;
use serialize::json;
use serialize::json::{DecodeResult, ApplicationError};


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
    creator: String,
    date: String,
    deer: String,
    irccode: String,
    kinskode: String,
    status: String
}


fn get_deer(deer_name: &str) -> DecodeResult<DeerApiResponse> {
    let mut stream = match TcpStream::connect(HTTP_SERVER, 80) {
        Ok(stream) => stream,
        Err(_) => return Err(ApplicationError(format!("Host Connection Error")))
    };

    stream.write(format!(
        "GET /deerlist.php?deer={} HTTP/1.0\r\nHost: deer.satf.se\r\n\r\n",
        deer_name
    ).as_bytes());

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


impl RustBotPlugin for DeerPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("deer");
        // conf.map("deerkins");
    }

    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        spawn(proc() {
            for (m, message) in rx.iter() {
                if message.get_args().len() < 2 {
                    continue
                }
                let unprefixed_msg = message.get_arg(1).as_slice().slice_from(1);
                let command_parts = if unprefixed_msg == "deer" {
                    ("deer", None)
                } else if unprefixed_msg.starts_with("deerkins ") {
                    ("deerkins", Some(unprefixed_msg.slice_from(9)))
                } else {
                    continue
                };
                match command_parts {
                    ("deer", _) => {
                        for deer_line in DEER.split('\n') {
                            m.reply(String::from_str(deer_line));
                        }
                    },
                    ("deerkins", Some(something)) => {
                        match get_deer(something) {
                            Ok(deer_data) => {
                                for deer_line in deer_data.irccode.as_slice().split('\n') {
                                    m.reply(String::from_str(deer_line));
                                }
                            },
                            Err(err) => {
                                m.reply(format!("error: {}", err));
                            }
                        }
                    }
                    _ => continue
                };
            }
        });
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match self.sender {
            Some(ref sender) => sender.send((m.clone(), message.clone())),
            None => ()
        };
    }
}
