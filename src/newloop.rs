use mio::{Poll, Token, Event};
use mio::tcp::{TcpStream};
use mio::timer::Timer;
use std::borrow::ToOwned;

pub fn run_loop(conf: &BotConfig) -> Result<(), Box<std::error::Error>> {
    // FIXME(sell)
    let addr: ::std::net::SocketAddr =
        format!("{}:{}", conf.get_host(), conf.get_port()).parse().unwrap();

    let mut poll = try!(Poll::new());

    // Setup the client socket
    let mut server = try!(TcpStream::connect(&addr));

    let interest = Ready::writable() | Ready::error() | Ready::hup();
    try!(poll.register(&server, IRC_SERVER, interest, PollOpt::edge()));

    let mut events = Events::with_capacity(1024);
    loop {
        try!(poll.poll(&mut events, None));

        for event in events.iter() {
            match event.token() {
                IRC_SERVER => {
                    let next_interest = try!(irc_server_handle(
                        &mut server_state, &mut poll, &event, &mut server));

                    try!(poll.reregister(&server, IRC_SERVER, next_interest, PollOpt::edge()));

                    try!(server_state.dispatch());
                }
                _ => {
                    warn!("unknown token: {:?}", event.token());
                }
            }
        }
    }
}

fn irc_server_handle(
    ss: &mut ServerState,
    poll: &mut Poll,
    event: &Event,
    stream: &mut TcpStream
) -> Result<Ready, Box<std::error::Error>> {
    let mut out = Ready::readable() | Ready::error() | Ready::hup();
    if event.kind().is_error() {
        panic!("error!");
    }
    if event.kind().is_hup() {
        panic!("hup!");
    }

    let mut buf = [0; 4096];
    while event.kind().is_readable() {
        match stream.read(&mut buf[..]) {
            Ok(len) => ss.ibuf.extend(&buf[..len]),
            Err(ref err) if would_block(err) => break,
            Err(err) => return Err(err.into()),
        }
        try!(extract_messages(&mut ss.ibuf, &mut ss.ibuf_standby, &mut ss.imsg));
    }

    while event.kind().is_writable() && ss.obuf.len() > 0 {
        let offset = 0;
        match stream.write(&ss.obuf[offset..]) {
            Ok(len) => offset += len,
            Err(ref err) if would_block(err) => {
                ss.obuf_standby.extend(&ss.obuf[offset..]);
                mem::swap(&mut ss.obuf, &mut ss.obuf_standby);
                break;
            }
            Err(err) => return Err(err.into()),
        }
    }
    if ss.obuf.len() > 0 {
        out.insert(Ready::writable());
    }

    Ok(out)
}

fn extract_messages(
    ibuf: &mut Vec<u8>,
    extra: &mut Vec<u8>,
    out: &mut Vec<Vec<u8>>,
) -> Result<(), Box<std::error::Error>> {
    assert!(extra.len() == 0);

    let mut offset = None;
    for (off, &by) in ibuf.iter().enumerate() {
        if by == b'\n' {
            offset = Some(off + 1);
        }
    }

    if offset.is_none() {
        // Incomplete
        return Ok(());
    }

    let offset = offset.unwrap();
    extra.extend(&ibuf[offset..]);
    mem::swap(ibuf, extra);

    let mut offset = 0;
    for (off, &by) in extra.iter().enumerate() {
        if by == b'\n' {
            let msg: &[u8] = &extra[offset..off];
            offset = off + 1;
            out.push(try!(IrcMsg::new(msg)).to_owned());
        }
    }
    
    Ok(())
}

enum Session {
    Connecting(Connecting),
    Connected(Connected),
}

struct Connecting {
    enabled_plugins: HashSet<String>,
    autojoin_on_connect: Vec<String>,
    autojoin_on_invite: HashSet<String>,
    command_prefixes: Vec<String>,
    target_nick: String,

    state_builder: StatePlugin,
    state: Option<State>,
}

struct Connected {
    autojoin_on_invite: HashSet<String>,
    plugins: PluginContainer,
    ping_man: ping::PingManager,
    bundler_man: BundlerManager,
    state: State,
}

struct ServerState {
    ibuf: Vec<u8>,
    ibuf_standby: Vec<u8>,
    imsg: Vec<IrcMsgBuf>,
    obuf: Vec<u8>,
    obuf_standby: Vec<u8>,
    session: Session,
}

struct TimerIntent {
    //
}

struct Timers {
    timers: BTreeMap<SteadyTime, TimerIntent>,
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
