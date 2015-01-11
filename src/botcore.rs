use std::io::net::tcp::TcpStream;
use std::io::BufferedReader;
use std::io::IoResult;
use std::io::IoErrorKind::EndOfFile;
use std::iter::IteratorExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::thread::{self, Thread};
use std::sync::mpsc::{sync_channel, channel};

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};

use irc::{IrcConnectionBuf, IrcEvent, State};

use command_mapper::PluginContainer;

// use plugins::{
//     DeerPlugin,
//     GreedPlugin,
//     SeenPlugin,
//     RadioPlugin,
//     PingPlugin,
//     WserverPlugin,
//     WhoAmIPlugin,
//     WaifuPlugin,
// };


#[derive(RustcDecodable, RustcEncodable, Show)]
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
    reader: Thread,
    writer: Thread,
    dispatch: Thread,
}


impl BotConnection {
    // pub fn join(self) -> thread::Result<()> {
    //     match (self.reader.join(), self.writer.join(), self.dispatch.join()) {
    //         (Ok(()), Ok(()), Ok(())) => Ok(()),
    //         (Err(err), _, _) => Err(err),
    //         (Ok(()), Err(err), _) => Err(err),
    //         (Ok(()), Ok(()), Err(err)) => Err(err),
    //     }
    // }
    
    pub fn new(conf: &BotConfig) -> IoResult<BotConnection> {
        let (server_txline_tx, server_txline_rx) = sync_channel::<Vec<u8>>(20);
        let (server_rxline_tx, server_rxline_rx) = channel::<Vec<u8>>();
        
        let dst_host = conf.get_host();
        let conn = try!(TcpStream::connect((dst_host.as_slice(), conf.get_port())));

        let reader_conn = conn.clone();
        let reader_guard = Thread::spawn(move |:| {
            let mut conn = BufferedReader::new(reader_conn);
            loop {
                match conn.read_until(b'\n') {
                    Ok(bytes) => {
                        println!("SERVER-READ: {:?}", ::std::str::from_utf8(bytes.as_slice()));
                        assert!(server_rxline_tx.send(bytes).is_ok());
                    }
                    Err(ref err) if err.kind == EndOfFile => break,
                    // FIXME
                    Err(err) => panic!("error reading socket: {}", err),
                }
            }
        });

        let mut writer_conn = conn.clone();
        let writer_guard = Thread::spawn(move |:| {
            for line in server_txline_rx.iter() {
                println!("SERVER-WRITE: {:?}", ::std::str::from_utf8(line.as_slice()));
                if writer_conn.write(line.as_slice()).is_err() {
                    break;
                }
            }
        });

        let mut container = PluginContainer::new(conf.command_prefix.clone());
        // if conf.enabled_plugins.contains(PingPlugin::get_plugin_name()) {
        //     container.register(box PingPlugin::new());
        // }
        // if conf.enabled_plugins.contains(GreedPlugin::get_plugin_name()) {
        //     container.register(box GreedPlugin::new());
        // }
        // if conf.enabled_plugins.contains(SeenPlugin::get_plugin_name()) {
        //     container.register(box SeenPlugin::new());
        // }
        // if conf.enabled_plugins.contains(DeerPlugin::get_plugin_name()) {
        //     container.register(box DeerPlugin::new());
        // }
        // if conf.enabled_plugins.contains(RadioPlugin::get_plugin_name()) {
        //     container.register(box RadioPlugin::new());
        // }
        // if conf.enabled_plugins.contains(WserverPlugin::get_plugin_name()) {
        //     container.register(box WserverPlugin::new());
        // }
        // if conf.enabled_plugins.contains(WhoAmIPlugin::get_plugin_name()) {
        //     container.register(box WhoAmIPlugin::new());
        // }
        // if conf.enabled_plugins.contains(WaifuPlugin::get_plugin_name()) {
        //     container.register(box WaifuPlugin::new());
        // }

        let mut nick = conf.nickname.clone();
        let chanels = conf.channels.clone();

        
        let dispatch_guard = thread::Builder::new().name("bot-dispatch".to_string()).spawn(move |:| {
            let mut state = State::new();
            let mut connbuf = IrcConnectionBuf::new();

            loop {
                println!("trying nick {}", nick.as_slice());
                let mut reg_fut = connbuf.register(nick.as_slice());
                while let Some(event) = connbuf.pop_event() {
                    println!("Dropped event while registering: {:?}", event);
                }
                println!("finished dispatching");

                while let Some(line) = connbuf.pop_line() {
                    assert!(server_txline_tx.send(line).is_ok());
                }
                println!("finished sending");

                println!("waiting for registration");
                match reg_fut.get() {
                    Ok(_) => {
                        println!("ok, connected as {}", nick.as_slice());
                        break;
                    }
                    Err(err) => {
                        if err.should_pick_new_nickname() {
                            nick.push_str("`");
                        } else {
                            panic!("{:?}", err)
                        }
                    }
                }
            }

            let join_futures: Vec<_> = chanels.iter()
                .map(|&mut: ch| connbuf.join(ch.as_slice())).collect();
            println!("join_futures: {}", join_futures.len());

            while let Some(event) = connbuf.pop_event() {
                println!("Dropped event while registering: {:?}", event);
            }
            while let Some(line) = connbuf.pop_line() {
                assert!(server_txline_tx.send(line).is_ok());
            }


            for line in server_rxline_rx.iter() {
                connbuf.push_line(line);
                while let Some(event) = connbuf.pop_event() {
                    state.on_event(&event);

                    if let IrcEvent::IrcMsg(ref message) = event {
                        container.dispatch(Arc::new(state.clone()), &server_txline_tx, message);
                    }
                }
                while let Some(line) = connbuf.pop_line() {
                    assert!(server_txline_tx.send(line).is_ok());
                }
            }
        });

        Ok(BotConnection {
            reader: reader_guard,
            writer: writer_guard,
            dispatch: dispatch_guard,
        })
    }
}
