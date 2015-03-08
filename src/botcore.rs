use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{channel, sync_channel};
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::net::TcpStream;
use irc::stream::{IrcConnector, RegisterReqBuilder};

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};

use irc::{BundlerManager, JoinBundlerTrigger};
use irc::parse::IrcMsg;
use irc::message_types::{client, server};

use command_mapper::PluginContainer;

use plugins::{
    DeerPlugin,
    GreedPlugin,
    SeenPlugin,
    RadioPlugin,
    PingPlugin,
    WserverPlugin,
    WhoAmIPlugin,
    LoggerPlugin,
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
    pub fn new(conf: &BotConfig) -> io::Result<BotConnection> {
        let (tx, rx) = sync_channel::<IrcMsg>(100);
        let (event_queue_txu, event_queue_rxu) = channel::<IrcMsg>();

        let conn = try!(TcpStream::connect(&(
            conf.get_host().as_slice(),
            conf.get_port()
        )));

        let mut reg_req = RegisterReqBuilder::new()
            .nick(&conf.nickname)
            .user("rustirc")
            .realname("https://github.com/infinityb/rust-irc-bot")
            .mode_invisible(true)
            .build().ok().unwrap();

        let mut connector = IrcConnector::from_pair(
            Box::new(BufReader::new(conn.try_clone().ok().unwrap())),
            Box::new(conn.try_clone().ok().unwrap()));

        let mut state;
        loop {
            match connector.register(&reg_req) {
                Ok(s) => {
                    state = s;
                    break
                },
                Err(err) => {
                    println!("Registration Error: {:?}", err);
                    reg_req.get_mut_nick().push_str("`");
                }
            }
        }
        let (mut reader, mut writer) = connector.split();

        let _ = ::std::thread::Builder::new().name("bot-reader".to_string()).spawn(move || {
            for msg in reader.iter() {
                let msg = match msg {
                    Ok(msg) => msg,
                    Err(err) => panic!("Error parsing message: {:?}", err),
                };
                if let Err(err) = event_queue_txu.send(msg) {
                    panic!("Error sending message: {:?}", err);
                }
            }
        });

        let mut bundler_man = BundlerManager::with_defaults();

        bundler_man.add_bundler_trigger(Box::new(
            JoinBundlerTrigger::new(state.get_self_nick().as_bytes())));

        for channel in conf.channels.iter() {
            info!("want join: {}", channel);
        }

        for channel in conf.channels.iter() {
            let join_msg = client::Join::new(channel.as_slice());
            writer.write_irc_msg(join_msg.to_irc_msg()).ok().unwrap();
        }
        
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
        if conf.enabled_plugins.contains(LoggerPlugin::get_plugin_name()) {
            container.register(LoggerPlugin::new());
        }

        let _ = ::std::thread::Builder::new().name("bot-writer".to_string()).spawn(move || {
            for irc_msg in rx.iter() {
                println!("bot-sender::irc_msg = {:?}", irc_msg);
                writer.write_irc_msg(&irc_msg).ok().unwrap();
            }
        });
        
        let autojoin_invited: HashSet<String> = conf.channels.iter().cloned().collect();

        for msg in event_queue_rxu.iter() {
            if let server::IncomingMsg::Ping(ping) = server::IncomingMsg::from_msg(msg.clone()) {
                if let Ok(pong) = ping.get_response() {
                    tx.send(pong.into_irc_msg()).ok().unwrap();
                }
            }

            if let server::IncomingMsg::Invite(invite) = server::IncomingMsg::from_msg(msg.clone()) {
                if autojoin_invited.contains(invite.get_target()) {
                    let join_msg = client::Join::new(invite.get_target());
                    tx.send(join_msg.into_irc_msg()).ok().unwrap();
                }
            }

            if let Some(join) = state.is_self_join(&msg) {
                tx.send(client::Who::new(join.get_channel()).into_irc_msg()).ok().expect("send fail");
            }
            for event in bundler_man.on_irc_msg(&msg).into_iter() {
                state.on_event(&event);
                println!("emit-event {:?} => state = {:?}", event, state);
            }
            container.dispatch(Arc::new(state.clone_frozen()), &tx, &msg);
        }
        println!("Finished popping from event_queue_rxu");

        Ok(BotConnection { foo: 0 })
    }
}
