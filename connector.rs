#![allow(dead_code)]

extern crate irc;
extern crate debug;

use std::fmt;
use std::collections::{RingBuf, Deque};
use std::comm::channel;
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

use irc::{
    IrcMessage,
    IrcConnection,
    IrcEventMessage,
    IrcEventBundle,
    IrcEventWatcherResponse,
    RegisterError,
    RegisterErrorType,
};


fn main() {
    let mut conn = match IrcConnection::new("127.0.0.1", 6667) {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    let mut nick = String::from_str("platy");

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
    
    println!("joining #dicks...");
    match conn.join("#dicks") {
        Ok(res) => {
            println!("succeeded in joining {}, got {} nicks",
                res.channel.as_slice(), res.nicks.len());
        },
        Err(err) => {
            println!("join error: {:?}", err);
            fail!("failed to join channel.. dying");
        }
    }

    loop {      
        match conn.recv() {
            IrcEventMessage(message) => {
                println!("RX: {}", message);
                if message.get_command().as_slice() == "PING" {
                    let response = format!("PONG :{}\n", message.get_arg(0));
                    println!("TX: {}", response.as_slice());
                    match conn.write_str(response.as_slice()) {
                        Ok(_) => (),
                        Err(err) => fail!("Error writing to IRC server: {}", err)
                    }
                }
            },
            IrcEventBundle(event) => {
                println!("got bundle back: {}", event.pretty_print());
            },
            IrcEventWatcherResponse(watcher) => {
                println!("got watcher back: {}", watcher.pretty_print());
            }
        }
    }
}
