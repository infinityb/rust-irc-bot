use rand;

use irc::{IrcMsg, client as cli2, server as ser2};

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
        if let Ok(privmsg) = msg.as_tymsg::<&ser2::Privmsg>() {
            if privmsg.get_target().starts_with(b"#") && rand::random::<f64>() < 0.0003 {
                let mut out = privmsg.get_body_raw().to_vec();
                apply_bitflips(&mut out[..]);
                if privmsg.get_body_raw() != &out[..] {
                    let response = cli2::PrivmsgBuf::new(privmsg.get_target(), &out).unwrap();
                    replier.reply(&response).unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_bitflips;

    #[test]
    fn test_bitflip() {
        let mut networking = [0; 13];
        for (i, o) in b"   networking".iter().zip(networking.iter_mut()) {
            *o = *i;
        }
        apply_bitflips(&mut networking);
        assert_eq!(&networking, b"   fetwgrkifg");
    }
}
