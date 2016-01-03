use std::io;
use std::collections::HashSet;
use std::sync::Arc;

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};
use mio::{EventLoop, EventLoopConfig, Token, EventSet, PollOpt};
use mio::tcp::TcpStream;

use irc::{BundlerManager, JoinBundlerTrigger};
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::State;

use irc_mio::IrcMsgRingBuf;
use irc_mio::PopError as IrcRingPopError;
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
    UnicodeNamePlugin,
    EightBallPlugin,
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
    fn connection(&mut self) -> &TcpStream {
        match *self {
            Bot2Session::Connecting(ref c) => &c.connection,
            Bot2Session::Connected(ref c) => &c.connection,
        }
    }

    fn operate(&mut self) -> (&mut TcpStream, &mut IrcMsgRingBuf, &mut IrcMsgRingBuf) {
        match *self {
            Bot2Session::Connecting(ref mut conn) => (
                &mut conn.connection, &mut conn.read_buffer, &mut conn.write_buffer),
            Bot2Session::Connected(ref mut conn) => (
                &mut conn.connection, &mut conn.read_buffer, &mut conn.write_buffer),
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
    }

    fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, IrcRingPopError> {
        use self::Bot2Session::{Connecting, Connected};
        match *self {
            Connecting(ref mut conn) => conn.dispatch_msg(),
            Connected(ref mut conn) => conn.dispatch_msg(eloop),
        }
    }

    fn dispatch_read(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<(), IrcRingPopError> {
        use self::Bot2Session::Connecting;

        while try!(self.dispatch_msg(eloop)) {}

        let should_upgrade = match *self {
            Connecting(ref bconn) => bconn.is_finished(),
            _ => false,
        };

        if should_upgrade {
            self.upgrade();
        }

        while try!(self.dispatch_msg(eloop)) {}

        Ok(())
    }

    fn dispatch_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use self::Bot2Session::{Connecting, Connected};
        match *self {
            Connecting(ref mut conn) => conn.dispatch_timeout(eloop),
            Connected(ref mut conn) => conn.dispatch_timeout(eloop),
        }
    }

    fn client_try_io(&mut self, eset: EventSet) -> io::Result<EventSet> {
        use ::mio::{TryRead, TryWrite};

        let (conn, read_buffer, write_buffer) = self.operate();
        let mut event_set = EventSet::none();

        loop {
            if !eset.is_readable() {
                break;
            }
            match try!(TryRead::try_read_buf(conn, read_buffer)) {
                Some(0) => {
                    info!("read 0 bytes: finished reading");
                    break;
                },
                Some(sz) => info!("read {} bytes", sz),
                None => {
                    info!("emptied kernel read buffer: subscribing");
                    event_set = event_set | EventSet::readable();
                    break;
                }
            }
        }

        loop {
            if !eset.is_writable() {
                break;
            }
            match try!(TryWrite::try_write_buf(conn, write_buffer)) {
                Some(0) => {
                    info!("wrote 0 bytes: finished writing");
                    break;
                },
                Some(sz) => info!("wrote {} bytes", sz),
                None => {
                    info!("filled kernel write buffer: subscribing");
                    event_set = event_set | EventSet::writable();
                    break;
                }
            }
        }

        Ok(event_set)
    }

    fn client_ready(&mut self, eloop: &mut EventLoop<BotHandler>, eset: EventSet) {
        if eset.is_error() {
            warn!("client_ready: eset with error: {:?}", eset);
            eloop.shutdown();
            return;
        }

        match self.client_try_io(EventSet::all()) {
            Ok(_) => (),
            Err(err) => {
                warn!("client_readable: error in client_try_io: {:?}", err);
                eloop.shutdown();
                return;
            }
        }

        if let Err(err) = self.dispatch_read(eloop) {
            warn!("ready/is_readable: error in dispatch_read");
            eloop.shutdown();
            return;
        }

        match self.client_try_io(EventSet::all()) {
            Ok(eset) => {
                eloop.reregister(self.connection(), CLIENT,
                    eset | EventSet::error(), PollOpt::empty()).unwrap();
            },
            Err(err) => {
                warn!("client_readable: error in client_try_io: {:?}", err);
                eloop.shutdown();
                return;
            }
        }
    }
}

struct BotConnector {
    plugins: PluginContainer,
    connection: TcpStream,
    autojoin_on_connect: Vec<String>,
    autojoin_on_invite: HashSet<String>,
    desired_nick: String,
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
        if conf.enabled_plugins.contains(UnicodeNamePlugin::get_plugin_name()) {
            plugins.register(UnicodeNamePlugin);
        }
        if conf.enabled_plugins.contains(AnimeCalendarPlugin::get_plugin_name()) {
            plugins.register(AnimeCalendarPlugin::new());
        }
        if conf.enabled_plugins.contains(EightBallPlugin::get_plugin_name()) {
            plugins.register(EightBallPlugin::new());
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
            connection: connection,
            autojoin_on_invite: autojoin_on_invite,
            autojoin_on_connect: autojoin_on_connect,

            desired_nick: conf.nickname.clone(),
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
            autojoin_on_invite: self.autojoin_on_invite,
            ping_man: ping::PingManager::new(),

            state: state,
            bundler_man: bundler_man,

            read_buffer: self.read_buffer,
            write_buffer: self.write_buffer,
        }
    }

    // returns Ok(true) when the next message should be immediately attempted.
    // returns Ok(false) when the next message should not be immediately attempted.
    fn dispatch_msg(&mut self) -> Result<bool, IrcRingPopError> {
        let msg = match self.read_buffer.pop_msg() {
            Ok(msg) => msg,
            Err(IrcRingPopError::MoreData) => return Ok(false),
            Err(err) => return Err(err),
        };

        if let server::IncomingMsg::Numeric(_, num) = server::IncomingMsg::from_msg(msg.clone()) {
            if num.get_code() == 433 {
                self.nick.push_str("`");
                if 24 <= self.nick.len() {
                    panic!("nick really long");
                }
                self.write_buffer
                    .push_msg(&client::Nick::new(&self.nick).into_irc_msg())
                    .ok().unwrap();
            }
        }

        if let server::IncomingMsg::Ping(ping) = server::IncomingMsg::from_msg(msg.clone()) {
            if let Ok(pong) = ping.get_response() {
                let pong_msg = pong.into_irc_msg();
                self.write_buffer.push_msg(&pong_msg).ok().unwrap();
            }
        }

        if let Some(state) = self.state_builder.on_irc_msg(&msg) {
            self.state = Some(state);
            return Ok(false);
        }
        Ok(true)
    }

    fn dispatch_timeout(&mut self, _eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        Ok(false)
    }
}

struct BotSession {
    plugins: PluginContainer,
    connection: TcpStream,
    autojoin_on_invite: HashSet<String>,
    ping_man: ping::PingManager,

    state: State,
    bundler_man: BundlerManager,

    // connection impl details
    read_buffer: IrcMsgRingBuf,
    write_buffer: IrcMsgRingBuf,
}

impl BotSession {
    fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, IrcRingPopError> {
        let msg = match self.read_buffer.pop_msg() {
            Ok(msg) => msg,
            Err(IrcRingPopError::MoreData) => return Ok(false),
            Err(err) => return Err(err),
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

    fn dispatch_timeout(&mut self, _eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
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
        BotHandler {
            session: Bot2Session::Connecting(connector),
        }
    }
}

impl ::mio::Handler for BotHandler {
    type Timeout = Token;
    type Message = IrcMsg;

    fn notify(&mut self, eloop: &mut EventLoop<BotHandler>, msg: IrcMsg) {
        {
            let (_, _, wbuf) = self.session.operate();
            wbuf.push_msg(&msg).ok().unwrap();
        }
        self.session.client_ready(eloop, EventSet::writable());
    }

    fn ready(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token, eset: EventSet) {
        if token == CLIENT {
            self.session.client_ready(eloop, eset);
        }
    }

    fn timeout(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        if token == CLIENT {
            if let Err(err) = self.session.dispatch_timeout(eloop) {
                warn!("Error during dispatch_timeout: {:?}", err);
                eloop.shutdown();
            }
        }
    }
}


pub fn run_loop(conf: &BotConfig) -> Result<(), ()> {
    let mut config = EventLoopConfig::default();
    let mut event_loop = EventLoop::configured(config).unwrap();

    let addr: ::std::net::SocketAddr =
        format!("{}:{}", conf.get_host(), conf.get_port()).parse().unwrap();
    let conn = TcpStream::connect(&addr).unwrap();
    let connector = BotConnector::configured(conn, conf);

    event_loop.register(&connector.connection, CLIENT,
        EventSet::readable() | EventSet::writable(), PollOpt::edge()).unwrap();
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
