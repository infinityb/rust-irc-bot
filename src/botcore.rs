use std::collections::HashSet;
use std::sync::Arc;

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};
use mio::{NonBlock, ReadHint, EventLoop, EventLoopConfig, Token, IntoNonBlock};
use mio::tcp::TcpStream;

use irc::{BundlerManager, JoinBundlerTrigger};
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::State;

use irc_mio::IrcMsgRingBuf;
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
    FetwgrkifgPlugin,
    AsciiArtPlugin,
    AnimeCalendarPlugin,
};


#[derive(RustcDecodable, RustcEncodable, Debug)]
pub struct BotConfig {
    pub server: String,
    pub command_prefixes: Vec<String>,
    pub username: String,
    pub realname: String,
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
        parser.parse(&self.server)
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

const CLIENT: Token = Token(1);

mod ping {
    use time::{Duration, SteadyTime};

    enum PingState {
        Pending(SteadyTime), // since when
        Good(SteadyTime), // since when
    }

    pub enum NextPing {
        Unknown, // If we are in pending state
        Now,
        Future(Duration),
    }

    impl NextPing {
        pub fn is_now(&self) -> bool {
            match *self {
                NextPing::Now => true,
                _ => false,
            }
        }
    }

    pub struct PingManager {
        interval: Duration,
        max_lag: Duration,
        state: PingState,
    }

    impl PingManager {
        pub fn new() -> PingManager {
            let now = SteadyTime::now();
            PingManager {
                interval: Duration::minutes(2),
                max_lag: Duration::minutes(5),
                state: PingState::Good(now),
            }
        }

        pub fn should_terminate(&self) -> bool {
            let now = SteadyTime::now();
            match self.state {
                PingState::Pending(st) => self.max_lag < (now - st),
                PingState::Good(_) => false,
            }
        }

        pub fn ping_sent(&mut self) {
            self.state = PingState::Pending(SteadyTime::now());
        }

        pub fn pong_received(&mut self) {
            let now = SteadyTime::now();
            match self.state {
                PingState::Pending(_st) => {
                    self.state = PingState::Good(now);
                },
                PingState::Good(_) => {
                    warn!("ping-state already in good condition; unsolicited PONG?");
                }
            }
        }

        pub fn next_ping(&mut self) -> NextPing {
            let now = SteadyTime::now();
            match self.state {
                PingState::Pending(_) => NextPing::Unknown,
                PingState::Good(st) if (now - st) < self.interval => {
                    let future = self.interval - (now - st);
                    assert!(Duration::zero() < future);
                    NextPing::Future(future)
                },
                PingState::Good(_) => NextPing::Now
            }
        }
    }
}

enum Bot2Session {
    Connecting(BotConnector),
    Connected(BotSession),
}

impl Bot2Session {
    fn operate(&mut self) -> (&mut NonBlock<TcpStream>, &mut IrcMsgRingBuf, &mut IrcMsgRingBuf) {
        match *self {
            Bot2Session::Connecting(ref mut conn) => (
                &mut conn.connection, &mut conn.read_buffer, &mut conn.write_buffer),
            Bot2Session::Connected(ref mut conn) => (
                &mut conn.connection, &mut conn.read_buffer, &mut conn.write_buffer),
        }
    }

    pub fn write_flush(&mut self) -> Result<bool, ()> {
        use ::mio::TryWrite;

        let (conn, _read_buffer, write_buffer) = self.operate();
        if !write_buffer.is_empty() {
            match TryWrite::write(conn, write_buffer) {
                Ok(Some(0)) => (),
                r @ Ok(Some(_)) => warn!("irc_conn.write(...) -> {:?}", r),
                r @ Ok(None) => warn!("irc_conn.write(...) -> {:?}", r),
                err @ Err(_) => panic!("irc_conn.write(...) -> {:?}", err),
            };
        }

        Ok(!write_buffer.is_empty())
    }


    pub fn read_flush(&mut self) -> Result<(), ()> {
        use ::mio::TryRead;

        let (conn, read_buffer, _write_buffer) = self.operate();
        let mut clear_connection: bool = false;
        match TryRead::read(conn, read_buffer) {
            Ok(Some(0)) => clear_connection = true,
            Ok(Some(_)) => (),
            Ok(None) => return Ok(()),
            Err(err) => warn!("CLIENT: read error: {:?}", err),
        }
        match clear_connection {
            true => Err(()),
            false => Ok(()),
        }
    }

    fn upgrade(&mut self) {
        use self::Bot2Session::{Connecting, Connected};
        use std::mem::{forget, replace, uninitialized};

        let moved_self = replace(self, unsafe { uninitialized() });
        forget(replace(self, match moved_self {
            Connecting(connector) => {
                if connector.is_finished()  {
                    Connected(connector.into_session())
                } else {
                    Connecting(connector)
                }
            }
            Connected(sth) => Connected(sth),
        }));
        println!("upgraded");
    }

    pub fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use self::Bot2Session::{Connecting, Connected};
        match *self {
            Connecting(ref mut conn) => conn.dispatch_msg(eloop),
            Connected(ref mut conn) => conn.dispatch_msg(eloop),
        }
    }
    
    pub fn dispatch_read(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<(), ()> {
        use self::Bot2Session::{Connecting};

        try!(self.read_flush());

        while try!(self.dispatch_msg(eloop)) {}

        let should_upgrade = match *self {
            Connecting(ref bconn) => bconn.is_finished(),
            _ => false,
        };
        if should_upgrade {
            self.upgrade();
        }

        try!(self.write_flush());
        try!(self.read_flush());
        while try!(self.dispatch_msg(eloop)) {
            println!("dispatch_msg");
        }

        Ok(())
    }

    pub fn dispatch_write(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        self.write_flush()
    }

    pub fn dispatch_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use self::Bot2Session::{Connecting, Connected};
        match *self {
            Connecting(ref mut conn) => conn.dispatch_timeout(eloop),
            Connected(ref mut conn) => conn.dispatch_timeout(eloop),
        }
    }
}

struct BotConnector {
    plugins: PluginContainer,
    connection: NonBlock<TcpStream>,
    autojoin_on_connect: Vec<String>,
    autojoin_on_invite: HashSet<String>,
    nick: String,

    state_builder: StatePlugin,
    state: Option<State>,
    read_buffer: IrcMsgRingBuf,
    write_buffer: IrcMsgRingBuf,
}

impl BotConnector {
    fn configured(connection: TcpStream, conf: &BotConfig) -> BotConnector {
        let mut plugins = PluginContainer::new(conf.command_prefixes.clone());
        if conf.enabled_plugins.contains(PingPlugin::get_plugin_name()) {
            plugins.register(PingPlugin::new());
        }
        if conf.enabled_plugins.contains(GreedPlugin::get_plugin_name()) {
            plugins.register(GreedPlugin::new());
        }
        if conf.enabled_plugins.contains(SeenPlugin::get_plugin_name()) {
            plugins.register(SeenPlugin::new());
        }
        if conf.enabled_plugins.contains(DeerPlugin::get_plugin_name()) {
            plugins.register(DeerPlugin::new());
        }
        if conf.enabled_plugins.contains(RadioPlugin::get_plugin_name()) {
            plugins.register(RadioPlugin::new());
        }
        if conf.enabled_plugins.contains(WserverPlugin::get_plugin_name()) {
            plugins.register(WserverPlugin::new());
        }
        if conf.enabled_plugins.contains(WhoAmIPlugin::get_plugin_name()) {
            plugins.register(WhoAmIPlugin::new());
        }
        if conf.enabled_plugins.contains(LoggerPlugin::get_plugin_name()) {
            plugins.register(LoggerPlugin::new());
        }
        if conf.enabled_plugins.contains(FetwgrkifgPlugin::get_plugin_name()) {
            plugins.register(FetwgrkifgPlugin::new());
        }
        if conf.enabled_plugins.contains(AsciiArtPlugin::get_plugin_name()) {
            plugins.register(AsciiArtPlugin::new());
        }
        if conf.enabled_plugins.contains(AnimeCalendarPlugin::get_plugin_name()) {
            info!("attached {}", AnimeCalendarPlugin::get_plugin_name());
            plugins.register(AnimeCalendarPlugin::new());
        }
        
        let autojoin_on_invite: HashSet<String> = conf.channels.iter().cloned().collect();
        let autojoin_on_connect: Vec<String> = conf.channels.iter().cloned().collect();

        let mut wbuf = IrcMsgRingBuf::new(1 << 16);

        wbuf
            .push_msg(&client::User::new(&conf.username, "8", "*", &conf.realname).into_irc_msg())
            .ok().unwrap();
        wbuf
            .push_msg(&client::Nick::new(&conf.nickname).into_irc_msg())
            .ok().unwrap();

        BotConnector {
            plugins: plugins,
            connection: connection.into_non_block().unwrap(),
            autojoin_on_invite: autojoin_on_invite,
            autojoin_on_connect: autojoin_on_connect,

            nick: conf.nickname.clone(),
            state_builder: StatePlugin::new(),
            state: None,

            read_buffer: IrcMsgRingBuf::new(1 << 16),
            write_buffer: wbuf,
        }
    }

    fn is_finished(&self) -> bool {
        self.state.is_some()
    }

    fn into_session(mut self) -> BotSession {
        let state = self.state.take().expect("is_finished() must be true before calling into_session()");

        let mut bundler_man = BundlerManager::with_defaults();
            bundler_man.add_bundler_trigger(Box::new(
                JoinBundlerTrigger::new(state.get_self_nick().as_bytes())));

        for channel_name in self.autojoin_on_connect.iter() {
            let join_msg = client::Join::new(&channel_name).into_irc_msg();
            // FIXME: luck
            self.write_buffer.push_msg(&join_msg).ok().unwrap();
        }

        BotSession {
            plugins: self.plugins,
            connection: self.connection,
            // autojoin_on_connect: self.autojoin_on_connect,
            autojoin_on_invite: self.autojoin_on_invite,
            ping_man: ping::PingManager::new(),

            state: state,
            bundler_man: bundler_man,
            
            read_buffer: self.read_buffer,
            write_buffer: self.write_buffer,
        }
    }

    pub fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use ::irc_mio as ringbuf;

        let msg = match self.read_buffer.pop_msg() {
            Ok(msg) => msg,
            Err(ringbuf::PopError::MoreData) => return Ok(false),
            Err(ringbuf::PopError::ProtocolError(err)) => {
                warn!("protocol error: {:?}", err);
                eloop.shutdown();
                return Err(());
            }
            Err(ringbuf::PopError::Parse(err)) => {
                warn!("dropping invalid irc message!: {:?}", err);
                return Ok(false);
            }
        };
        if let server::IncomingMsg::Ping(ping) = server::IncomingMsg::from_msg(msg.clone()) {
            if let Ok(pong) = ping.get_response() {
                let pong_msg = pong.into_irc_msg();
                self.write_buffer.push_msg(&pong_msg).ok().unwrap();
            }
        }
        if let Some(state) = self.state_builder.on_irc_msg(&msg) {
            println!("state becomes some");
            self.state = Some(state);
            return Ok(false);
        }
        Ok(true)
    }

    pub fn dispatch_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        Ok(false)
    }
}

struct BotSession {
    plugins: PluginContainer,
    connection: NonBlock<TcpStream>,
    // autojoin_on_connect: Vec<String>,
    autojoin_on_invite: HashSet<String>,
    ping_man: ping::PingManager,

    state: State,
    bundler_man: BundlerManager,

    // connection impl details
    read_buffer: IrcMsgRingBuf,
    write_buffer: IrcMsgRingBuf,
}

impl BotSession {
    pub fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use ::irc_mio as ringbuf;

        let msg = match self.read_buffer.pop_msg() {
            Ok(msg) => msg,
            Err(ringbuf::PopError::MoreData) => {
                return Ok(false);
            },
            Err(ringbuf::PopError::ProtocolError(err)) => {
                warn!("protocol error: {:?}", err);
                eloop.shutdown();
                return Err(());
            }
            Err(ringbuf::PopError::Parse(err)) => {
                warn!("dropping invalid irc message!: {:?}", err);
                return Ok(false);
            }
        };

        if let server::IncomingMsg::Ping(ping) = server::IncomingMsg::from_msg(msg.clone()) {
            if let Ok(pong) = ping.get_response() {
                let pong_msg = pong.into_irc_msg();
                self.write_buffer.push_msg(&pong_msg).ok().unwrap();
            }
        }

        if let server::IncomingMsg::Pong(_) = server::IncomingMsg::from_msg(msg.clone()) {
            self.ping_man.pong_received();
        }

        if let server::IncomingMsg::Invite(invite) = server::IncomingMsg::from_msg(msg.clone()) {
            if self.autojoin_on_invite.contains(invite.get_target()) {
                let join_msg = client::Join::new(invite.get_target()).into_irc_msg();
                self.write_buffer.push_msg(&join_msg).ok().unwrap();
            }
        }
        
        if let Some(join) = self.state.is_self_join(&msg) {
            let who = client::Who::new(join.get_channel()).into_irc_msg();
            self.write_buffer.push_msg(&who).ok().unwrap();
        }
        for event in self.bundler_man.on_irc_msg(&msg).into_iter() {
            self.state.on_event(&event);
        }
        self.plugins.dispatch(Arc::new(self.state.clone_frozen()), &eloop.channel(), &msg);
        Ok(true)
    }

    pub fn dispatch_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        if self.ping_man.should_terminate() {
            let quit = client::Quit::new("Server not responding to PING").into_irc_msg();
            self.write_buffer.push_msg(&quit).ok().unwrap();
            return Ok(false);
        }

        if self.ping_man.next_ping().is_now() {
            let now = ::time::get_time();
            warn!("emitting ping: {:?}", now);
            let ping = client::Ping::new("swagever").into_irc_msg();
            self.write_buffer.push_msg(&ping).ok().unwrap();
            self.ping_man.ping_sent();
        }

        Ok(false)
    }
}


struct BotHandler {
    session: Bot2Session,
}

impl BotHandler {
    fn new(connector: BotConnector) -> BotHandler {
        BotHandler { session: Bot2Session::Connecting(connector) }
    }
}

impl ::mio::Handler for BotHandler {
    type Timeout = Token;
    type Message = IrcMsg;

    fn readable(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token, _: ReadHint) {
        match token {
            CLIENT => if let Err(err) = self.session.dispatch_read(eloop) {
                warn!("error in dispatch_read: {:?}", err);
                eloop.shutdown();
            },
            _ => panic!("unexpected token"),
        }
    }

    fn notify(&mut self, eloop: &mut EventLoop<BotHandler>, msg: IrcMsg) {
        {
            let (_, _, wbuf) = self.session.operate();
            wbuf.push_msg(&msg).ok().unwrap();
        }
        if let Err(err) = self.session.dispatch_write(eloop) {
            panic!("--!!-- error pushing: {:?}", err);
        }
    }

    fn timeout(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        match token {
            CLIENT => {
                if let Err(err) = self.session.dispatch_timeout(eloop) {
                    warn!("error in dispatch_timeout: {:?}", err);
                    eloop.shutdown();
                }
                if let Err(err) = self.session.dispatch_write(eloop) {
                    warn!("error in dispatch_write: {:?}", err);
                    eloop.shutdown();
                }
                if let Err(err) = self.session.dispatch_read(eloop) {
                    warn!("error in dispatch_read: {:?}", err);
                    eloop.shutdown();
                }
                if let Err(err) = self.session.dispatch_timeout(eloop) {
                    warn!("error in dispatch_timeout: {:?}", err);
                    eloop.shutdown();
                }
                if let Err(err) = self.session.dispatch_write(eloop) {
                    warn!("error in dispatch_write: {:?}", err);
                    eloop.shutdown();
                }
                if let Err(err) = self.session.dispatch_read(eloop) {
                    warn!("error in dispatch_read: {:?}", err);
                    eloop.shutdown();
                }
            },
            _ => panic!("unexpected token"),
        }
    }

    fn writable(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        match token {
            CLIENT => if let Err(err) = self.session.dispatch_write(eloop) {
                warn!("error in dispatch_write: {:?}", err);
                eloop.shutdown();
            },
            _ => panic!("unexpected token"),
        }
    }
}


pub fn run_loop(conf: &BotConfig) -> Result<(), ()> {
    let mut event_loop = EventLoop::configured(EventLoopConfig {
        io_poll_timeout_ms: 60000,
        timer_tick_ms: 10000,
        .. EventLoopConfig::default()
    }).unwrap();

    let addr: ::std::net::SocketAddr = 
        format!("{}:{}", conf.get_host(), conf.get_port()).parse().unwrap();
    let conn = TcpStream::connect(&addr).unwrap();
    let connector = BotConnector::configured(conn, conf);

    event_loop.register(&connector.connection, CLIENT).unwrap();
    event_loop.timeout_ms(CLIENT, 2500).unwrap();
    event_loop.run(&mut BotHandler::new(connector)).unwrap();

    Ok(())
}

pub struct StatePlugin {
    initial_nick: String,
    in_isupport: bool,
    isupport_finished: bool,
    emitted_state: bool,
}

impl StatePlugin {
    pub fn new() -> StatePlugin {
        StatePlugin {
            initial_nick: String::new(),
            in_isupport: false,
            isupport_finished: false,
            emitted_state: false,
        }
    }

    fn on_irc_msg(&mut self, msg: &IrcMsg) -> Option<State> {
        if self.emitted_state {
            return None;
        }

        let args = msg.get_args();

        if msg.get_command() == "001" {
            self.initial_nick = ::std::str::from_utf8(args[0]).unwrap().to_string();
        }
        if msg.get_command() == "005" {
            self.in_isupport = true;
        }

        // FIXME: ISUPPORT is required atm. We need an alternate way to determine
        // whether the IRC connection has ``completed''.
        if self.in_isupport && msg.get_command() != "005" {
            self.in_isupport = false;
            self.isupport_finished = true;
        }

        if self.initial_nick.len() > 0 && self.isupport_finished {
            let mut state = State::new();
            state.set_self_nick(&self.initial_nick);
            self.emitted_state = true;
            return Some(state);
        }

        None
    }
}

#[derive(Debug)]
pub enum MaybeString<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> MaybeString<'a> {
    pub fn new(buf: &'a [u8]) -> MaybeString<'a> {
        match ::std::str::from_utf8(buf) {
            Ok(s) => MaybeString::String(s),
            Err(_) => MaybeString::Bytes(buf),
        }
    }
}
