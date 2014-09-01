use std::collections::{TreeMap, RingBuf, Deque};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch
};
use message::{
    IrcMessage,
    IrcProtocolMessage,
    IrcHostmaskPrefix,
    IrcOtherPrefix,
};


struct WhoRecord {
    hostname: String,
    server: String,
    nick: String,
    rest: String
}


impl WhoRecord {
    fn new(args: &Vec<String>) -> Option<WhoRecord> {
        match args.as_slice() {
            [ref _self_nick, ref _channel, ref hostname,
             ref server, ref nick, ref _unk, ref rest
            ] => {
                Some(WhoRecord {
                    hostname: hostname.clone(),
                    server: server.clone(),
                    nick: nick.clone(),
                    rest: rest.clone()
                })
            },
            _ => None
        }
    }
}

struct WhoBundler {
    target_channel: String,
    who_records: Vec<WhoRecord>
}


impl WhoBundler {
    fn add_record(&mut self, args: &Vec<String>) {
        match WhoRecord::new(args) {
            Some(who_rec) => {
                self.who_records.push(who_rec);
            },
            None => ()
        }
    }

    fn accept(&mut self, message: &IrcMessage) -> bool {
        if message.get_args().len() < 2 {
            return false;
        }
        if message.get_arg(1).as_slice() != self.target_channel.as_slice() {
            return false;
        }
        match *message.get_message() {
            IrcProtocolMessage::Numeric(352, ref message) => {
                self.add_record(message);
                true
            },
            IrcProtocolMessage::Numeric(315, ref message) => {
                // finished
                false
            },
            _ => true
        }
    }
}


pub struct IrcUser {
    nick: String,
    username: String,
    hostname: String,
}


pub struct IrcChannel {
    name: String,
    users: Vec<IrcUser>
}


pub struct IrcStatePlugin {
    channels: TreeMap<String, IrcChannel>,
    users: TreeMap<String, IrcUser>,
    who_bundlers: RingBuf<WhoBundler>
}


impl IrcStatePlugin {
    pub fn new() -> IrcStatePlugin {
        IrcStatePlugin {
            channels: TreeMap::new(),
            users: TreeMap::new(),
            who_bundlers: RingBuf::new()
        }
    }

    fn update(&mut self, message: &IrcMessage) {
        match message.get_prefix() {
            Some(&IrcHostmaskPrefix(ref mask)) => {
                println!("{}", mask);
            },
            Some(&IrcOtherPrefix(ref other)) => {
                println!("{}", other);
            },
            None => ()
        }
    }

    fn add_channel(&mut self, channel: IrcChannel) {
        self.channels.insert(channel.name.clone(), channel);
    }
}

impl RustBotPlugin for IrcStatePlugin {
    fn accept(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        // If we find a JOIN message:
        //   * Attach a WhoBundler to our listener buffer
        //   * send a WHO to that channel
        //   * on completion of the WhoBundler, update states.
    }
}
