#![allow(dead_code)]
#![feature(if_let, slicing_syntax, phase)]

#[phase(plugin, link)] extern crate log;

extern crate time;
extern crate serialize;

extern crate url;
extern crate http;
extern crate toml;
extern crate irc;


use std::path::Path;
use std::io::fs::File;
use std::os::args_as_bytes;

use botcore::{BotConfig, BotConnection};

mod state;
mod botcore;
mod plugins;
mod command_mapper;


#[deriving(Decodable, Encodable, Show)]
struct AppConfig {
    server_host: String,
    server_port: u16,
    nickname: String,
    channels: Vec<String>
}

fn parse_appconfig() -> Option<AppConfig> {
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
    let mut parser = toml::Parser::new(contents.as_slice());
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
    toml::decode::<AppConfig>(table)
}


fn main() {
    let appconfig = match parse_appconfig() {
        Some(config) => config,
        None => fail!("bad config")
    };
    let botconfig = BotConfig {
        server: (
            appconfig.server_host.clone(),
            appconfig.server_port
        ),
        command_prefix: "!".to_string(),
        nickname: appconfig.nickname.clone(),
        channels: appconfig.channels.clone()
    };

    let conn = BotConnection::new(&botconfig);
    let conn = match conn {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    drop(conn);
}
