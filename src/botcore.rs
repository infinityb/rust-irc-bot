use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender};
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::net::TcpStream;
use time::{Duration, SteadyTime};

use url::{
    Url, SchemeType, Host,
    ParseResult, UrlParser
};
use mio::{NonBlock, ReadHint, EventLoop, EventLoopConfig, Token, tcp};
use mio::buf::RingBuf;

use irc::recv::{self, IrcMsgBuffer};
use irc::stream::{IrcReader, IrcWriter, IrcWrite, IrcReaderIter, IrcConnector, RegisterReqBuilder};
use irc::{BundlerManager, JoinBundlerTrigger};
use irc::parse::IrcMsg;
use irc::message_types::{client, server};
use irc::State;


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

const TIMEOUT: Token = Token(0);
const CLIENT: Token = Token(1);

#[derive(PartialEq, Eq)]
enum RegistrationStatus {
    Initial,
    Pending,
    Finished,
}

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
                PingState::Good(st) => {
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

// struct BotSession {
//     plugins: PluginContainer,
//     connection: NonBlock<TcpStream>,
//     autojoin_on_invite: HashSet<String>,
//     ping_man: ping::PingManager,

//     // connection impl details
//     read_buffer: IrcMsgRingBuf,
//     write_buffer: IrcMsgRingBuf,
// }

pub fn write_irc_msg(rb: &mut RingBuf, msg: &IrcMsg) -> io::Result<()> {
    try!(rb.write_all(msg.as_bytes()));
    try!(rb.write_all(b"\n"));
    Ok(())
}

struct BotHandler {
    connection: Option<NonBlock<TcpStream>>,
    plugins: PluginContainer,
    autojoin_on_invite: HashSet<String>, // TODO: move to config?
    autojoin: Vec<String>,
    ping_status: ping::PingManager,

    reg: RegistrationStatus,
    state_builder: StatePlugin,
    state: Option<State>,
    bundler_man: Option<BundlerManager>,

    rx_buf: RingBuf,
    rx_msg: IrcMsgBuffer,
    tx_buf: RingBuf,
}

impl BotHandler {
    fn new(plugins: PluginContainer, autojoin_on_invite: HashSet<String>, autojoin: Vec<String>, conn: NonBlock<TcpStream>) -> BotHandler {
        BotHandler {
            connection: Some(conn),
            plugins: plugins,
            autojoin_on_invite: autojoin_on_invite,
            autojoin: autojoin,
            ping_status: ping::PingManager::new(),

            reg: RegistrationStatus::Initial,
            state_builder: StatePlugin::new(),
            state: None,
            bundler_man: None,

            rx_buf: RingBuf::new(1 << 16),
            rx_msg: IrcMsgBuffer::new(1 << 16),
            tx_buf: RingBuf::new(1 << 16),
        }
    }

    fn client_read_dispatch(&mut self, eloop: &mut EventLoop<BotHandler>) -> bool {
        let msg = match self.rx_msg.recv() {
            Ok(msg) => msg,
            Err(recv::RecvError::MoreData) => {
                return false;
            },
            Err(recv::RecvError::Parse(err)) => {
                warn!("dropping invalid irc message!: {:?}", err);
                return false;
            }
        };

        if let Some(state) = self.state_builder.on_irc_msg(&msg) {
            let mut bundler_man = BundlerManager::with_defaults();
            bundler_man.add_bundler_trigger(Box::new(
                JoinBundlerTrigger::new(state.get_self_nick().as_bytes())));
            self.bundler_man = Some(bundler_man);
            self.state = Some(state);

            for channel_name in self.autojoin.iter() {
                let join_msg = client::Join::new(&channel_name).into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &join_msg).unwrap();
            }
        }

        if let server::IncomingMsg::Ping(ping) = server::IncomingMsg::from_msg(msg.clone()) {
            if let Ok(pong) = ping.get_response() {
                let pong_msg = pong.into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &pong_msg).unwrap();
            }
        }

        if let server::IncomingMsg::Pong(pong) = server::IncomingMsg::from_msg(msg.clone()) {
            self.ping_status.pong_received();
        }

        if let server::IncomingMsg::Invite(invite) = server::IncomingMsg::from_msg(msg.clone()) {
            if self.autojoin_on_invite.contains(invite.get_target()) {
                let join_msg = client::Join::new(invite.get_target()).into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &join_msg).unwrap();
            }
        }
        
        if let Some(ref mut state) = self.state {
            if let Some(join) = state.is_self_join(&msg) {
                let who = client::Who::new(join.get_channel()).into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &who).unwrap();
            }
            for event in self.bundler_man.as_mut().unwrap().on_irc_msg(&msg).into_iter() {
                state.on_event(&event);
            }

            let message_channel = eloop.channel();
            self.plugins.dispatch(Arc::new(state.clone_frozen()), &message_channel, &msg);
        }

        true
    }

    fn client_write_helper(&mut self, _: &mut EventLoop<BotHandler>) {
        use ::mio::TryWrite;

        if let Some(ref mut connection) = self.connection {
            if !self.tx_buf.is_empty() {
                match TryWrite::write(connection, &mut self.tx_buf) {
                    Ok(Some(0)) => (),
                    r @ Ok(Some(_)) => println!("irc_conn.write(...) -> {:?}", r),
                    r @ Ok(None) => println!("irc_conn.write(...) -> {:?}", r),
                    err @ Err(_) => panic!("irc_conn.write(...) -> {:?}", err),
                };
            }
        }
    }

    fn client_read(&mut self, eloop: &mut EventLoop<BotHandler>) {
        use ::mio::TryRead;
        use std::io::{Write, stdout};
        use std::sync::mpsc::TryRecvError;

        let mut clear_connection = false;
        if let Some(ref mut connection) = self.connection {
            match TryRead::read(connection, &mut self.rx_buf) {
                Ok(Some(0)) => {
                    clear_connection = true;
                    eloop.deregister(connection).unwrap();
                }
                Ok(Some(n)) => {
                    let mut data = Vec::new();
                    io::copy(&mut self.rx_buf, &mut data).unwrap();
                    println!("CLIENT: read {:?} bytes from socket: {:?}", n, MaybeString::new(&data));
                    self.rx_msg.push(&data[..]).ok().unwrap();
                }
                Ok(None) => println!("CLIENT: read none?"),
                Err(err) => println!("CLIENT: read error: {:?}", err),

            }
        }
        if clear_connection {
            self.connection = None;
        }

        while self.client_read_dispatch(eloop) {}
    }

    fn client_timeout(&mut self, eloop: &mut EventLoop<BotHandler>) {
        use ::mio::TryWrite;
        
        eloop.timeout_ms(CLIENT, 2500).unwrap();

        if let Some(ref mut connection) = self.connection {
            if self.ping_status.should_terminate() {
                let quit = client::Quit::new("Server not responding to PING").into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &quit).unwrap();
                eloop.shutdown();
                return;
            }

            if self.ping_status.next_ping().is_now() {
                let ping = client::Ping::new("swagever").into_irc_msg();
                write_irc_msg(&mut self.tx_buf, &ping).unwrap();
                self.ping_status.ping_sent();
            }
        } else {
            eloop.shutdown();
        }

        self.client_write_helper(eloop);
    }

    fn client_write(&mut self, eloop: &mut EventLoop<BotHandler>) {
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

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: IrcMsg) {
        write_irc_msg(&mut self.tx_buf, &msg).unwrap();
        self.client_write_helper(event_loop);
    }

    fn timeout(&mut self, eloop: &mut EventLoop<BotHandler>, token: Token) {
        use ::mio::TryWrite;
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
    use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};

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

    let mut incoming = IrcMsgBuffer::new(1 << 16);

    let mut eloop_config = EventLoopConfig {
        io_poll_timeout_ms: 60000,
        timer_tick_ms: 10000,
        .. EventLoopConfig::default()
    };

    let mut event_loop = EventLoop::configured(eloop_config).unwrap();

    let autojoin_invited: HashSet<String> = conf.channels.iter().cloned().collect();
    let autojoin: Vec<String> = conf.channels.iter().cloned().collect();
    let mut sock = ::std::net::TcpStream::connect(&(&conf.get_host() as &str, conf.get_port())).unwrap();
    info!("  -> {:?}", sock);

    println!("{:?}", sock.write_all(
        client::User::new("rustirc", "8", "*", "https://github.com/infinityb/rust-irc-bot")
        .into_irc_msg().as_bytes()).unwrap());
    println!("{:?}", sock.write_all(b"\n").unwrap());
    println!("{:?}", sock.write_all(
        client::Nick::new(&conf.nickname)
        .into_irc_msg().as_bytes()).unwrap());
    println!("{:?}", sock.write_all(b"\n").unwrap());

    event_loop.register(&sock, CLIENT).unwrap();
    event_loop.timeout_ms(CLIENT, 2500).unwrap();
    event_loop.run(&mut BotHandler::new(plugins, autojoin_invited, autojoin, NonBlock::new(sock))).unwrap();

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
enum MaybeString<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> MaybeString<'a> {
    fn new(buf: &'a [u8]) -> MaybeString<'a> {
        match ::std::str::from_utf8(buf) {
            Ok(s) => MaybeString::String(s),
            Err(_) => MaybeString::Bytes(buf),
        }
    }
}