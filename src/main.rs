#![deny(warnings)]
#![feature(slice_patterns, slice_concat_ext)]

#![feature(plugin)]
#![plugin(phf_macros)]

#[macro_use] extern crate log;

extern crate rustc_serialize;
extern crate hyper;
extern crate irc;
extern crate rand;
extern crate time;
extern crate toml;
extern crate url;
extern crate mio;
extern crate bytes;
extern crate phf;

use std::io::Read;
use std::fs::File;
use std::env::args_os;

use botcore::BotConfig;

mod botcore;
mod plugins;
mod command_mapper;
mod irc_mio;
mod utils;

fn parse_appconfig() -> Option<BotConfig> {
    let args = args_os().collect::<Vec<_>>();

    let filename = match args.as_slice() {
        &[] => unreachable!(),
        &[_] => return None,
        &[_, ref filename] => filename,
        &[_, ref filename, ..] => filename,
    };

    let mut buf = String::new();
    let read_result = File::open(filename)
        .and_then(|mut f| f.read_to_string(&mut buf));

    if let Err(err) = read_result {
        panic!("Error reading file {:?}: {}", filename, err);
    }

    let mut parser = toml::Parser::new(&buf);
    let table = match parser.parse() {
        Some(table) => {
            let core_key = "core".to_string();
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

    println!("starting");
    botcore::run_loop(&appconfig).ok().expect("main loop failed");
}
