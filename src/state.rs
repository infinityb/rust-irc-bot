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

	IrcEvent,
	IrcEventMessage,
	IrcEventJoinBundle,
	IrcEventWhoBundle,
};

pub enum XXBotUserId {
	Known(BotUserId),
	Anonymous
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
	user_map: HashMap<String, BotChannelId>,
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

	fn on_self_part(&mut self, msg: &IrcMessage) {
		let channel_name = msg.get_args()[0].to_string();
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
		//
	}

	fn on_message(&mut self, msg: &IrcMessage) {
		// :rustbot!rustbot@out-ab-133.wireless.telus.com PART #sample
		if msg.command() == "PART" {
			if msg.source_nick() == Some(self.botnick.as_slice()) {
				return self.on_self_part(msg);
			} else {
				return self.on_other_part(msg);
			}
		}
		println!("state = {}", self);
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

	fn on_join(&mut self, join_res: &JoinResult) {
		let join = match *join_res {
			Ok(ref ok) => ok,
			Err(_) => return
		};
		if let Some(_) = self.channel_map.find(&join.channel) {
			warn!("We know about {} before joining it?", join.channel);
		}
		let channel_id = self.__find_channel_id(join.channel.as_slice());
		self.channels.insert(channel_id, InternalChannel {
			id: channel_id,
			name: join.channel.to_string(),
			topic: String::new(),
			users: HashSet::new(),
		});
	}

	fn on_who(&mut self, who_res: &WhoResult) {
		let who = match *who_res {
			Ok(ref ok) => ok,
			Err(_) => return
		};
	}

	pub fn on_event(&mut self, event: &IrcEvent) {
		match *event {
			IrcEventMessage(ref message) => self.on_message(message),
			IrcEventJoinBundle(ref join_bun) => self.on_join(join_bun),
			IrcEventWhoBundle(ref who_bun) => self.on_who(who_bun),
		}
	}
}
