use std::fmt;

use std::collections::hashmap::{Occupied, Vacant};
use std::collections::{
    HashMap,
    HashSet
};

use irc::{
    IrcMessage,
    JoinResult,
    WhoResult,
    WhoRecord,

    IrcEvent,
    IrcEventMessage,
    IrcEventJoinBundle,
    IrcEventWhoBundle,
};

pub enum XXBotUserId {
    Known(BotUserId),
    Anonymous
}

#[deriving(Clone, Show)]
pub enum MessageEndpoint {
    KnownUser(BotUserId),
    KnownChannel(BotChannelId),
    Server(String),
    AnonymousUser,
}

#[deriving(Clone, Show, Hash, PartialEq, Eq)]
pub struct BotUserId(u64);


struct InternalUser {
    id: BotUserId,
    prefix: String,
    channels: HashSet<BotChannelId>
}

impl fmt::Show for InternalUser {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InternalUser(prefix: {})", self.prefix)
    }
}


#[deriving(Clone, Show, Hash, PartialEq, Eq)]
pub struct BotChannelId(u64);


struct InternalChannel {
    id: BotChannelId,
    name: String,
    topic: String,
    users: HashSet<BotUserId>
}

impl fmt::Show for InternalChannel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InternalChannel(name: {})", self.name)
    }
}


#[deriving(Show)]
pub struct State {
    botnick: String,

    user_seq: u64,
    user_map: HashMap<String, BotUserId>,
    users: HashMap<BotUserId, InternalUser>,

    channel_seq: u64,
    channel_map: HashMap<String, BotChannelId>,
    channels: HashMap<BotChannelId, InternalChannel>,
}

impl State {
    pub fn new() -> State {
        State {
            botnick: String::new(),

            user_seq: 0,
            user_map: HashMap::new(),
            users: HashMap::new(),

            channel_seq: 0,
            channel_map: HashMap::new(),
            channels: HashMap::new(),
        }
    }

    pub fn get_bot_nick<'a>(&'a self) -> &'a str {
        self.botnick.as_slice()
    }

    fn on_self_part(&mut self, msg: &IrcMessage) {
        let channel_name = msg.get_args()[0].to_string();
        info!("on-self-part ({}); popping channel {}", self.botnick, channel_name);
        // fn pop(&mut self, k: &K) -> Option<V>
        let maybe_chan_state = match self.channel_map.pop(&channel_name) {
            Some(chan_id) => self.channels.pop(&chan_id),
            None => None
        };
        let state = match maybe_chan_state {
            Some(state) => state,
            None => {
                warn!("We parted {} without knowing about it?", channel_name);
                return
            }
        };
        for user_id in state.users.iter() {
            match self.users.find_mut(user_id) {
                Some(user) => {
                    user.channels.remove(&state.id);
                },
                None => {
                    warn!("Inconsistent state {} on {}", user_id, channel_name);
                }
            }
        }
    }

    fn on_other_part(&mut self, msg: &IrcMessage) {
        info!("on-other-part for {}", msg);
        let channel_name = msg.get_args()[0].to_string();
        let user_nick = match msg.source_nick() {
            Some(user_nick) => user_nick.to_string(),
            None => return
        };
        info!("on-other-part ({}); popping channel {} {}", self.botnick, user_nick, channel_name);
        
        let user_id = match self.user_map.find(&user_nick) {
            Some(user_id) => user_id.clone(),
            None => {
                warn!("Saw message ({}) for unknown user {}", msg, user_nick);
                return;
            }
        };
        let channel_id = match self.channel_map.find(&channel_name) {
            Some(channel_id) => channel_id.clone(),
            None => {
                warn!("Saw message ({}) for unknown channel {}", msg, channel_name);
                return;
            }
        };
        let remove_user = match self.users.find_mut(&user_id) {
            Some(user_state) => {
                user_state.channels.remove(&channel_id);
                info!("users[{}].channels.len() = {}", user_id, user_state.channels.len());
                user_state.channels.len() == 0
            },
            None => {
                warn!("Inconsistent state: {} lookup failure", channel_id);
                return;
            }
        };
        if remove_user {
            info!("removing user {}", user_id);
            self.users.remove(&user_id);
            self.user_map.remove(&user_nick);
        }
        match self.channels.find_mut(&channel_id) {
            Some(channel_state) => {
                channel_state.users.remove(&user_id);
            },
            None => {
                warn!("Inconsistent state: {} lookup failure", channel_id);
                return;
            }
        }
    }

    fn on_topic(&mut self, msg: &IrcMessage) {
        if msg.command() != "TOPIC" {
            return;
        }
        let chan_id = match self.identify_channel(msg.get_args()[0]) {
            Some(chan_id) => chan_id,
            None => return
        };
        let channel = match self.channels.find_mut(&chan_id) {
            Some(channel) => channel,
            None => return
        };
        channel.topic.clear();
        channel.topic.push_str(msg.get_args()[1]);
    }

    fn on_nick(&mut self, from_nick: &str, msg: &IrcMessage) {
        let to_nick = msg.get_args()[0];
        let from_prefix = match msg.get_prefix_raw() {
            Some(prefix) => prefix,
            None => {
                warn!("bad message: no prefix");
                return;
            }
        };
        let user_id = match self.user_map.pop(&from_nick.to_string()) {
            Some(user_id) => user_id,
            None => return
        };
        self.user_map.insert(to_nick.to_string(), user_id);
        match self.users.find_mut(&user_id) {
            Some(user_rec) => {
                user_rec.prefix.clear();
                user_rec.prefix.push_str(from_prefix);
            },
            None => {
                warn!("inconsistent state");
                return;
            }
        };
        
    }

    fn on_message(&mut self, msg: &IrcMessage) {
        if msg.command() == "001" {
            self.botnick.clear();
            self.botnick.push_str(msg.get_args()[0]);
        }
        // :rustbot!rustbot@out-ab-133.wireless.telus.com PART #sample
        if msg.command() == "PART" {
            if msg.source_nick() == Some(self.botnick.as_slice()) {
                return self.on_self_part(msg);
            } else {
                return self.on_other_part(msg);
            }
        }
        if msg.command() == "JOIN" && msg.source_nick().is_some() {
            self.on_other_join(msg)
        }
        if msg.command() == "TOPIC" {
            self.on_topic(msg);
        }
        if let ("NICK", Some(source_nick)) = (msg.command(), msg.source_nick()) {
            self.on_nick(source_nick, msg);
        }
    }

    fn __find_channel_id(&mut self, name: &str) -> BotChannelId {
        let cur_seq = BotChannelId(self.channel_seq);
        let (should_incr, chan_id) = match self.channel_map.entry(name.to_string()) {
            Occupied(entry) => (false, entry.get().clone()),
            Vacant(entry) => (true, entry.set(cur_seq).clone()),
        };
        if should_incr {
            self.channel_seq += 1;
        }
        chan_id
    }

    pub fn identify_channel(&self, chan: &str) -> Option<BotChannelId> {
        match self.channel_map.find(&chan.to_string()) {
            Some(chan_id) => Some(chan_id.clone()),
            None => None
        }
    }

    fn find_channel_by_name(&self, chan: &str) -> Option<&InternalChannel> {
        let chan_id = match self.identify_channel(chan) {
            Some(chan_id) => chan_id,
            None => return None
        };
        self.channels.find(&chan_id)
    }

    fn __find_user_id(&mut self, nick: &str) -> BotUserId {
        let cur_seq = BotUserId(self.user_seq);
        let (should_incr, user_id) = match self.user_map.entry(nick.to_string()) {
            Occupied(entry) => (false, *entry.get()),
            Vacant(entry) => (true, *entry.set(cur_seq)),
        };
        if should_incr {
            self.user_seq += 1;
        }
        user_id
    }

    pub fn identify_nick(&self, nick: &str) -> Option<BotUserId> {
        match self.user_map.find(&nick.to_string()) {
            Some(user_id) => Some(*user_id),
            None => None
        }
    }

    fn find_user_by_nick(&self, nick: &str) -> Option<&InternalUser> {
        let user_id = match self.identify_nick(nick) {
            Some(user_id) => user_id,
            None => return None
        };
        self.users.find(&user_id)
    }


    fn on_self_join(&mut self, join_res: &JoinResult) {
        let join = match *join_res {
            Ok(ref ok) => ok,
            Err(_) => return
        };
        if let Some(_) = self.channel_map.find(&join.channel) {
            warn!("We know about {} before joining it?", join.channel);
        }
        let channel_id = self.__find_channel_id(join.channel.as_slice());
        let topic = match join.topic {
            Some(ref topic) => topic.text.clone(),
            None => String::new()
        };

        self.channels.insert(channel_id, InternalChannel {
            id: channel_id,
            name: join.channel.to_string(),
            topic: topic,
            users: HashSet::new(),
        });
    }

    fn on_other_join(&mut self, join: &IrcMessage) {
        let channel = join.get_args()[0];
        let prefix = match join.get_prefix_raw() {
            Some(prefix) => prefix.to_string(),
            None => return
        };
        if let Some(ref nick) = join.source_nick() {
            let user_id = self.__find_user_id(nick.as_slice());
            let chan_id = self.__find_channel_id(channel);

            match self.users.entry(user_id) {
                Occupied(mut entry) => {
                    entry.get_mut().channels.insert(chan_id);
                },
                Vacant(entry) => {
                    let mut channels = HashSet::new();
                    channels.insert(chan_id);
                    entry.set(InternalUser {
                        id: user_id,
                        prefix: prefix,
                        channels: channels
                    });
                }
            };
        }       
    }

    fn on_who_record(&mut self, chan_id: BotChannelId, rec: &WhoRecord) -> BotUserId {
        let user_id = self.__find_user_id(rec.nick.as_slice());
        match self.users.entry(user_id) {
            Occupied(mut entry) => {
                entry.get_mut().channels.insert(chan_id);
            },
            Vacant(entry) => {
                let mut channels = HashSet::new();
                channels.insert(chan_id);
                entry.set(InternalUser {
                    id: user_id,
                    prefix: format!("XX{}", rec.get_prefix()),
                    channels: channels
                });
            }
        };
        user_id
    }

    fn on_who(&mut self, who_res: &WhoResult) {
        let who = match *who_res {
            Ok(ref ok) => ok,
            Err(_) => return
        };
        let channel_id = self.__find_channel_id(who.channel.as_slice());
        let mut user_ids = Vec::new();
        for rec in who.who_records.iter() {
            user_ids.push(self.on_who_record(channel_id, rec));
        }
        match self.channels.find_mut(&channel_id) {
            Some(ref mut channel) => {
                channel.users.extend(user_ids.into_iter());
            },
            None => {
                warn!("Inconsistent state: {} lookup failure", channel_id);
            }
        }
    }

    pub fn on_event(&mut self, event: &IrcEvent) {
        match *event {
            IrcEventMessage(ref message) => self.on_message(message),
            IrcEventJoinBundle(ref join_bun) => self.on_self_join(join_bun),
            IrcEventWhoBundle(ref who_bun) => self.on_who(who_bun),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::{State, BotUserId};
    use std::io::{IoResult, BufReader};
    use irc::{
        BundlerManager,
        IrcMessage,
        JoinBundlerTrigger,
        WhoBundlerTrigger,
    };

    const TEST_SESSION_STATETRACKER: &'static [u8] =
        include_bin!("../testdata/statetracker.txt");

    #[deriving(Show)]
    enum SessionRecord {
        ContentLine(IrcMessage),
        CommentLine(String),
    }


    struct SessionReplayer<'a> {
        reader: BufReader<'a>
    }

    impl<'a> SessionReplayer<'a> {
        fn new<'a>(buf: &'a [u8]) -> SessionReplayer<'a> {
            SessionReplayer {
                reader: BufReader::new(buf)
            }
        }
    }

    fn decode_line(line_res: IoResult<String>) -> Option<SessionRecord> {
        let line = match line_res {
            Ok(ok) => ok,
            Err(err) => fail!("error reading: {}", err)
        };

        let trim_these: &[_] = &['\r', '\n'];
        let slice = line.as_slice().trim_right_chars(trim_these);

        if slice.starts_with(">> ") {
            return match IrcMessage::from_str(slice[3..]) {
                Ok(irc_msg) => Some(ContentLine(irc_msg)),
                Err(_) => None
            }
        }
        if slice.starts_with("## ") {
            return Some(CommentLine(slice[3..].to_string()));
        }
        None
    }

    fn marker_match(rec: &SessionRecord, target: &str) -> bool {
        match *rec {
            CommentLine(ref comm) => comm.as_slice() == target,
            _ => false
        }
    }

    #[test]
    fn test_state_tracking() {
        let mut reader = BufReader::new(TEST_SESSION_STATETRACKER);
        let mut iterator = reader.lines().filter_map(decode_line);
        let mut bundler = BundlerManager::new();
        bundler.add_bundler_trigger(box JoinBundlerTrigger::new());
        bundler.add_bundler_trigger(box WhoBundlerTrigger::new());

        let mut state = State::new();
        
        let it = |target: &str, statefunc: |&mut State|| {
            if target != "" {
                for rec in iterator {
                    if marker_match(&rec, target) {
                        break;
                    }
                    if let ContentLine(ref content) = rec {
                        for event in bundler.on_message(content).iter() {
                            state.on_event(event);
                        }
                    }
                }
            }
            statefunc(&mut state);
        };


        let mut random_user_id_hist = Vec::new();
        let mut chan_test_id_hist = Vec::new();

        it("should have a channel `#test` with 7 users", |state| {
            let channel_id = match state.channel_map.find(&"#test".to_string()) {
                Some(channel_id) => *channel_id,
                None => fail!("channel `#test` not found.")
            };
            chan_test_id_hist.push(channel_id);

            let channel_state = match state.channels.find(&channel_id) {
                Some(channel) => channel.clone(),
                None => fail!("channel `#test` had Id but no state")
            };
            assert_eq!(channel_state.users.len(), 7);
        });

        it("topic of `#test` should be `irc is bad.`", |state| {
            let chan_id = state.identify_channel("#test").unwrap();
            let channel = state.channels.find(&chan_id).unwrap();
            assert_eq!(channel.topic.as_slice(), "irc is bad.");
        });

        it("should have a user `randomuser` after JOIN", |state| {
            let randomuser_id = state.identify_nick("randomuser").unwrap();
            if random_user_id_hist.contains(&randomuser_id) {
                assert!(false, "nick `randomuser` BotUserId must change between losses in view");
            }
            random_user_id_hist.push(randomuser_id);
            match state.users.find(&randomuser_id) {
                Some(randomuser) => {
                    assert_eq!(
                        randomuser.prefix.as_slice(),
                        "randomuser!rustbot@coolhost");
                },
                None => fail!("inconsistent state. state = {}", state)
            }
        });

        it("should not have a user `randomuser` after PART", |state| {
            assert!(state.identify_nick("randomuser").is_none());
        });

        it("topic of `#test` should be `setting a cool topic`", |state| {
            let chan_id = state.identify_channel("#test").unwrap();
            let channel = state.channels.find(&chan_id).unwrap();
            assert_eq!(channel.topic.as_slice(), "setting a cool topic");
        });

        let mut randomuser_id: Option<BotUserId> = None;
        it("store randomuser's UserID here", |state| {
            randomuser_id = state.identify_nick("randomuser")
        });
        let randomuser_id = randomuser_id.expect("bad randomuser");

        it("ensure randomuser's UserID has not changed after a nick change", |state| {
            assert_eq!(state.identify_nick("resumodnar"), Some(randomuser_id));
        });

        it("should not have a channel `#test` anymore", |state| {
            assert!(
                state.identify_channel("#test").is_none(),
                "#test was not cleaned up correctly");
        });

        it("should have the channel `#test` once again", |state| {
            let test_id = state.identify_channel("#test").unwrap();
            if chan_test_id_hist.contains(&test_id) {
                assert!(false, "channel `#test` BotChannelId must change between losses in view");
            }
            chan_test_id_hist.push(test_id);
        });

        let mut randomuser_id: Option<BotUserId> = None;

        it("should have a channel `#test2` with 2 users", |state| {
            assert!(state.identify_channel("#test2").is_some());
            randomuser_id = state.identify_nick("randomuser");
            assert!(randomuser_id.is_some(), "randomuser wasn't found!");
        });

        it("randomuser should have the same ID as before", |state| {
            assert!(state.identify_channel("#test2").is_some());
            assert_eq!(
                state.identify_nick("randomuser").unwrap(),
                randomuser_id.unwrap());
        });

        it("randomuser should not have the same ID as before", |state| {
            assert!(state.identify_channel("#test2").is_some());
            if state.identify_nick("randomuser") == randomuser_id {
                assert!(false, "randomuser should be different now");
            }
        });
    }
}
