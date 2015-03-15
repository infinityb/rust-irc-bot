#![deny(unused_must_use)]
#![feature(collections, core, net, rustc_private, std_misc)]

#[macro_use] extern crate log;


extern crate "rustc-serialize" as rustc_serialize;

extern crate hyper;
extern crate irc;
extern crate rand;
extern crate time;
extern crate toml;
extern crate url;

use std::io::Read;
use std::fs::File;
use std::env::args_os;

use botcore::{BotConfig, BotConnection};

mod botcore;
mod plugins;
mod command_mapper;


fn parse_appconfig() -> Option<BotConfig> {
    let args = args_os().collect::<Vec<_>>();

    let filename = match args.as_slice() {
        [] => unreachable!(),
        [_] => return None,
        [_, ref filename] => filename,
        [_, ref filename, ..] => filename,
    };

    let mut buf = String::new();
    let read_result = File::open(filename)
        .and_then(|mut f| f.read_to_string(&mut buf));
    
    if let Err(err) = read_result {
        panic!("Error reading file {:?}: {}", filename, err);
    }

    let mut parser = toml::Parser::new(buf.as_slice());
    let table = match parser.parse() {
        Some(table) => {
            let core_key = String::from_str("core");
            match table.get(&core_key) {
                Some(value) => value.clone(),
                None => panic!("failed to parse in some way.")
            }
        }
        None => panic!("failed to parse in some way.")
    };
    toml::decode::<BotConfig>(table)
}


fn main() {
    let appconfig = match parse_appconfig() {
        Some(config) => config,
        None => panic!("bad config")
    };

    let conn = BotConnection::new(&appconfig);
    let conn = match conn {
        Ok(stream) => stream,
        Err(err) => panic!("{}", err)
    };
    drop(conn);
}