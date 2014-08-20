use connection::{
    RustBotPluginApi,
    RustBotPlugin,
};
use message::{
    IrcMessage
};


struct GreedState {
    channel: String,
    last_played: Option<(String, Vec<u16>, u16)>
}


pub struct GreedPlugin {
    bot: RustBotPluginApi,
    states: Vec<GreedState>
}


fn is_channel(message: &IrcMessage) -> bool {
    false
}

fn get_channel(message: &IrcMessage) -> String {
    String::from_str("")
}

fn is_command_greed(message: &IrcMessage) -> bool {
    if message.get_command().as_slice() == "PRIVMSG" {
        let args = message.get_args();
        if args.len() > 0 {
            args[0].as_slice() == "!greed"
        } else {
            false
        }
    } else {
        false
    }
}

fn find_or_create_state(states: &mut Vec<GreedState>, channel: String) -> &mut GreedState {
    let channel_slice = channel.as_slice();
    
    let mut want_idx = None;
    for (i, state) in states.iter().enumerate() {
        if state.channel.as_slice() == channel_slice {
            want_idx = Some(i);
        }
    }
    match want_idx {
        Some(idx) => {
            states.get_mut(idx)
        },
        None => {
            states.push(GreedState {
                channel: channel.clone(),
                last_played: None
            });
            states.mut_last().unwrap()
        }
    }
}


impl GreedPlugin {
    pub fn new(botapi: RustBotPluginApi) -> GreedPlugin {
        GreedPlugin {
            bot: botapi,
            states: Vec::new()
        }
    }
}


impl RustBotPlugin for GreedPlugin {
    fn accept(&mut self, message: &IrcMessage) {
        if is_command_greed(message) && is_channel(message) {
            let channel = get_channel(message);

            let source_nick = match message.source_nick() {
                Some(source_nick) => source_nick,
                None => return
            };

            let mut state = find_or_create_state(&mut self.states, channel);

            // roll and store with cur_state

        }
    }
}
