#![deny(unused_must_use)]
#![feature(slicing_syntax, rustc_private, collections, io, os, core, std_misc, path, rand)]

#[macro_use] extern crate log;


extern crate time;
extern crate "rustc-serialize" as rustc_serialize;

extern crate url;
extern crate hyper;
extern crate toml;
extern crate irc;

use std::path::Path;
use std::old_io::fs::File;
use std::os::args_as_bytes;

use botcore::{BotConfig, BotConnection};

mod botcore;
mod plugins;
mod command_mapper;


fn parse_appconfig() -> Option<BotConfig> {
    let filename = Path::new(match args_as_bytes().as_slice() {
        [] => panic!("impossible"),
        [_] => return None,
        [_, ref filename] => filename.clone(),
        [_, ref filename, ..] => filename.clone()
    });
    let mut file = match File::open(&filename) {
        Ok(file) => file,
        Err(err) => panic!("{}", err)
    };
    let contents = match file.read_to_string() {
        Ok(contents) => contents,
        Err(err) => panic!("{}", err)
    };
    let mut parser = toml::Parser::new(contents.as_slice());
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