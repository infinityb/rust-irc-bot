use std::collections::{TreeMap};
use message::{
    IrcMessage,
    IrcHostmaskPrefix,
    IrcOtherPrefix,
};

pub struct IrcUser<'a> {
    nick: String,
    username: String,
    hostname: String,
}


pub struct IrcChannel<'a> {
    name: String,
    users: Vec<String>
}


pub struct IrcState<'a> {
    channels: TreeMap<String, IrcChannel<'a>>,
    users: TreeMap<String, IrcUser<'a>>
}


impl<'a> IrcState<'a> {
    pub fn new<'a>() -> IrcState<'a> {
        IrcState {
            channels: TreeMap::new(),
            users: TreeMap::new()
        }
    }

    pub fn update(&mut self, message: &IrcMessage) {
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

    pub fn add_channel(&mut self, channel: IrcChannel<'a>) {
        self.channels.insert(channel.name.clone(), channel);
    }
}


#[test]
fn test_insert_channel() {
    let mut state: IrcState = IrcState::new();
    state.add_channel(IrcChannel {
        name: String::from_str("#coolchannel"),
        users: vec!["aibi", "faux", "aers", "owly"].move_iter()
                .map(|s| String::from_str(s)).collect()
    });
}
