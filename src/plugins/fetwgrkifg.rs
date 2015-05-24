use std::sync::mpsc::SyncSender;

use rand;

use irc::parse::IrcMsg;
use irc::message_types::{server, client};

use command_mapper::{Replier, RustBotPlugin};

pub struct FetwgrkifgPlugin;

impl FetwgrkifgPlugin {
    pub fn new() -> FetwgrkifgPlugin {
        FetwgrkifgPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "fetwgrkifg"
    }
}

fn apply_bitflips(input: &mut [u8]) {
    let and_mask = [0xFF, 0xFF, 0xFF, 0xF7];
    let and_cycle = and_mask.iter().cloned().cycle();
    for (flip, input_byte) in Iterator::zip(and_cycle, input.iter_mut()) {
        *input_byte &= flip;
    }
}

impl RustBotPlugin for FetwgrkifgPlugin {
    fn on_message(&mut self, replier: &mut Replier, msg: &IrcMsg) {
        if let server::IncomingMsg::Privmsg(ref privmsg) = server::IncomingMsg::from_msg(msg.clone()) {
            if privmsg.get_target().starts_with("#") && rand::random::<f64>() < 0.006 {
                let mut out = privmsg.get_body_raw().to_vec();
                apply_bitflips(&mut out[..]);
                if privmsg.get_body_raw() != &out[..] {
                    let _ = replier.reply(client::Privmsg::new(privmsg.get_target(), &out[..]).into_irc_msg());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_bitflips;
    use std::slice::bytes::copy_memory;

    #[test]
    fn test_bitflip() {
        let mut networking = [0; 13];
        copy_memory(&mut networking, b"   networking");
        apply_bitflips(&mut networking);
        assert_eq!(networking, b"   fetwgrkifg");
    }
}
