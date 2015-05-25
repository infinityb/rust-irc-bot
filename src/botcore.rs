use std::collections::HashSet;
use std::sync::Arc;
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::mpsc::{channel, Sender, Receiver};

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};
use mio::{NonBlock, ReadHint, EventLoop, EventLoopConfig, Token, IntoNonBlock};
use mio::buf::RingBuf;

use irc::recv::{self, IrcMsgBuffer};
use irc::{BundlerManager, JoinBundlerTrigger};
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::State;

use ringbuf::{IrcMsgReceiver, IrcMsgSender};
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
                PingState::Pending(st) => {
                    info!("got PONG with {} lag", now - st);
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

struct BotSession {
    plugins: PluginContainer,
    connection: NonBlock<TcpStream>,
    autojoin_on_connect: Vec<String>,
    autojoin_on_invite: HashSet<String>,
    ping_man: ping::PingManager,

    // Post-registration things
    state_builder: StatePlugin,
    state: Option<State>,
    bundler_man: Option<BundlerManager>,

    // connection impl details
    read_buffer: IrcMsgReceiver,
    write_buffer: IrcMsgSender,
}

impl BotSession {
    pub fn configured(connection: NonBlock<TcpStream>, conf: &BotConfig) -> BotSession {
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
        
        let autojoin_on_invite: HashSet<String> = conf.channels.iter().cloned().collect();
        let autojoin_on_connect: Vec<String> = conf.channels.iter().cloned().collect();

        BotSession {
            plugins: plugins,
            connection: connection,
            autojoin_on_invite: autojoin_on_invite,
            autojoin_on_connect: autojoin_on_connect,
            ping_man: ping::PingManager::new(),

            state_builder: StatePlugin::new(),
            state: None,
            bundler_man: None,

            read_buffer: IrcMsgReceiver::new(1 << 16),
            write_buffer: IrcMsgSender::new(1 << 16),
        }
    }

    pub fn dispatch_msg(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use ::ringbuf;

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

        if let Some(state) = self.state_builder.on_irc_msg(&msg) {
            let mut bundler_man = BundlerManager::with_defaults();
            bundler_man.add_bundler_trigger(Box::new(
                JoinBundlerTrigger::new(state.get_self_nick().as_bytes())));
            self.bundler_man = Some(bundler_man);
            self.state = Some(state);

            for channel_name in self.autojoin_on_connect.iter() {
                let join_msg = client::Join::new(&channel_name).into_irc_msg();
                self.write_buffer.push_msg(&join_msg).ok().unwrap();
            }
        }

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
        
        if let Some(ref mut state) = self.state {
            if let Some(join) = state.is_self_join(&msg) {
                let who = client::Who::new(join.get_channel()).into_irc_msg();
                self.write_buffer.push_msg(&who).ok().unwrap();
            }
            for event in self.bundler_man.as_mut().unwrap().on_irc_msg(&msg).into_iter() {
                state.on_event(&event);
            }
            self.plugins.dispatch(Arc::new(state.clone_frozen()), &eloop.channel(), &msg);
        }

        self.dispatch_write(eloop).ok().unwrap();
        Ok(true)
    }

    pub fn dispatch_read(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use ::mio::TryRead;

        let mut clear_connection = false;

        match TryRead::read(&mut self.connection, &mut self.read_buffer) {
            Ok(Some(0)) => clear_connection = true,
            Ok(Some(_)) => (),
            Ok(None) => return Ok(false),
            Err(err) => warn!("CLIENT: read error: {:?}", err),
        }
        if clear_connection {
            warn!("connection invalid!");
            return Err(());
        }

        while let Ok(should_continue) = self.dispatch_msg(eloop) {
            if !should_continue {
                break;
            }
        }

        self.dispatch_write(eloop).ok().unwrap();
        Ok(true)
    }

    pub fn dispatch_write(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        use std::sync::mpsc::TryRecvError;
        use ::mio::TryWrite;

        if !self.write_buffer.is_empty() {
            match TryWrite::write(&mut self.connection, &mut self.write_buffer) {
                Ok(Some(0)) => (),
                r @ Ok(Some(_)) => warn!("irc_conn.write(...) -> {:?}", r),
                r @ Ok(None) => warn!("irc_conn.write(...) -> {:?}", r),
                err @ Err(_) => panic!("irc_conn.write(...) -> {:?}", err),
            };
        }
        Ok(!self.write_buffer.is_empty())
    }

    pub fn dispatch_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) -> Result<bool, ()> {
        if self.ping_man.should_terminate() {
            let quit = client::Quit::new("Server not responding to PING").into_irc_msg();
            self.write_buffer.push_msg(&quit).ok().unwrap();
            return Ok(false);
        }

        if self.ping_man.next_ping().is_now() {
            let ping = client::Ping::new("swagever").into_irc_msg();
            self.write_buffer.push_msg(&ping).ok().unwrap();
            self.ping_man.ping_sent();
        }

        if let Err(err) = self.dispatch_write(eloop) {
            warn!("error in dispatch_write: {:?}", err);
            return Err(());
        }
        Ok(false)
    }
}


struct BotHandler {
    session: BotSession,
}

impl BotHandler {
    fn new(session: BotSession) -> BotHandler {
        BotHandler { session: session }
    }

    fn client_read(&mut self, eloop: &mut EventLoop<BotHandler>) {
        loop {
            match self.session.dispatch_read(eloop) {
                Ok(true) => (),
                Ok(false) => break,
                Err(err) => {
                    warn!("error in dispatch_read: {:?}", err);
                    eloop.shutdown();
                    return;
                }
            }
        }
    }

    fn client_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) {       
        eloop.timeout_ms(CLIENT, 2500).unwrap();
        loop {
            match self.session.dispatch_timeout(eloop) {
                Ok(true) => (),
                Ok(false) => break,
                Err(err) => {
                    warn!("error in dispatch_timeout: {:?}", err);
                    eloop.shutdown();
                    return;
                }
            }
        }
    }

    fn client_write(&mut self, eloop: &mut EventLoop<BotHandler>) {
        loop {
            match self.session.dispatch_write(eloop) {
                Ok(true) => (),
                Ok(false) => break,
                Err(err) => {
                    warn!("error in dispatch_write: {:?}", err);
                    eloop.shutdown();
                    return;
                }
            }
        }
    }
}

impl ::mio::Handler for BotHandler {
    type Timeout = Token;
    type Message = IrcMsg;

    fn readable(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token, _: ReadHint) {
        match token {
            CLIENT => self.client_read(eloop),
            _ => panic!("unexpected token"),
        }
    }

    fn notify(&mut self, eloop: &mut EventLoop<BotHandler>, msg: IrcMsg) {
        self.session.write_buffer.push_msg(&msg).ok().unwrap();
        if let Err(err) = self.session.dispatch_write(eloop) {
            panic!("--!!-- error pushing: {:?}", err);
        }
    }

    fn timeout(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        match token {
            CLIENT => self.client_timeout(eloop),
            _ => panic!("unexpected token"),
        }
    }

    fn writable(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        match token {
            CLIENT => self.client_write(eloop),
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

    let mut sock = ::std::net::TcpStream::connect(&(&conf.get_host() as &str, conf.get_port())).unwrap();
    info!("  -> {:?}", sock);

    sock.write_all(
        client::User::new("rustirc", "8", "*", "https://github.com/infinityb/rust-irc-bot")
        .into_irc_msg().as_bytes()).unwrap();
    sock.write_all(b"\r\n").unwrap();

    sock.write_all(
        client::Nick::new(&conf.nickname)
        .into_irc_msg().as_bytes()).unwrap();
    sock.write_all(b"\r\n").unwrap();

    event_loop.register(&sock, CLIENT).unwrap();
    event_loop.timeout_ms(CLIENT, 2500).unwrap();

    let sock = sock.into_non_block().ok().unwrap();

    let session = BotSession::configured(sock, conf);
    event_loop.run(&mut BotHandler::new(session)).unwrap();

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
            self.initial_nick = String::from_str(::std::str::from_utf8(args[0]).unwrap());
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