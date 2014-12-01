use std::io::IoResult;
use std::task::TaskBuilder;
use std::collections::HashSet;
use std::sync::Arc;

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};

use irc::{IrcConnection, IrcEvent, IrcConnectionCommand, State};

use command_mapper::PluginContainer;

use plugins::{
    DeerPlugin,
    GreedPlugin,
    SeenPlugin,
    RadioPlugin,
    PingPlugin,
    WserverPlugin,
    WhoAmIPlugin,
};


#[deriving(Decodable, Encodable, Show)]
pub struct BotConfig {
    pub server: String,
    pub command_prefix: String,
    pub nickname: String,
    pub channels: Vec<String>,
    pub enabled_plugins: HashSet<String>,
}

pub fn irc_scheme_type_mapper(scheme: &str) -> SchemeType {
    match scheme {
        "irc" => SchemeType::Relative(6667),
        "ircs" => SchemeType::Relative(6697),
        _ => SchemeType::NonRelative,
    }    
}


impl BotConfig {
    fn get_url(&self) -> ParseResult<Url> {
        let mut parser = UrlParser::new();
        parser.scheme_type_mapper(irc_scheme_type_mapper);
        parser.parse(self.server.as_slice())
    }

    fn get_host(&self) -> String {
        let server = self.get_url().unwrap();
        match server.host() {
            Some(&Host::Domain(ref string)) => string.clone(),
            Some(&Host::Ipv6(ref addr)) => addr.serialize(),
            None => panic!()
        }
    }

    fn get_port(&self) -> u16 {
        let server = self.get_url().unwrap();
        server.port().unwrap_or(6667)
    }
}

pub struct BotConnection {
    foo: uint
    //
}


impl BotConnection {
    pub fn new(conf: &BotConfig) -> IoResult<BotConnection> {
        let (mut conn, event_queue) = try!(IrcConnection::new(
		(conf.get_host().as_slice(), conf.get_port())));

        let (event_queue_txu, event_queue_rxu) = channel();
        spawn(proc() {
            for event in event_queue.iter() {
                event_queue_txu.send(event);
            }
        });

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
                        panic!("{}", err)
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
                    panic!("failed to join channel.. dying");
                }
            }
            println!("END joining {}...", channel);
        }

        let mut state = State::new();

        let mut container = PluginContainer::new(conf.command_prefix.clone());
        if conf.enabled_plugins.contains(PingPlugin::get_plugin_name()) {
            container.register(box PingPlugin::new());
        }
        if conf.enabled_plugins.contains(GreedPlugin::get_plugin_name()) {
            container.register(box GreedPlugin::new());
        }
        if conf.enabled_plugins.contains(SeenPlugin::get_plugin_name()) {
            container.register(box SeenPlugin::new());
        }
        if conf.enabled_plugins.contains(DeerPlugin::get_plugin_name()) {
            container.register(box DeerPlugin::new());
        }
        if conf.enabled_plugins.contains(RadioPlugin::get_plugin_name()) {
            container.register(box RadioPlugin::new());
        }
        if conf.enabled_plugins.contains(WserverPlugin::get_plugin_name()) {
            container.register(box WserverPlugin::new());
        }
        if conf.enabled_plugins.contains(WhoAmIPlugin::get_plugin_name()) {
            container.register(box WhoAmIPlugin::new());
        }

        let (tx, rx) = sync_channel(0);
        let cmd_queue = conn.get_command_queue();

        TaskBuilder::new().named("bot-sender").spawn(proc() {
            for message in rx.iter() {
                cmd_queue.send(IrcConnectionCommand::raw_write(message));
            }
        });

        for event in event_queue_rxu.iter() {
            state.on_event(&event);
            if let IrcEvent::Message(ref message) = event {
                container.dispatch(Arc::new(state.clone()), &tx, message);
            }
        }

        Ok(BotConnection { foo: 0})
    }
}
