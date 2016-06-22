use std::fmt;
use std::convert::From;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::io::{self, Read};

use rustc_serialize::json::{self, DecoderError};
use hyper;
use time::{Timespec, get_time, Duration, SteadyTime};

use irc::{IrcMsg, IrcMsgBuf};

use utils::formatting::duration_to_string;
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

    pub fn end_time(&self) -> Timespec {
        Timespec::new(self.end_time, 0)
    }

    #[allow(unused)]
    pub fn start_offset(&self) -> Duration {
        Duration::seconds(self.start_offset)
    }
}

impl fmt::Display for Upcoming {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let air_time_relative = self.start_time() - get_time();

        write!(f, "\x02{}\x02 episode \x02{}\x02 airs in \x02{}\x02 on \x02{}\x02",
            self.title_name, self.count,
            duration_to_string(air_time_relative),
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
    let client = hyper::Client::new();
    let mut resp = try!(client.get(UPCOMING_URL).send());

    let mut body = String::new();
    try!(resp.read_to_string(&mut body));
    Ok(try!(json::decode::<Vec<Upcoming>>(&body)))
}

pub struct AnimeCalendarPlugin {
    sender: Option<SyncSender<(CommandMapperDispatch, IrcMsgBuf)>>
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

fn maybe_lower_contains(haystack: &str, needle: Option<&str>) -> Option<bool> {
    needle.map(|n| lower_contains(haystack, n))
}

// THere must be a better way to do this.
fn maybe_str_ref<'a>(opt: &'a Option<String>) -> Option<&'a str> {
    match opt.as_ref() {
        Some(val) => Some(val.as_ref()),
        None => None
    }
}

impl AniCalInternal {
    fn new() -> AniCalInternal {
        AniCalInternal {
            cache: Cache::new(),
        }
    }

    fn handle_upcoming(&mut self, m: &CommandMapperDispatch, search: Option<&str>) {
        match *self.cache.get_or_else(get_upcoming) {
            Ok(ref records) => {
                let now = get_time();
                let found_records = records.iter()
                    .filter(|r| maybe_lower_contains(&r.title_name, search).unwrap_or(true))
                    .filter(|r| r.end_time() > now)
                    .take(5);

                let mut found = false;
                for record in found_records {
                    found = true;
                    m.reply(&format!("{}", record));
                }
                if !found {
                    m.reply("No results found");
                }
            }
            Err(ref err) => m.reply(&format!("err: {:?}", err)),
        }
    }

    fn start(&mut self, rx: Receiver<(CommandMapperDispatch, IrcMsgBuf)>) {
        for (m, _) in rx.iter() {
            let command_phrase = m.command();
            match command_phrase.token {
                CMD_UPCOMING => {
                    let search = command_phrase.get::<String>(&"search");
                    self.handle_upcoming(&m, maybe_str_ref(&search))
                }
                _ => ()
            }
        }
    }
}

impl RustBotPlugin for AnimeCalendarPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_UPCOMING, Format::from_str("upcoming").unwrap());
        conf.map_format(CMD_UPCOMING, Format::from_str("upcoming {*search}").unwrap());
        conf.map_format(CMD_UPCOMING, Format::from_str("showtime").unwrap());
        conf.map_format(CMD_UPCOMING, Format::from_str("showtime {*search}").unwrap());
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
                if let Err(err) = sender.send((m.clone(), message.to_owned())) {
                    m.reply(&format!("Service ``animecalendar'' unavailable: {:?}", err));
                }
            }
            None => ()
        };
    }
}
