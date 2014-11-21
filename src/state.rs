#![warn(dead_code)]
#![deny(unused_variables, unused_mut)]


use std::fmt;
use std::cmp::max;

use std::ascii::AsciiExt;
use std::default::Default;
use std::collections::{
    HashMap,
    HashSet,
};
use std::collections::hash_map::{
    Occupied,
    Vacant,
};

use irc::{
    IrcMessage,
    IrcMsgPrefix,
    JoinSuccess,
    WhoRecord,
    WhoSuccess,
    IrcEvent,
};
pub use self::MessageEndpoint::{
    KnownUser,
    KnownChannel,
    AnonymousUser,
};

macro_rules! deref_opt_or_return(
    ($inp:expr, $erp:expr, $fr:expr) => (
        match $inp {
            Some(x) => *x,
            _ => {
                warn!($erp);
                return $fr;
            }
        }
    );
)


pub type BotUserId = UserId;
pub type BotChannelId = ChannelId;

#[deriving(Clone, Show)]
pub enum MessageEndpoint {
    KnownUser(UserId),
    KnownChannel(ChannelId),
    Server(String),
    AnonymousUser,
}




#[deriving(Clone, Show, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserId(u64);



trait Diff<DiffType> {
    fn diff(&self, other: &Self) -> DiffType;
}

trait Patch<DiffType> {
    fn patch(&self, diff: &DiffType) -> Self;
}

fn check_patch<DT: fmt::Show, T: fmt::Show+Diff<DT>+Patch<DT>+Eq>(left: &T, right: &T) -> bool {
    let diff = left.diff(right);
    let result = left.patch(&diff) == *right;
    info!("applying diff: {}", diff);
    if !result {
        panic!("{} -> {}, invalid diff: {}", left, right, diff);
    }
    result
}

#[deriving(Clone, PartialEq, Eq, Show)]
struct User {
    id: UserId,
    prefix: IrcMsgPrefix<'static>,
    channels: HashSet<ChannelId>
}

impl User {
    fn from_who(id: UserId, who: &WhoRecord) -> User {
        User {
            id: id,
            prefix: who.get_prefix().into_owned(),
            channels: Default::default(),
        }
    }

    fn from_info(user_info: &UserInfo) -> User {
        User {
            id: user_info.id,
            prefix: user_info.prefix.clone(),
            channels: Default::default(),
        }
    }

    pub fn get_nick(&self) -> &str {
        let prefix = self.prefix.as_slice();
        match prefix.find('!') {
            Some(idx) => prefix[0..idx],
            None => prefix
        }
    }

    fn set_nick(&mut self, nick: &str) {
        self.prefix = self.prefix.with_nick(nick).expect("Need nicked prefix");
    }
}

impl Diff<Vec<UserDiffCmd>> for User {
    fn diff(&self, other: &User) -> Vec<UserDiffCmd> {
        let mut cmds = Vec::new();
        if self.prefix != other.prefix {
            cmds.push(UserDiffCmd::ChangePrefix(other.prefix.as_slice().to_string()));
        }
        for &added_channel in other.channels.difference(&self.channels) {
            cmds.push(UserDiffCmd::AddChannel(added_channel));
        }
        for &removed_channel in self.channels.difference(&other.channels) {
            cmds.push(UserDiffCmd::RemoveChannel(removed_channel));
        }
        cmds
    }
}

impl Patch<Vec<UserDiffCmd>> for User {
    fn patch(&self, diff: &Vec<UserDiffCmd>) -> User {
        let mut other = self.clone();
        for cmd in diff.iter() {
            match *cmd {
                UserDiffCmd::ChangePrefix(ref prefix_str) => {
                    other.prefix = IrcMsgPrefix::new(prefix_str.clone().into_maybe_owned());
                },
                UserDiffCmd::AddChannel(chan_id) => {
                    other.channels.insert(chan_id);
                },
                UserDiffCmd::RemoveChannel(chan_id) => {
                    other.channels.remove(&chan_id);
                }
            }
        }
        other
    }
}

#[deriving(Clone, Show, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(u64);


#[deriving(Clone, PartialEq, Eq, Show)]
struct Channel {
    id: ChannelId,
    name: String,
    topic: String,
    users: HashSet<UserId>
}

impl Channel {
    fn from_info(chan_info: &ChannelInfo) -> Channel {
        Channel {
            id: chan_info.id,
            name: chan_info.name.clone(),
            topic: chan_info.topic.clone(),
            users: Default::default(),
        }
    }

    fn set_topic(&mut self, topic: &str) {
        self.topic.clear();
        self.topic.push_str(topic);
    }
}

impl Diff<Vec<ChannelDiffCmd>> for Channel {
    fn diff(&self, other: &Channel) -> Vec<ChannelDiffCmd> {
        let mut cmds = Vec::new();
        if self.topic != other.topic {
            cmds.push(ChannelDiffCmd::ChangeTopic(other.topic.clone()));
        }
        for &added_user in other.users.difference(&self.users) {
            cmds.push(ChannelDiffCmd::AddUser(added_user));
        }
        for &removed_user in self.users.difference(&other.users) {
            cmds.push(ChannelDiffCmd::RemoveUser(removed_user));
        }
        assert_eq!(self.clone().patch(&cmds), *other);
        cmds
    }
}

impl Patch<Vec<ChannelDiffCmd>> for Channel {
    fn patch(&self, diff: &Vec<ChannelDiffCmd>) -> Channel {
        let mut other = self.clone();
        for cmd in diff.iter() {
            match *cmd {
                ChannelDiffCmd::ChangeTopic(ref topic) => {
                    other.topic = topic.clone();
                },
                ChannelDiffCmd::AddUser(user_id) => {
                    other.users.insert(user_id);
                },
                ChannelDiffCmd::RemoveUser(user_id) => {
                    other.users.remove(&user_id);
                }
            }
        }
        other
    }
}

#[deriving(Show)]
struct UserInfo {
    id: UserId,
    prefix: IrcMsgPrefix<'static>,
}

impl UserInfo {
    fn from_internal(user: &User) -> UserInfo {
        UserInfo {
            id: user.id,
            prefix: user.prefix.into_owned(),
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
    id: ChannelId,
    name: String,
    topic: String
}

impl ChannelInfo {
    fn from_internal(chan: &Channel) -> ChannelInfo {
        ChannelInfo {
            id: chan.id,
            name: chan.name.clone(),
            topic: chan.topic.clone()
        }
    }

    fn from_join(id: ChannelId, join: &JoinSuccess) -> ChannelInfo {
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

#[deriving(Show)]
pub enum ChannelDiffCmd {
    ChangeTopic(String),
    AddUser(UserId),
    RemoveUser(UserId),
}

#[deriving(Show)]
pub enum UserDiffCmd {
    ChangePrefix(String),
    AddChannel(ChannelId),
    RemoveChannel(ChannelId),
}


#[deriving(Show)]
pub enum StateCommand {
    CreateUser(UserInfo),
    UpdateUser(UserId, Vec<UserDiffCmd>),
    RemoveUser(UserId),

    CreateChannel(ChannelInfo),
    UpdateChannel(ChannelId, Vec<ChannelDiffCmd>),
    RemoveChannel(ChannelId),

    UpdateSelfNick(String),
    SetGeneration(u64),
}

#[deriving(Show)]
pub struct StateDiff {
    from_generation: u64,
    to_generation: u64,
    commands: Vec<StateCommand>
}

#[deriving(Show, Clone)]
pub struct State {
    // Can this be made diffable by using sorted `users`, `channels`,
    // `users[].channels` and `channels[].users`?  TreeSet.
    user_seq: u64,
    channel_seq: u64,

    self_nick: String,
    self_id: UserId,

    user_map: HashMap<String, UserId>,
    users: HashMap<UserId, User>,

    channel_map: HashMap<String, ChannelId>,
    channels: HashMap<ChannelId, Channel>,

    generation: u64,
}

impl State {
    pub fn new() -> State {
        State {
            user_seq: 1,
            channel_seq: 0,
            self_nick: String::new(),
            user_map: Default::default(),
            users: Default::default(),
            self_id: UserId(0),
            channel_map: Default::default(),
            channels: Default::default(),
            generation: 0,
        }
    }

    fn on_other_part(&mut self, msg: &IrcMessage) {
        let msg_args = msg.get_args();
        if msg_args.len() < 1 {
            warn!("Invalid message. PART with no arguments: {}", msg);
            return;
        }
        let channel_name = msg_args[0].to_ascii_lower();
        let user_nick = match msg.source_nick() {
            Some(user_nick) => user_nick.to_string(),
            None => {
                warn!("Invalid message. PART with no prefix: {}", msg);
                return;
            }
        };

        let chan_id = deref_opt_or_return!(self.channel_map.get(&channel_name),
            "Got channel without knowing about it.", ());
        let user_id = deref_opt_or_return!(self.user_map.get(&user_nick),
            "Got user without knowing about it.", ());

        self.validate_state_internal_panic();
        self.unlink_user_channel(user_id, chan_id);
    }

    fn on_self_part(&mut self, msg: &IrcMessage) {
        let msg_args = msg.get_args();
        if msg_args.len() < 1 {
            warn!("Invalid message. PART with no arguments: {}", msg);
            return;
        }
        assert!(self.remove_channel_by_name(msg_args[0]).is_some());
    }

    fn on_other_quit(&mut self, msg: &IrcMessage) {
        match msg.source_nick() {
            Some(user_nick) => self.remove_user_by_nick(user_nick),
            None => {
                warn!("Invalid message. QUIT with no prefix: {}", msg);
                return;
            }
        };
    }

    fn on_other_join(&mut self, join: &IrcMessage) {
        let msg_args = join.get_args();
        if msg_args.len() < 1 {
            warn!("Invalid message. JOIN with no arguments: {}", join);
            return;
        }
        let channel_name = msg_args[0].to_ascii_lower();
        let user_nick = match join.source_nick() {
            Some(user_nick) => user_nick,
            None => {
                warn!("Invalid message. JOIN with no prefix: {}", join);
                return;
            }
        };
        let chan_id = match self.channel_map.get(&channel_name) {
            Some(chan_id) => *chan_id,
            None => panic!("Got message for channel {} without knowing about it.", channel_name)
        };
        
        let (is_create, user_id) = match self.user_map.get(&user_nick.to_string()) {
            Some(user_id) => {
                (false, *user_id)
            },
            None => {
                let new_user_id = UserId(self.user_seq);
                self.user_seq += 1;
                (true, new_user_id)
            }
        };
        if is_create {
            let user = User {
                id: user_id,
                prefix: join.get_prefix().expect("user lacking prefix").into_owned(),
                channels: HashSet::new(),
            };
            self.users.insert(user_id, user);
            self.user_map.insert(user_nick.to_string(), user_id);
        }
        self.users.get_mut(&user_id).expect("user not found").channels.insert(chan_id);

        assert!(self.update_channel_by_name(channel_name.as_slice(), proc(channel) {
            channel.users.insert(user_id);
        }), "Got message for channel {} without knowing about it.");
    }

    fn on_self_join(&mut self, join: &JoinSuccess) {
        let channel_name = join.channel.as_slice().to_ascii_lower();
        if let Some(_) = self.channel_map.get(&channel_name) {
            warn!("Joining already joined channel {}; skipped", join.channel);
            return;
        }
        warn!("users = {}", join.nicks);
        let new_chan_id = ChannelId(self.channel_seq);
        self.channel_seq += 1;

        self.channels.insert(new_chan_id, Channel::from_info(
            &ChannelInfo::from_join(new_chan_id, join)));
        self.channel_map.insert(channel_name.clone(), new_chan_id);
    }

    fn validate_state_with_who(&self, who: &WhoSuccess) {
        let chan_name = who.channel.as_slice().to_ascii_lower();

        let (_, channel) = match self.get_channel_by_name(chan_name.as_slice()) {
            Some(chan_pair) => chan_pair,
            None => return
        };

        info!("Validating channel state");
        let mut known_users = HashSet::new();
        for user in channel.users.iter() {
            match self.users.get(user) {
                Some(user) => {
                    known_users.insert(user.get_nick().to_string());
                },
                None => panic!("Inconsistent state"),
            }
        }
        
        let mut valid_users = HashSet::new();
        for rec in who.who_records.iter() {
            valid_users.insert(rec.nick.clone());
        }
        
        let mut is_valid = true;
        for valid_unknowns in valid_users.difference(&known_users) {
            warn!("Valid but unknown nick: {}", valid_unknowns);
            is_valid = false;
        }

        for invalid_knowns in known_users.difference(&valid_users) {
            warn!("Known but invalid nick: {}", invalid_knowns);
            is_valid = false;
        }

        if is_valid {
            info!("Channel state has been validated: sychronized");
        } else {
            warn!("Channel state has been validated: desynchronized!");
        }
    }

    fn on_who(&mut self, who: &WhoSuccess) {
        // If we WHO a channel that we aren't in, we aren't changing any
        // state.
        let chan_name = who.channel.to_ascii_lower();

        let chan_id = match self.get_channel_by_name(chan_name.as_slice()) {
            Some((chan_id, channel)) => {
                if !channel.users.is_empty() {
                    self.validate_state_with_who(who);
                    return;
                }
                chan_id
            }
            None => return
        };

        let nicks: Vec<_> = who.who_records.iter().map(|who| who.nick.clone()).collect();
        warn!("nicks: {}", nicks);

        let mut users = Vec::with_capacity(who.who_records.len());
        let mut user_ids = Vec::with_capacity(who.who_records.len());

        for rec in who.who_records.iter() {
            user_ids.push(match self.user_map.get(&rec.nick) {
                Some(user_id) => *user_id,
                None => {
                    let new_user_id = UserId(self.user_seq);
                    self.user_seq += 1;
                    users.push(User::from_who(new_user_id, rec));
                    new_user_id
                }
            });
        }
        for user in users.into_iter() {
            self.insert_user(user);
        }
        for user_id in user_ids.iter() {
            match self.users.get_mut(user_id) {
                Some(user_state) => {
                    user_state.channels.insert(chan_id);
                },
                None => {
                    if *user_id != self.self_id {
                        panic!("{}", user_id);
                    }
                }
            };
        }

        let tmp_chan_name = chan_name.clone();
        assert!(self.update_channel_by_name(chan_name.as_slice(), proc(channel) {
            let added = user_ids.len() - channel.users.len();
            info!("Added {} users for channel {}", added, tmp_chan_name);
            channel.users.extend(user_ids.into_iter());
        }), "Got message for channel {} without knowing about it.");
    }

    fn on_topic(&mut self, msg: &IrcMessage) {
        assert_eq!(msg.command(), "TOPIC");
        assert_eq!(msg.get_args().len(), 2);

        let chan_name = msg.get_args()[0].to_ascii_lower();
        assert!(self.update_channel_by_name(chan_name[], proc(channel) {
            channel.set_topic(msg.get_args()[1]);
        }));
    }

    fn on_nick(&mut self, msg: &IrcMessage) {
        assert_eq!(msg.command(), "NICK");
        assert_eq!(msg.get_args().len(), 1);

        assert!(self.update_user_by_nick(msg.source_nick().unwrap(), proc(user) {
            user.set_nick(msg.get_args()[0]);
        }))
    }

    fn on_kick(&mut self, msg: &IrcMessage) {
        assert_eq!(msg.command(), "KICK");
        assert_eq!(msg.get_args().len(), 3);

        let channel_name = msg.get_args()[0].to_ascii_lower();
        let kicked_user_nick = msg.get_args()[1].to_string();

        let (chan_id, user_id) = match (
            self.channel_map.get(&channel_name),
            self.user_map.get(&kicked_user_nick)
        ) {
            (Some(chan_id), Some(user_id)) => (*chan_id, *user_id),
            (None, Some(_)) => {
                warn!("Strange: unknown channel {}", channel_name);
                return;
            },
            (Some(_), None) => {
                warn!("Strange: unknown nick {}", kicked_user_nick);
                return;
            },
            (None, None) => {
                warn!("Strange: unknown chan {} and nick {}", channel_name, kicked_user_nick);
                return;
            }
        };
        self.unlink_user_channel(user_id, chan_id);
    }

    fn from_message(&mut self, msg: &IrcMessage) {
        let is_self = msg.source_nick() == Some(self.self_nick.as_slice());
        let () = match (msg.command(), is_self, msg.get_prefix().is_some()) {
            ("001", _, _) => {
                let self_nick = msg.get_args()[0].to_string();
                self.initialize_self_nick(&self_nick);
            },
            ("PART", true, _) => return self.on_self_part(msg),
            ("PART", false, _) => return self.on_other_part(msg),
            ("QUIT", false, _) => return self.on_other_quit(msg),
            // is this JOIN right?
            ("JOIN", false, true) => return self.on_other_join(msg),
            ("TOPIC", _, true) => self.on_topic(msg),
            ("NICK", _, true) => return self.on_nick(msg),
            ("KICK", _, true) => return self.on_kick(msg),
            _ => ()
        };
    }

    pub fn on_event(&mut self, event: &IrcEvent) {
        let () = match *event {
            IrcEvent::Message(ref message) => self.from_message(message),
            IrcEvent::JoinBundle(Ok(ref join_bun)) => self.on_self_join(join_bun),
            IrcEvent::JoinBundle(Err(_)) => (),
            IrcEvent::WhoBundle(Ok(ref who_bun)) => self.on_who(who_bun),
            IrcEvent::WhoBundle(Err(_)) => (),
        };
    }

    fn validate_state_internal_panic(&mut self) {
        match self.validate_state_internal() {
            Ok(()) => (),
            Err(msg) => panic!("invalid state: {}, dump = {}", msg, self)
        };
    }

    fn validate_state_internal(&self) -> Result<(), String> {
        for (&id, state) in self.channels.iter() {
            if id != state.id {
                return Err(format!("{} at channels[{}]", state.id, id));
            }
            for &user_id in state.users.iter() {
                if let Some(user_state) = self.users.get(&user_id) {
                    if !user_state.channels.contains(&id) {
                        return Err(format!("{0} ref {1} => {1} ref {0} not holding", id, user_id));
                    }
                } else {
                    return Err(format!("{} refs non-existent {}", id, user_id));
                }
            }
        }
        for (&id, state) in self.users.iter() {
            if id != state.id {
                return Err(format!("{} at users[{}]", state.id, id));
            }
            for &chan_id in state.channels.iter() {
                if let Some(chan_state) = self.channels.get(&chan_id) {
                    if !chan_state.users.contains(&id) {
                        return Err(format!("{0} ref {1} => {1} ref {0} not holding", id, chan_id));
                    }
                } else {
                    return Err(format!("{} refs non-existent {}", id, chan_id));
                }
            }
        }
        for (name, &id) in self.channel_map.iter() {
            if let Some(state) = self.channels.get(&id) {
                if name.as_slice() != state.name.as_slice() {
                    return Err(format!("{} at channel_map[{}]", state.id, name));
                }
            } else {
                return Err(format!("channel map inconsistent"));
            }
        }
        for (name, &id) in self.user_map.iter() {
            if let Some(state) = self.users.get(&id) {
                if name.as_slice() != state.get_nick() {
                    return Err(format!("{} at user_map[{}]", state.id, name));
                }
            } else {
                return Err(format!(
                    concat!(
                        "user map inconsistent: self.user_map[{}] is not None ",
                        "=> self.users[{}] is not None"
                    ), name, id));
            }
        }
        Ok(())
    }

    pub fn get_self_nick<'a>(&'a self) -> &'a str {
        self.self_nick.as_slice()
    }

    pub fn set_self_nick(&mut self, new_nick: &str) {
        if self.self_nick.as_slice() != "" {
            let user_id = match self.user_map.remove(&self.self_nick) {
                Some(user_id) => user_id,
                None => panic!("inconsistent user_map: {}[{}]",
                    self.user_map, self.self_nick)
            };
            self.user_map.insert(new_nick.to_string(), user_id);
        }
        self.self_nick = new_nick.to_string();
    }

    fn initialize_self_nick(&mut self, new_nick: &String) {
        self.user_map.insert(new_nick.clone(), self.self_id);
        self.users.insert(self.self_id, User {
            id: self.self_id,
            // FIXME: hack
            prefix: IrcMsgPrefix::new(format!("{}!someone@somewhere", new_nick[]).into_maybe_owned()),
            channels: HashSet::new(),
        });
        self.set_self_nick(new_nick.as_slice());
    }

    fn apply_update_self_nick(&mut self, new_nick: &String) {
        assert!(self.user_map.remove(&self.self_nick).is_some());
        self.set_self_nick(new_nick.as_slice());
        self.user_map.insert(new_nick.clone(), self.self_id);
    }

    fn apply_remove_channel(&mut self, id: ChannelId) {
        info!("remove_channel({})", id);
        self.remove_channel_by_id(id);
    }

    fn apply_create_chan(&mut self, chan_info: &ChannelInfo) {
        let ChannelId(chan_id) = chan_info.id;
        self.channel_seq = max(self.channel_seq, chan_id);

        self.channels.insert(chan_info.id, Channel::from_info(chan_info));
        let channel_name = chan_info.name.as_slice().to_ascii_lower();
        self.channel_map.insert(channel_name, chan_info.id);
    }

    fn apply_update_chan(&mut self, id: ChannelId, diff: &Vec<ChannelDiffCmd>) {
        match self.channels.entry(id) {
            Occupied(mut entry) => {
                let new_thing = entry.get().patch(diff);
                entry.set(new_thing);
            }
            Vacant(_) => warn!("Unknown channel {}", id)
        };
    }

    fn apply_create_user(&mut self, user_info: &UserInfo) {
        let UserId(user_id) = user_info.id;
        self.user_seq = max(self.user_seq, user_id);

        self.users.insert(user_info.id, User::from_info(user_info));
        self.user_map.insert(user_info.get_nick().to_string(), user_info.id);
    }

    fn apply_update_user(&mut self, id: UserId, diff: &Vec<UserDiffCmd>) {
        match self.users.entry(id) {
            Occupied(mut entry) => {
                let old_nick = entry.get().get_nick().to_string();
                let new_user = entry.get().patch(diff);
                let new_nick = new_user.get_nick().to_string();

                if old_nick.as_slice() != new_nick.as_slice() {
                    assert_eq!(self.user_map.remove(&old_nick), Some(id));
                    self.user_map.insert(new_nick, id);
                }
                entry.set(new_user);
            }
            Vacant(_) => warn!("Unknown channel {}", id)
        };
    }

    fn apply_remove_user(&mut self, id: UserId) {
        info!("apply_remove_user({})", id);
        let user_info = match self.users.remove(&id) {
            Some(user_info) => user_info,
            None => panic!("cannot apply command: {} not found.", id)
        };
        let user_nick = user_info.get_nick().to_string();
        match self.user_map.remove(&user_nick) {
            Some(user_id) => assert_eq!(user_id, id),
            None => panic!("inconsistent user_mapm: {}[{}]",
                self.user_map, user_nick)
        };
    }

    pub fn apply_command(&mut self, cmd: &StateCommand) {
        match *cmd {
            StateCommand::UpdateSelfNick(ref new_nick) =>
                self.apply_update_self_nick(new_nick),
            StateCommand::SetGeneration(generation) => self.generation = generation,

            StateCommand::CreateUser(ref info) =>
                self.apply_create_user(info),
            StateCommand::UpdateUser(id, ref diff) =>
                self.apply_update_user(id, diff),
            StateCommand::RemoveUser(id) =>
                self.apply_remove_user(id),

            StateCommand::CreateChannel(ref info) =>
                self.apply_create_chan(info),
            StateCommand::UpdateChannel(id, ref diff) =>
                self.apply_update_chan(id, diff),
            StateCommand::RemoveChannel(id) =>
                self.apply_remove_channel(id),
        }
    }

    fn unlink_user_channel(&mut self, uid: UserId, chid: ChannelId) {
        let should_remove = match self.users.entry(uid) {
            Occupied(mut entry) => {
                if entry.get().channels.len() == 1 && entry.get().channels.contains(&chid) {
                    true
                } else {
                    entry.get_mut().channels.remove(&chid);
                    false
                }
            }
            Vacant(_) => panic!("Inconsistent state")
        };
        if should_remove {
            warn!("removing {}", uid);
            self.remove_user_by_id(uid);
        }

        let should_remove = match self.channels.entry(chid) {
            Occupied(mut entry) => {
                if entry.get().users.len() == 1 && entry.get().users.contains(&uid) {
                    true
                } else {
                    entry.get_mut().users.remove(&uid);
                    false
                }
            },
            Vacant(_) => panic!("Inconsistent state")
        };
        if should_remove {
            warn!("removing {}", chid);
            self.remove_channel_by_id(chid);
        }
    }

    fn update_channel(&mut self, id: ChannelId, modfunc: proc(&mut Channel)) -> bool {
        match self.channels.entry(id) {
            Occupied(mut entry) => {
                // Channel currently has no indexed mutable state
                modfunc(entry.get_mut());
                true
            }
            Vacant(_) => false
        }
    }

    fn update_channel_by_name(&mut self, name: &str, modfunc: proc(&mut Channel)) -> bool {
        let lowered = name.to_ascii_lower();
        let chan_id = deref_opt_or_return!(
            self.channel_map.get(&lowered), "Unknown channel name", false);
        let result = self.update_channel(chan_id, modfunc);
        self.validate_state_internal_panic();
        result
    }

    fn remove_channel_by_name(&mut self, name: &str) -> Option<ChannelId> {
        let lowered = name.to_ascii_lower();
        let chan_id = deref_opt_or_return!(
            self.channel_map.get(&lowered), "Unknown channel name", None);
        assert!(self.remove_channel_by_id(chan_id));
        self.validate_state_internal_panic();
        Some(chan_id)
    }

    fn remove_channel_by_id(&mut self, id: ChannelId) -> bool {
        let (chan_name, users): (_, Vec<_>) = match self.channels.get(&id) {
            Some(chan_state) => (
                chan_state.name.clone(),
                chan_state.users.iter().map(|x| x.clone()).collect()
            ),
            None => return false
        };
        for user_id in users.into_iter() {
            self.channels.get_mut(&id).unwrap().users.remove(&user_id);
            self.users.get_mut(&user_id).unwrap().channels.remove(&id);
            // self.unlink_user_channel(user_id, id);
        }
        self.channels.remove(&id);
        self.channel_map.remove(&chan_name);
        self.validate_state_internal_panic();
        true
    }

    fn get_channel_by_name(&self, name: &str) -> Option<(ChannelId, &Channel)> {
        let chan_id = match self.channel_map.get(&name.to_string()) {
            Some(chan_id) => *chan_id,
            None => return None
        };
        match self.channels.get(&chan_id) {
            Some(channel) => Some((chan_id, channel)),
            None => panic!("Inconsistent state")
        }
    }

    fn insert_user(&mut self, user: User) {
        let user_id = user.id;
        let nick = user.prefix.nick().unwrap().to_string();
        assert!(self.users.insert(user_id, user).is_none());
        assert!(self.user_map.insert(nick, user_id).is_none());
        self.validate_state_internal_panic();
    }

    fn update_user_by_nick(&mut self, nick: &str, modfunc: proc(&mut User)) -> bool {
        let user_id = deref_opt_or_return!(self.user_map.get(&nick.to_string()),
            "Couldn't find user by nick", false);
        let result = self.update_user(user_id, modfunc);

        self.validate_state_internal_panic();
        result
    }

    fn update_user(&mut self, id: UserId, modfunc: proc(&mut User)) -> bool {
        match self.users.entry(id) {
            Occupied(mut entry) => {
                let prev_nick = entry.get().prefix.nick().unwrap().to_string();
                modfunc(entry.get_mut());
                let new_nick = entry.get().prefix.nick().unwrap().to_string();
                warn!("prev_nick != new_nick || {} != {}", prev_nick, new_nick);
                if prev_nick != new_nick {
                    warn!("self.user_map -- REMOVE {}; INSERT {}", prev_nick, new_nick);
                    self.user_map.remove(&prev_nick);
                    self.user_map.insert(new_nick, id);
                }
                true
            }
            Vacant(_) => false
        }
    }

    fn remove_user_by_nick(&mut self, name: &str) -> Option<UserId> {
        let user_id = match self.user_map.get(&name.to_string()) {
            Some(user_id) => *user_id,
            None => return None
        };
        match self.remove_user_by_id(user_id) {
            true => Some(user_id),
            false => panic!("Inconsistent state")
        }
    }

    fn remove_user_by_id(&mut self, id: UserId) -> bool {
        if self.self_id == id {
            panic!("Tried to remove self");
        }
        let (nick, channels): (_, Vec<_>) = match self.users.get(&id) {
            Some(user_state) => (
                user_state.prefix.nick().unwrap().to_string(),
                user_state.channels.iter().map(|x| *x).collect(),
            ),
            None => return false
        };
        for chan_id in channels.into_iter() {
            self.channels.get_mut(&chan_id).unwrap().users.remove(&id);
            self.users.get_mut(&id).unwrap().channels.remove(&chan_id);
        }

        self.users.remove(&id).unwrap();
        self.user_map.remove(&nick).unwrap();
        self.validate_state_internal_panic();
        true
    }

    pub fn identify_channel(&self, chan: &str) -> Option<ChannelId> {
        let channel_name = chan.to_ascii_lower();
        match self.channel_map.get(&channel_name) {
            Some(chan_id) => Some(chan_id.clone()),
            None => None
        }
    }

    pub fn resolve_channel(&self, chid: ChannelId) -> Option<&Channel> {
        self.channels.get(&chid)
    }

    pub fn identify_nick(&self, nick: &str) -> Option<UserId> {
        match self.user_map.get(&nick.to_string()) {
            Some(user_id) => Some(*user_id),
            None => None
        }
    }

    pub fn resolve_user(&self, uid: UserId) -> Option<&User> {
        self.users.get(&uid)
    }
}

impl Eq for State {}

impl PartialEq for State {
    fn eq(&self, other: &State) -> bool {
        for (nick, id) in self.user_map.iter() {
            if Some(id) != other.user_map.get(nick) {
                return false;
            }
        }
        for (nick, id) in other.user_map.iter() {
            if Some(id) != self.user_map.get(nick) {
                return false;
            }
        }
        for (name, id) in self.channel_map.iter() {
            if Some(id) != other.channel_map.get(name) {
                return false;
            }
        }
        for (name, id) in other.channel_map.iter() {
            if Some(id) != self.channel_map.get(name) {
                return false;
            }
        }
        for (id, self_state) in self.users.iter() {
            if let Some(other_state) = other.users.get(id) {
                if self_state != other_state {
                    return false;
                }
            } else {
                return false;
            }
        }
        for (id, self_state) in self.channels.iter() {
            if let Some(other_state) = other.channels.get(id) {
                if self_state != other_state {
                    return false;
                }
            } else {
                return false;
            }
        }

        if self.user_seq != other.user_seq {
            return false;
        }
        if self.channel_seq != other.channel_seq {
            return false;
        }
        if self.self_nick != other.self_nick {
            return false;
        }
        if self.generation != other.generation {
            return false;
        }
        return true;
    }
}

impl Diff<StateDiff> for State {
    fn diff(&self, other: &State) -> StateDiff {
        let mut commands = Vec::new();
        if self.self_nick != other.self_nick {
            commands.push(StateCommand::UpdateSelfNick(other.self_nick.clone()));
        }

        for (&id, cstate) in other.channels.iter() {
            if let Some(old_channel) = self.channels.get(&id) {
                if cstate != old_channel {
                    commands.push(StateCommand::UpdateChannel(id, old_channel.diff(cstate)));
                }
            } else {
                commands.push(StateCommand::CreateChannel(ChannelInfo::from_internal(cstate)));
                if !cstate.users.is_empty() {
                    let diff: Vec<_> = cstate.users.iter()
                        .map(|&x| ChannelDiffCmd::AddUser(x)).collect();
                    commands.push(StateCommand::UpdateChannel(id, diff));
                }
            }
        }
        for (&id, _) in self.channels.iter() {
            if !other.channels.contains_key(&id) {
                commands.push(StateCommand::RemoveChannel(id));
            }
        }

        for (&id, ustate) in other.users.iter() {
            if let Some(old_user) = self.users.get(&id) {
                if ustate != old_user {
                    commands.push(StateCommand::UpdateUser(id, old_user.diff(ustate)));
                }
            } else {
                commands.push(StateCommand::CreateUser(UserInfo::from_internal(ustate)));
                if !ustate.channels.is_empty() {
                    let diff: Vec<_> = ustate.channels.iter()
                        .map(|&x| UserDiffCmd::AddChannel(x)).collect();
                    commands.push(StateCommand::UpdateUser(id, diff));
                }
            }
        }
        for (&id, _) in self.users.iter() {
            if !other.users.contains_key(&id) {
                commands.push(StateCommand::RemoveUser(id));
            }
        }

        if self.generation != other.generation {
            commands.push(StateCommand::SetGeneration(other.generation));
        }

        StateDiff {
            from_generation: self.generation,
            to_generation: other.generation,
            commands: commands,
        }
    }
}

impl Patch<StateDiff> for State {
    fn patch(&self, diff: &StateDiff) -> State {
        let mut new = self.clone();
        assert_eq!(self.generation, diff.from_generation);
        for command in diff.commands.iter() {
            new.apply_command(command);
        }
        assert_eq!(self.generation, diff.from_generation);
        new
    }
}


#[cfg(test)]
mod tests {
    use super::{State, UserId};
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
        Content(IrcMessage),
        Comment(String),
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
                Ok(irc_msg) => Some(SessionRecord::Content(irc_msg)),
                Err(_) => None
            }
        }
        if slice.starts_with("## ") {
            return Some(SessionRecord::Comment(slice[3..].to_string()));
        }
        None
    }

    fn marker_match(rec: &SessionRecord, target: &str) -> bool {
        match *rec {
            SessionRecord::Comment(ref comm) => comm.as_slice() == target,
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
                    if let SessionRecord::Content(ref content) = rec {
                        for event in bundler.on_message(content).iter() {
                            state.on_event(event);
                            state.validate_state_internal_panic();
                        }
                    }
                }
            }
            statefunc(&mut state);
        };

        let mut random_user_id_hist = Vec::new();
        let mut chan_test_id_hist = Vec::new();

        it("should have a channel `#test` with 7 users", |state| {
            let channel_id = match state.channel_map.get(&"#test".to_string()) {
                Some(channel_id) => *channel_id,
                None => panic!("channel `#test` not found.")
            };
            chan_test_id_hist.push(channel_id);

            let channel_state = match state.channels.get(&channel_id) {
                Some(channel) => channel.clone(),
                None => panic!("channel `#test` had Id but no state")
            };
            assert_eq!(channel_state.users.len(), 7);
        });

        it("topic of `#test` should be `irc is bad.`", |state| {
            let msg = format!("state.identify_channel failed on line {}", 1 + line!());
            let chan_id = state.identify_channel("#test").expect(msg.as_slice());
            let msg = format!("state.channels.find failed on line {}", 1 + line!());
            let channel = state.channels.get(&chan_id).expect(msg.as_slice());
            assert_eq!(channel.topic.as_slice(), "irc is bad.");
        });

        it("should have a user `randomuser` after JOIN", |state| {
            let msg = format!("state.identify_nick failed on line {}", 1 + line!());
            let randomuser_id = state.identify_nick("randomuser").expect(msg.as_slice());
            if random_user_id_hist.contains(&randomuser_id) {
                assert!(false, "nick `randomuser` UserId must change between losses in view");
            }
            random_user_id_hist.push(randomuser_id);
            match state.users.get(&randomuser_id) {
                Some(randomuser) => {
                    assert_eq!(
                        randomuser.prefix.as_slice(),
                        "randomuser!rustbot@coolhost");
                },
                None => panic!("inconsistent state. state = {}", state)
            }
        });

        it("should not have a user `randomuser` after PART", |state| {
            assert_eq!(state.identify_nick("randomuser"), None);
        });

        it("should not have a user `randomuser` after KICK", |state| {
            assert_eq!(state.identify_nick("randomuser"), None);
        });

        it("should not have a user `randomuser` after QUIT", |state| {
            assert_eq!(state.identify_nick("randomuser"), None);
        });

        it("topic of `#test` should be `setting a cool topic`", |state| {
            let msg = format!("state.identify_channel failed on line {}", 1 + line!());
            let chan_id = state.identify_channel("#test").expect(msg.as_slice());
            let msg = format!("state.channels.find failed on line {}", 1 + line!());
            let channel = state.channels.get(&chan_id).expect(msg.as_slice());
            assert_eq!(channel.topic.as_slice(), "setting a cool topic");
        });

        let mut randomuser_id: Option<UserId> = None;
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
                assert!(false, "channel `#test` ChannelId must change between losses in view");
            }
            chan_test_id_hist.push(test_id);
        });

        let mut randomuser_id: Option<UserId> = None;

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

        it("randomuser should have been forgotten", |state| {
            assert_eq!(state.identify_nick("randomuser"), None);
        });

        it("randomuser should not have the same ID as before", |state| {
            assert!(state.identify_channel("#test2").is_some());
            if state.identify_nick("randomuser") == randomuser_id {
                assert!(false, "randomuser should be different now");
            }
        });
    }
}
