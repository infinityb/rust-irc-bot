use std::fmt;

use std::collections::hashmap::{Occupied, Vacant};
use std::collections::{
    HashMap,
    HashSet
};

use irc::{
    IrcMessage,
    IrcPrefix,
    JoinSuccess,
    WhoRecord,
    WhoSuccess,
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

impl InternalUser {
    fn from_info(user_info: &UserInfo) -> InternalUser {
        InternalUser {
            id: user_info.id,
            prefix: user_info.prefix.clone(),
            channels: HashSet::new(),
        }
    }

    fn get_nick(&self) -> &str {
        let prefix = self.prefix.as_slice();
        match prefix.find('!') {
            Some(idx) => prefix[0..idx],
            None => prefix
        }
    }

    fn set_nick(&mut self, nick: &str) {
        let old_prefix = IrcPrefix::new(self.prefix.as_slice());
        self.prefix = old_prefix.with_nick(nick).to_string();
    }
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

impl InternalChannel {
    fn from_info(chan_info: &ChannelInfo) -> InternalChannel {
        InternalChannel {
            id: chan_info.id,
            name: chan_info.name.clone(),
            topic: chan_info.topic.clone(),
            users: HashSet::new(),
        }
    }
}

impl fmt::Show for InternalChannel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InternalChannel(name: {})", self.name)
    }
}

#[deriving(Show)]
struct UserInfo {
    id: BotUserId,
    prefix: String
}

impl UserInfo {
    fn from_who(id: BotUserId, who: &WhoRecord) -> UserInfo {
        UserInfo {
            id: id,
            prefix: who.get_prefix()
        }
    }

    fn from_join(id: BotUserId, join: &IrcMessage) -> UserInfo {
        UserInfo {
            id: id,
            prefix: join.get_prefix_raw().unwrap().to_string()
        }
    }

    fn get_nick(&self) -> &str {
        let prefix = self.prefix.as_slice();
        match prefix.find('!') {
            Some(idx) => prefix[0..idx],
            None => prefix
        }
    }
}

#[deriving(Show)]
struct ChannelInfo {
    id: BotChannelId,
    name: String,
    topic: String
}

impl ChannelInfo {
    fn from_join(id: BotChannelId, join: &JoinSuccess) -> ChannelInfo {
        let topic = match join.topic {
            Some(ref topic) => topic.text.clone(),
            None => String::new()
        };

        ChannelInfo {
            id: id,
            name: join.channel.to_string(),
            topic: topic
        }
    }
}

struct StateCommandStreamBuilder<'a> {
    state: &'a State,
    user_seq: u64,
    channel_seq: u64,
}

impl<'a> StateCommandStreamBuilder<'a> {
    fn new(state: &'a State) -> StateCommandStreamBuilder<'a> {
        StateCommandStreamBuilder {
            state: state,
            user_seq: state.user_seq,
            channel_seq: state.channel_seq
        }
    }

    fn on_other_part(&self, msg: &IrcMessage) -> Vec<StateCommand> {
        let msg_args = msg.get_args();
        if msg_args.len() < 1 {
            warn!("Invalid message. PART with no arguments: {}", msg);
            return Vec::new();
        }
        let channel_name = msg_args[0].to_string();
        let user_nick = match msg.source_nick() {
            Some(user_nick) => user_nick.to_string(),
            None => {
                warn!("Invalid message. PART with no prefix: {}", msg);
                return Vec::new();
            }
        };
        let chan_id = match self.state.channel_map.find(&channel_name) {
            Some(chan_id) => *chan_id,
            None => panic!("Got message for channel {} without knowing about it.", channel_name)
        };
        let user_id = match self.state.user_map.find(&user_nick) {
            Some(user_id) => *user_id,
            None => panic!("Got message for user {} without knowing about it.", channel_name)
        };
        let mut commands = Vec::new();
        commands.push(RemoveUserFromChannel(user_id, chan_id));

        if let Some(user_state) = self.state.users.find(&user_id) {
            if user_state.channels.len() == 1 {
                if user_state.channels.contains(&chan_id) {
                    commands.push(RemoveUser(user_id));
                } else {
                    panic!("Inconsistent state");
                }
            }
        } else {
            panic!("Inconsistent state")
        }
        commands
    }

    fn on_self_part(&self, msg: &IrcMessage) -> Vec<StateCommand> {
        info!("on-self-part called.  We should clean the channel up");
        let msg_args = msg.get_args();
        if msg_args.len() < 1 {
            warn!("Invalid message. PART with no arguments: {}", msg);
            return Vec::new();
        }

        let mut commands = Vec::new();
        let channel_name = msg_args[0].to_string();
        let chan_state = match self.state.channel_map.find(&channel_name) {
            Some(chan_id) => match self.state.channels.find(chan_id) {
                Some(chan_state) => chan_state,
                None => panic!("Inconsistent state")
            },
            None => panic!("We left {} without knowing about it.", channel_name)
        };
        // Should this be here or should we just do it on the receiver?
        // If we move this out, we should still validate that our state is
        // consistent, to prevent crashing the receiver.
        for &user_id in chan_state.users.iter() {
            let user_state = match self.state.users.find(&user_id) {
                Some(user_state) => user_state,
                None => panic!("Inconsistent state: {}", user_id),
            };
            commands.push(RemoveUserFromChannel(user_id, chan_state.id));
            if user_state.channels.len() == 1 && user_state.channels.contains(&chan_state.id) {
                commands.pop();
                commands.push(RemoveUser(user_id));
            }
        }

        commands.push(RemoveChannel(chan_state.id));
        return commands;
    }

    fn on_other_join(&mut self, join: &IrcMessage) -> Vec<StateCommand> {
        let channel = join.get_args()[0].to_string();

        let chan_id = match self.state.channel_map.find(&channel) {
            Some(channel_id) => *channel_id,
            None => return Vec::new()
        };
        let source_nick = match join.source_nick() {
            Some(nick) => nick.to_string(),
            None => return Vec::new(),
        };
        let mut commands = Vec::new();
        let user_id = match self.state.user_map.find(&source_nick) {
            Some(user_id) => *user_id,
            None => {
                let new_user_id = BotUserId(self.user_seq);
                self.user_seq += 1;
                commands.push(AddUser(UserInfo::from_join(new_user_id, join)));
                commands.push(IncrementUserId);
                new_user_id
            }
        };
        commands.push(AddUserToChannel(user_id, chan_id));
        commands
    }

    fn on_self_join(&mut self, join: &JoinSuccess) -> Vec<StateCommand> {
        if let Some(_) = self.state.channel_map.find(&join.channel) {
            warn!("Joining already joined channel {}; skipped", join.channel);
            return Vec::new();
        }

        let new_chan_id = BotChannelId(self.channel_seq);
        self.channel_seq += 1;

        let mut commands = Vec::with_capacity(4);
        commands.push(AddChannel(ChannelInfo::from_join(new_chan_id, join)));
        commands.push(IncrementChannelId);
        commands
    }

    fn on_who_record(&mut self, chan_id: BotChannelId, rec: &WhoRecord) -> Vec<StateCommand> {
        let mut commands = Vec::with_capacity(4);
        let user_id = match self.state.user_map.find(&rec.nick) {
            Some(user_id) => *user_id,
            None => {
                let new_user_id = BotUserId(self.user_seq);
                self.user_seq += 1;
                commands.push(AddUser(UserInfo::from_who(new_user_id, rec)));
                commands.push(IncrementUserId);
                new_user_id
            }
        };
        commands.push(AddUserToChannel(user_id, chan_id));
        commands
    }

    fn on_who(&mut self, who: &WhoSuccess) -> Vec<StateCommand> {
        // If we WHO a channel that we aren't in, we aren't changing any
        // state.
        let channel_id = match self.state.channel_map.find(&who.channel) {
            Some(channel_id) => *channel_id,
            None => return Vec::new()
        };

        let mut commands = Vec::new();
        for rec in who.who_records.iter() {
            commands.extend(self.on_who_record(channel_id, rec).into_iter());
        }
        commands
    }

    fn on_topic(&self, msg: &IrcMessage) -> Vec<StateCommand> {
        assert_eq!(msg.command(), "TOPIC");
        assert_eq!(msg.get_args().len(), 2);
        let channel = msg.get_args()[0].to_string();
        let new_topic = msg.get_args()[1].to_string();

        let chan_id = match self.state.channel_map.find(&channel) {
            Some(channel_id) => *channel_id,
            None => return Vec::new()
        };
        let mut commands = Vec::new();
        commands.push(UpdateChannel(ChannelInfo {
            id: chan_id,
            name: channel,
            topic: new_topic,
        }));
        commands
    }

    fn on_nick(&self, msg: &IrcMessage) -> Vec<StateCommand> {
        assert_eq!(msg.command(), "NICK");
        assert_eq!(msg.get_args().len(), 1);

        let new_nick = msg.get_args()[0].to_string();
        let source_nick = match msg.source_nick() {
            Some(source_nick) => source_nick.to_string(),
            None => return Vec::new(),
        };
        let prefix = match msg.get_prefix() {
            Some(prefix) => prefix,
            None => return Vec::new(),
        };

        let user_id = match self.state.user_map.find(&source_nick) {
            Some(user_id) => *user_id,
            None => return Vec::new()
        };
        let mut commands = Vec::new();
        commands.push(UpdateUser(UserInfo {
            id: user_id,
            prefix: prefix.with_nick(new_nick[]).to_string()
        }));
        commands
    }

    fn from_message(&mut self, msg: &IrcMessage) -> Vec<StateCommand> {
        if msg.command() == "001" {
            return vec![UpdateSelfNick(msg.get_args()[0].to_string())];
        }
        if msg.command() == "PART" {
            info!("handling PART, {} == {}", msg.source_nick(), Some(self.state.self_nick.as_slice()));
            if msg.source_nick() == Some(self.state.self_nick.as_slice()) {
                return self.on_self_part(msg);
            } else {
                return self.on_other_part(msg);
            }
        }
        if msg.command() == "JOIN" && msg.get_prefix().is_some() {
            return self.on_other_join(msg);
        }
        if msg.command() == "TOPIC" && msg.get_prefix().is_some() {
            return self.on_topic(msg);
        }
        if msg.command() == "NICK" && msg.get_prefix().is_some() {
            return self.on_nick(msg);
        }
        Vec::new()
    }

    fn from_event(&mut self, event: &IrcEvent) -> Vec<StateCommand> {
        match *event {
            IrcEventMessage(ref message) => self.from_message(message),
            IrcEventJoinBundle(Ok(ref join_bun)) => self.on_self_join(join_bun),
            IrcEventJoinBundle(Err(_)) => Vec::new(),
            IrcEventWhoBundle(Ok(ref who_bun)) => self.on_who(who_bun),
            IrcEventWhoBundle(Err(_)) => Vec::new(),
        }
    }
}

#[deriving(Show)]
pub enum StateCommand {
    UpdateSelfNick(String),
    AddUser(UserInfo),
    UpdateUser(UserInfo),
    RemoveUser(BotUserId),
    RemoveUserFromChannel(BotUserId, BotChannelId),
    AddChannel(ChannelInfo),
    UpdateChannel(ChannelInfo),
    RemoveChannel(BotChannelId),
    // RemoveChannelFromUser(BotChannelId, BotUserId),
    IncrementUserId,
    IncrementChannelId,
    AddUserToChannel(BotUserId, BotChannelId),
}


pub struct StateBuilder;

#[deriving(Show)]
pub struct State {
    user_seq: u64,
    channel_seq: u64,

    self_nick: String,
    user_map: HashMap<String, BotUserId>,
    users: HashMap<BotUserId, InternalUser>,
    channel_map: HashMap<String, BotChannelId>,
    channels: HashMap<BotChannelId, InternalChannel>,
}

impl State {
    pub fn new() -> State {
        State {
            user_seq: 0,
            channel_seq: 0,
            self_nick: String::new(),
            user_map: HashMap::new(),
            users: HashMap::new(),
            channel_map: HashMap::new(),
            channels: HashMap::new(),
        }
    }

    pub fn get_self_nick<'a>(&'a self) -> &'a str {
        self.self_nick.as_slice()
    }

    fn apply_update_self_nick(&mut self, new_nick: &String) {
        if self.self_nick.as_slice() != "" {
            let user_id = match self.user_map.pop(&self.self_nick) {
                Some(user_id) => user_id,
                None => panic!("inconsistent user_map: {}[{}]",
                    self.user_map, self.self_nick)
            };
            self.user_map.insert(new_nick.clone(), user_id);
        }
        self.self_nick = new_nick.clone();
    }

    fn apply_remove_user(&mut self, id: BotUserId) {
        info!("apply_remove_user({})", id);
        let user_info = match self.users.pop(&id) {
            Some(user_info) => user_info,
            None => panic!("cannot apply command: {} not found.", id)
        };
        let user_nick = user_info.get_nick().to_string();
        match self.user_map.pop(&user_nick) {
            Some(user_id) => assert_eq!(user_id, id),
            None => panic!("inconsistent user_map: {}[{}]",
                self.user_map, user_nick)
        };
    }

    fn apply_remove_user_from_chan(&mut self,
                                   uid: BotUserId,
                                   chid: BotChannelId
    ) {
        info!("remove_user_from_chan({}, {})", uid, chid);
        match self.users.find_mut(&uid) {
            Some(user_info) => user_info.channels.remove(&chid),
            None => panic!("cannot apply command: {} not found.", uid)
        };
        match self.channels.find_mut(&chid) {
            Some(channel_info) => channel_info.users.remove(&uid),
            None => panic!("cannot apply command: {} not found.", chid)
        };
    }

    fn apply_remove_channel(&mut self, id: BotChannelId) {
        info!("remove_channel({})", id);
        let chan_info = match self.channels.pop(&id) {
            Some(chan_info) => chan_info,
            None => panic!("cannot apply command: {} not found.", id)
        };
        match self.channel_map.pop(&chan_info.name) {
            Some(chan_id) => assert_eq!(chan_id, id),
            None => panic!("inconsistent channel_map")
        };
    }

    fn apply_add_channel(&mut self, chan_info: &ChannelInfo) {
        assert_eq!(chan_info.id, BotChannelId(self.channel_seq));
        self.channels.insert(chan_info.id, InternalChannel::from_info(chan_info));
        self.channel_map.insert(chan_info.name.clone(), chan_info.id);
    }

    fn apply_update_channel(&mut self, chan_info: &ChannelInfo) {
        match self.channels.find_mut(&chan_info.id) {
            Some(channel) => channel.topic = chan_info.topic.clone(),
            None => {}
        };
    }

    fn apply_add_user(&mut self, user_info: &UserInfo) {
        assert_eq!(user_info.id, BotUserId(self.user_seq));
        self.users.insert(user_info.id, InternalUser::from_info(user_info));
        self.user_map.insert(user_info.get_nick().to_string(), user_info.id);
    }

    fn apply_update_user(&mut self, user_info: &UserInfo) {
        let old_nick = match self.users.find_mut(&user_info.id) {
            Some(user_state) => {
                let old_nick = user_state.get_nick().to_string();
                user_state.set_nick(user_info.get_nick());
                old_nick
            },
            None => {
                warn!("updating non-existent user");
                return;
            }
        };
        if let Some(user_id) = self.user_map.pop(&old_nick) {
            assert_eq!(user_id, user_info.id);
            self.user_map.insert(user_info.get_nick().to_string(), user_id);
            info!("user_map = {}", self.user_map);
        }
    }

    fn apply_add_user_to_channel(&mut self,
                                 uid: BotUserId,
                                 chid: BotChannelId
    ) {
        match self.users.find_mut(&uid) {
            Some(user_state) => user_state.channels.insert(chid),
            None => panic!("inconsistent state")
        };
        match self.channels.find_mut(&chid) {
            Some(channel_state) => channel_state.users.insert(uid),
            None => panic!("inconsistent state")
        };
    }

    pub fn apply_command(&mut self, cmd: &StateCommand) {
        match *cmd {
            UpdateSelfNick(ref new_nick) =>
                self.apply_update_self_nick(new_nick),
            AddUser(ref user_info) => self.apply_add_user(user_info),
            UpdateUser(ref user_info) => self.apply_update_user(user_info),
            AddUserToChannel(user_id, chan_id) =>
                self.apply_add_user_to_channel(user_id, chan_id),
            RemoveUser(user_id) => self.apply_remove_user(user_id),
            RemoveUserFromChannel(user_id, chan_id) =>
                self.apply_remove_user_from_chan(user_id, chan_id),
            AddChannel(ref chan_info) =>
                self.apply_add_channel(chan_info),
            UpdateChannel(ref chan_info) =>
                self.apply_update_channel(chan_info),
            RemoveChannel(chan_id) =>
                self.apply_remove_channel(chan_id),
            IncrementUserId => self.user_seq += 1,
            IncrementChannelId => self.channel_seq += 1,
        }
    }

    pub fn identify_channel(&self, chan: &str) -> Option<BotChannelId> {
        match self.channel_map.find(&chan.to_string()) {
            Some(chan_id) => Some(chan_id.clone()),
            None => None
        }
    }

    pub fn identify_nick(&self, nick: &str) -> Option<BotUserId> {
        match self.user_map.find(&nick.to_string()) {
            Some(user_id) => Some(*user_id),
            None => None
        }
    }
    
    pub fn on_event(&mut self, event: &IrcEvent) {
        let commands = StateCommandStreamBuilder::new(self).from_event(event);
        for command in commands.iter() {
            info!("command application: {}", command);
            self.apply_command(command);
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
            Err(err) => panic!("error reading: {}", err)
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
                None => panic!("channel `#test` not found.")
            };
            chan_test_id_hist.push(channel_id);

            let channel_state = match state.channels.find(&channel_id) {
                Some(channel) => channel.clone(),
                None => panic!("channel `#test` had Id but no state")
            };
            assert_eq!(channel_state.users.len(), 7);
        });

        it("topic of `#test` should be `irc is bad.`", |state| {
            let msg = format!("state.identify_channel failed on line {}", 1 + line!());
            let chan_id = state.identify_channel("#test").expect(msg.as_slice());
            let msg = format!("state.channels.find failed on line {}", 1 + line!());
            let channel = state.channels.find(&chan_id).expect(msg.as_slice());
            assert_eq!(channel.topic.as_slice(), "irc is bad.");
        });

        it("should have a user `randomuser` after JOIN", |state| {
            let msg = format!("state.identify_nick failed on line {}", 1 + line!());
            let randomuser_id = state.identify_nick("randomuser").expect(msg.as_slice());
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
                None => panic!("inconsistent state. state = {}", state)
            }
        });

        it("should not have a user `randomuser` after PART", |state| {
            assert!(state.identify_nick("randomuser").is_none());
        });

        it("topic of `#test` should be `setting a cool topic`", |state| {
            let msg = format!("state.identify_channel failed on line {}", 1 + line!());
            let chan_id = state.identify_channel("#test").expect(msg.as_slice());
            let msg = format!("state.channels.find failed on line {}", 1 + line!());
            let channel = state.channels.find(&chan_id).expect(msg.as_slice());
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
