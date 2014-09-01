#![allow(dead_code)]
extern crate toml;
extern crate irc;
extern crate debug;
extern crate serialize;


use std::path::Path;
use std::io::fs::File;
use std::os::args_as_bytes;

use toml::Parser;


use irc::{
    IrcConnection,
};


#[deriving(Decodable, Encodable)]
struct BotConfig {
    server_host: String,
    server_port: u16,
    nickname: String,
    channels: Vec<String>
}


fn parse_botconfig() -> Option<BotConfig> {
    let filename = Path::new(match args_as_bytes().as_slice() {
        [] => fail!("impossible"),
        [_] => return None,
        [_, ref filename] => filename.clone(),
        [_, ref filename, ..] => filename.clone()
    });
    let mut file = match File::open(&filename) {
        Ok(file) => file,
        Err(err) => fail!("{}", err)
    };
    let contents = match file.read_to_string() {
        Ok(contents) => contents,
        Err(err) => fail!("{}", err)
    };
    let mut parser = Parser::new(contents.as_slice());
    let table = match parser.parse() {
        Some(table) => {
            let core_key = String::from_str("core");
            match table.find(&core_key) {
                Some(value) => value.clone(),
                None => fail!("failed to parse in some way.")
            }
        }
        None => fail!("failed to parse in some way.")
    };
    toml::decode::<BotConfig>(table)
}


fn main() {
    let botconfig = match parse_botconfig() {
        Some(config) => config,
        None => fail!("bad config")
    };

    let conn = IrcConnection::new(
        botconfig.server_host.as_slice(),
        botconfig.server_port);

    let (mut conn, event_queue) = match conn {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    let mut nick = botconfig.nickname.clone();

    loop {
        println!("trying nick {}", nick.as_slice());
        match conn.register(nick.as_slice()) {
            Ok(_) => {
                println!("ok, connected as {}", nick.as_slice());
                break;
            }
            Err(err) => {
                if err.should_pick_new_nickname() {
                    nick = nick.append("`");
                } else {
                    fail!("{:?}", err)
                }
            }
        };
    }
    
    for channel in botconfig.channels.iter() {
        println!("joining {}...", channel);
        match conn.join(channel.as_slice()) {
            Ok(res) => {
                println!("succeeded in joining {}, got {} nicks",
                    res.channel.as_slice(), res.nicks.len());
            },
            Err(err) => {
                println!("join error: {:?}", err);
                fail!("failed to join channel.. dying");
            }
        }
    }

    loop {
        println!("{:?}", event_queue.recv());
        // match  {
        //     IrcEventMessage(message) => {
        //         println!("RX: {}", message);
        //     },
        //     IrcEventBundle(event) => {
        //         // println!("got bundle back: {}", event.pretty_print());
        //     },
        //     IrcEventWatcherResponse(watcher) => {
        //         // println!("got watcher back: {}", watcher.pretty_print());
        //     }
        // }
    }
}
