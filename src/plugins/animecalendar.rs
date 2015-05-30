use std::fmt;
use std::convert::From;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::io::{self, Read};

use rustc_serialize::json::{self, DecoderError};
use url::Url;
use hyper;
use hyper::client::request::Request;
use hyper::method::Method::Get;
use time::{Timespec, get_time, Duration, SteadyTime};

use irc::parse::IrcMsg;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

static UPCOMING_URL: &'static str = "http://anime.yshi.org/api/calendar/upcoming/100";

const CMD_UPCOMING: Token = Token(0);

#[derive(RustcDecodable, RustcEncodable, Clone)]
struct Upcoming {
    title_name: String,
    syoboi_pid: i32,
    start_time: i64,
    end_time: i64,
    count: String,
    start_offset: i64,
    episode_name: String,
    channel_name: String,
    special_name: String,
    comment: String,
}

impl Upcoming {
    pub fn start_time(&self) -> Timespec {
        Timespec::new(self.start_time, 0)
    }

    pub fn start_offset(&self) -> Duration {
        Duration::seconds(self.start_offset)
    }
}

impl fmt::Display for Upcoming {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let now = get_time();
        write!(f, "{} episode {} airs in {} on {}",
            self.title_name, self.count,
            self.start_time() - now,
            self.channel_name)
    }
}

#[derive(Debug)]
enum ApiFailure {
    RequestError(hyper::Error),
    ResponseReadError(io::Error),
    ResponseDeserializeError(DecoderError)
}

impl From<hyper::Error> for ApiFailure {
    fn from(err: hyper::Error) -> ApiFailure {
        ApiFailure::RequestError(err)
    }
}

impl From<io::Error> for ApiFailure {
    fn from(err: io::Error) -> ApiFailure {
        ApiFailure::ResponseReadError(err)
    }
}

impl From<DecoderError> for ApiFailure {
    fn from(err: DecoderError) -> ApiFailure {
        ApiFailure::ResponseDeserializeError(err)
    }
}

fn get_upcoming() -> Result<Vec<Upcoming>, ApiFailure> {
    let url = Url::parse(UPCOMING_URL).ok().expect("Invalid UPCOMING_URL");
    let mut resp = try!(try!(try!(Request::new(Get, url)).start()).send());

    let mut body = String::new();
    try!(resp.read_to_string(&mut body));
    Ok(try!(json::decode::<Vec<Upcoming>>(&body)))
}

pub struct AnimeCalendarPlugin {
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMsg)>>
}


impl AnimeCalendarPlugin {
    pub fn new() -> AnimeCalendarPlugin {
        AnimeCalendarPlugin { sender: None }
    }

    pub fn get_plugin_name() -> &'static str {
        "animecalendar"
    }
}

struct CacheItem<T, E> {
    retrieval_time: SteadyTime,
    value: Result<T, E>
}

struct Cache<T, E> {
    negative_expiry: Duration,
    positive_expiry: Duration,
    item: Option<CacheItem<T, E>>,
}

impl<T, E> Cache<T, E> {
    fn new() -> Cache<T, E> {
        Cache {
            negative_expiry: Duration::seconds(30),
            positive_expiry: Duration::minutes(5),
            item: None,
        }
    }

    fn is_valid(&self) -> bool {
        self.get().is_some()
    }

    fn set<'a>(&'a mut self, result: Result<T, E>) -> &'a Result<T, E> {
        self.item = Some(CacheItem {
            retrieval_time: SteadyTime::now(),
            value: result,
        });
        &self.item.as_ref().unwrap().value
    }

    fn get<'a>(&'a self) -> Option<&'a Result<T, E>> {
        let now = SteadyTime::now();
        match self.item {
            Some(ref item) => match item.value.is_ok() {
                true if now < item.retrieval_time + self.positive_expiry => {
                    Some(&item.value)
                },
                false if now < item.retrieval_time + self.negative_expiry => {
                    Some(&item.value)
                },
                _ => None
            },
            None => None,
        }
    }

    fn get_or_else<'a, F>(&'a mut self, func: F) -> &'a Result<T, E> where F: FnOnce() -> Result<T, E> {
        if self.is_valid() {
            self.get().unwrap()
        } else {
            let value = func();
            self.set(value)
        }
    }
}

struct AniCalInternal {
    cache: Cache<Vec<Upcoming>, ApiFailure>,
}

// This could be made allocation-free 
fn lower_contains(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.to_lowercase();
    let needle = needle.to_lowercase();

    haystack.contains(&needle)
}

impl AniCalInternal {
    fn new() -> AniCalInternal {
        AniCalInternal {
            cache: Cache::new(),
        }
    }

    fn handle_upcoming(&mut self, m: &CommandMapperDispatch, search: &str) {
        match *self.cache.get_or_else(get_upcoming) {
            Ok(ref records) => {
                let now = get_time();
                let found_records = records.iter()
                    .filter(|r| lower_contains(&r.title_name, search))
                    .filter(|r| r.start_time() > now)
                    .take(5);
                for record in found_records {
                    m.reply(&format!("{}", record));
                }
                
            }
            Err(ref err) => m.reply(&format!("err: {:?}", err)),
        }
    }

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMsg)>) {
        for (m, _) in rx.iter() {
            let command_phrase = m.command();
            match command_phrase.token {
                CMD_UPCOMING => match command_phrase.get::<String>(&"search") {
                    Some(ref search) => self.handle_upcoming(&m, search),
                    None => (),
                },
                _ => () 
            }
        }
    }
}

impl RustBotPlugin for AnimeCalendarPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_UPCOMING, Format::from_str("upcoming {*search}").unwrap());
    }

    fn start(&mut self) {
        info!("started AnimeCalendarPlugin");
        let (tx, rx) = sync_channel(10);
        self.sender = Some(tx);

        let _ = ::std::thread::Builder::new().name("plugin-animecalendar".to_string()).spawn(move || {
            info!("AniCalInternal started.");
            AniCalInternal::new().start(rx);
        });
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMsg) {
        info!("dispatching AnimeCalendarPlugin command");
        match self.sender {
            Some(ref sender) => {
                if let Err(err) = sender.send((m.clone(), message.clone())) {
                    m.reply(&format!("Service ``animecalendar'' unavailable: {:?}", err));
                }
            }
            None => ()
        };
    }
}


