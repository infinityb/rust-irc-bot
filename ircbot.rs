#![allow(dead_code)]

extern crate irc;
extern crate debug;


use irc::{
    IrcConnection,
    IrcEventMessage,
    IrcEventBundle,
    IrcEventWatcherResponse,
};


fn main() {
    let (mut conn, event_queue) = match IrcConnection::new("127.0.0.1", 6667) {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    let mut nick = String::from_str("nano");

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
    
    println!("joining #...");
    match conn.join("#") {
        Ok(res) => {
            println!("succeeded in joining {}, got {} nicks",
                res.channel.as_slice(), res.nicks.len());
        },
        Err(err) => {
            println!("join error: {:?}", err);
            fail!("failed to join channel.. dying");
        }
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
        match event_queue.recv() {
            IrcEventMessage(message) => {
                println!("RX: {}", message);
                if message.get_command().as_slice() == "PING" {
                    let response = format!("PONG :{}\n", message.get_arg(0));
                    println!("TX: {}", response.as_slice());
                    conn.write_str(response.as_slice())
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
