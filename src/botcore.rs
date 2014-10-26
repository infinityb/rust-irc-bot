use std::io::IoResult;
use std::task::TaskBuilder;

use irc::IrcConnection;
use irc::connection::RawWrite;
use irc::event::IrcEventMessage;

use command_mapper::PluginContainer;

use state::State;
use plugins::{
    DeerPlugin,
    GreedPlugin,
    SeenPlugin,
    RadioPlugin,
    PingPlugin,
    WserverPlugin,
};

pub struct BotConfig {
    pub server: (String, u16),
    pub command_prefix: String,
    pub nickname: String,
    pub channels: Vec<String>
}

pub struct BotConnection {
    foo: uint
    //
}

impl BotConnection {
    pub fn new(conf: &BotConfig) -> IoResult<BotConnection> {
        let (ref host, port) = conf.server;
        let (mut conn, event_queue) = try!(IrcConnection::new(host.as_slice(), port));

        let mut nick = conf.nickname.clone();
        loop {
            println!("trying nick {}", nick.as_slice());
            match conn.register(nick.as_slice()) {
                Ok(_) => {
                    println!("ok, connected as {}", nick.as_slice());
                    break;
                }
                Err(err) => {
                    if err.should_pick_new_nickname() {
                        nick.push_str("`");
                    } else {
                        fail!("{}", err)
                    }
                }
            };
        }

        for channel in conf.channels.iter() {
            println!("want join: {}", channel);
        }

        for channel in conf.channels.iter() {
            println!("joining {}...", channel);
            match conn.join(channel.as_slice()) {
                Ok(res) => {
                    println!("succeeded in joining {}, got {} nicks",
                        res.channel.as_slice(), res.nicks.len());
                    match conn.who(channel.as_slice()) {
                        Ok(who_res) => {
                            println!("succeeded in WHOing {}, got {} nicks",
                                who_res.channel.as_slice(), who_res.who_records.len());
                        },
                        Err(who_err) => {
                            println!("failed to WHO {}: {}", channel, who_err);
                        }
                    }
                },
                Err(err) => {
                    println!("join error: {}", err);
                    fail!("failed to join channel.. dying");
                }
            }
            println!("END joining {}...", channel);
        }

        let mut state = State::new();

        let mut container = PluginContainer::new(conf.command_prefix.clone());
        container.register(box PingPlugin::new());
        container.register(box GreedPlugin::new());
        container.register(box SeenPlugin::new());
        container.register(box DeerPlugin::new());
        container.register(box RadioPlugin::new());
        container.register(box WserverPlugin::new());

        let (tx, rx) = sync_channel(0);
        let cmd_queue = conn.get_command_queue();
        TaskBuilder::new().named("bot-sender").spawn(proc() {
            for message in rx.iter() {
                cmd_queue.send(RawWrite(message));
            }
        });

        for event in event_queue.iter() {
            state.on_event(&event);

            if let IrcEventMessage(ref message) = event {
                // println!("{}", message);
                container.dispatch("", &tx, message);
            }           
        }

        Ok(BotConnection { foo: 0})
    }
}