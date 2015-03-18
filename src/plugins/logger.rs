use std::io::{self, Write};
use std::fs::File;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};

use time::{get_time, now_utc};
use irc::parse::IrcMsg;

use command_mapper::RustBotPlugin;

fn logger_loop(rx: Receiver<IrcMsg>) -> Result<(), io::Error> {
    let logfile = format!("logs/{}.irclog", now_utc().rfc3339());
    let mut log = try!(File::create(&logfile));
    for msg in rx.iter() {
        let timespec = get_time();
        let timefmt = format!("{}:{} ", timespec.sec, timespec.nsec);
        try!(log.write_all(timefmt.as_bytes()));
        try!(log.write_all(msg.as_bytes()));
        try!(log.write_all(b"\r\n"));
    }
    Ok(())
}


pub struct LoggerPlugin {
    sender: Option<SyncSender<IrcMsg>>
}

impl LoggerPlugin {
    pub fn new() -> LoggerPlugin {
        LoggerPlugin { sender: None }
    }

    pub fn get_plugin_name() -> &'static str {
        "logger"
    }
}

impl RustBotPlugin for LoggerPlugin {
    fn start(&mut self) {
        let (tx, rx) = sync_channel(10);
        let _ = ::std::thread::Builder::new().name("plugin-logger".to_string()).spawn(move || {
            if let Err(err) = logger_loop(rx) {
                info!("Error in logger loop: {:?}", err);
            }
        });
        self.sender = Some(tx);
    }

    fn on_message(&mut self, _: &SyncSender<IrcMsg>, msg: &IrcMsg) {
        let mut disable_self = false;
        if let Some(ref sender) = self.sender {
            if let Err(err) = sender.send(msg.clone()) {
                info!("Logger service gone: {:?}", err);
                disable_self = true;
            }
        }
        if disable_self {
            self.sender = None;
        }
    }
}
