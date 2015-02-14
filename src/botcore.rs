use std::old_io::IoResult;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{channel, sync_channel};

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};

use irc::{IrcConnection, IrcEvent, IrcConnectionCommand, State};
use irc::parse::IrcMsg;

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


#[derive(RustcDecodable, RustcEncodable, Debug)]
pub struct BotConfig {
    pub server: String,
    pub command_prefixes: Vec<String>,
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
    foo: usize
    //
}


impl BotConnection {
    pub fn new(conf: &BotConfig) -> IoResult<BotConnection> {
        let (mut conn, event_queue) = try!(IrcConnection::new(
		(conf.get_host().as_slice(), conf.get_port())));

        let (event_queue_txu, event_queue_rxu) = channel();
        ::std::thread::Builder::new().name("bot-event-sender".to_string()).spawn(move || {
            for event in event_queue.iter() {
                event_queue_txu.send(event).unwrap();
            }
        });

        let mut nick = conf.nickname.clone();
        loop {
            info!("trying nick {}", nick.as_slice());
            match conn.register(nick.as_slice()) {
                Ok(_) => {
                    info!("ok, connected as {}", nick.as_slice());
                    break;
                }
                Err(err) => {
                    if err.should_pick_new_nickname() {
                        nick.push_str("`");
                    } else {
                        panic!("{:?}", err)
                    }
                }
            };
        }

        for channel in conf.channels.iter() {
            info!("want join: {}", channel);
        }

        for channel in conf.channels.iter() {
            info!("joining {}...", channel);
            match conn.join(channel.as_slice()) {
                Ok(res) => {
                    info!("succeeded in joining {:?}, got {} nicks",
                        res.channel.as_slice(), res.nicks.len());
                    match conn.who(channel.as_slice()) {
                        Ok(who_res) => {
                            info!("succeeded in WHOing {:?}, got {} nicks",
                                who_res.channel.as_slice(), who_res.who_records.len());
                        },
                        Err(who_err) => {
                            info!("failed to WHO {:?}: {:?}", channel, who_err);
                        }
                    }
                },
                Err(err) => {
                    info!("join error: {:?}", err);
                    panic!("failed to join channel.. dying");
                }
            }
            info!("END joining {:?}...", channel);
        }

        let mut state = State::new();

        let mut container = PluginContainer::new(conf.command_prefixes.clone());
        if conf.enabled_plugins.contains(PingPlugin::get_plugin_name()) {
            container.register(PingPlugin::new());
        }
        if conf.enabled_plugins.contains(GreedPlugin::get_plugin_name()) {
            container.register(GreedPlugin::new());
        }
        if conf.enabled_plugins.contains(SeenPlugin::get_plugin_name()) {
            container.register(SeenPlugin::new());
        }
        if conf.enabled_plugins.contains(DeerPlugin::get_plugin_name()) {
            container.register(DeerPlugin::new());
        }
        if conf.enabled_plugins.contains(RadioPlugin::get_plugin_name()) {
            container.register(RadioPlugin::new());
        }
        if conf.enabled_plugins.contains(WserverPlugin::get_plugin_name()) {
            container.register(WserverPlugin::new());
        }
        if conf.enabled_plugins.contains(WhoAmIPlugin::get_plugin_name()) {
            container.register(WhoAmIPlugin::new());
        }

        let (tx, rx) = sync_channel::<IrcMsg>(0);
        let cmd_queue = conn.get_command_queue();


        ::std::thread::Builder::new().name("bot-sender".to_string()).spawn(move || {
            for message in rx.iter() {
                cmd_queue.send(IrcConnectionCommand::raw_write(message.into_bytes())).unwrap();
            }
        });

        for event in event_queue_rxu.iter() {
            state.on_event(&event);
            if let IrcEvent::IrcMsg(ref message) = event {
                container.dispatch(Arc::new(state.clone()), &tx, message);
            }
        }

        Ok(BotConnection { foo: 0})
    }
}
