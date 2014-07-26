extern crate debug;

use std::string::{String};
use std::fmt;


pub struct IrcMessage {
    prefix: Option<String>,
    command: String,
    args: Vec<String>,
}

fn parse_message_args(text: &str) -> Result<Vec<String>, ()> {
    if text.len() == 0 {
        return Err(())
    }
    if text.char_at(0) == ':' {
        return Ok(vec![String::from_str(text.slice_from(1))]);
    }
    let (arg_parts, rest) = match text.find_str(" :") {
        Some(val) => {
            (text.slice_to(val), Some(text.slice_from(val + 2)))
        },
        None => {
            (text, None)
        }
    };
    let mut output: Vec<String> = arg_parts.split(' ')
            .map(|s| String::from_str(s)).collect();
    match rest {
        Some(val) => output.push(String::from_str(val)),
        None => ()
    };
    Ok(output)
}

fn parse_message_rest(text: &str) -> Result<(String, Vec<String>), ()> {
    let parts: Vec<&str> = text.splitn(' ', 1).collect();
    let args = match parse_message_args(parts[1]) {
        Ok(args) => args,
        Err(err) => return Err(err)
    };
    Ok((String::from_str(parts[0]), args))
}

impl IrcMessage {
    pub fn from_str(text: &str) -> Result<IrcMessage, ()> {
        if text.len() == 0 {
            return Err(());
        }
        let (prefix, command, args) = if text.char_at(0) == ':' {
                let parts: Vec<&str> = text.splitn(' ', 1).collect();
                if parts.len() < 2 {
                    return Err(());
                }
                let (command, args) = match parse_message_rest(parts[1]) {
                    Ok(result) => result,
                    Err(err) => return Err(err)
                };

                (Some(String::from_str(parts[0].slice_from(1))), command, args)
            } else {
                assert!(text.len() > 0);
                let (command, args) = match parse_message_rest(text) {
                    Ok(result) => result,
                    Err(err) => return Err(err)
                };
                (None, command, args)
            };
        Ok(IrcMessage { prefix: prefix, command: command, args: args })
    }

    pub fn get_command<'a>(&'a self) -> &'a str {
        self.command.as_slice()
    }
}


impl fmt::Show for IrcMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut arg_string = String::new();
        arg_string = arg_string.append("[");
        for part in self.args.iter().map(|s| s.as_slice()) {
            arg_string = arg_string.append(format!("{:?}, ", part).as_slice());
        }
        arg_string = arg_string.append("]");

        match self.prefix {
            Some(ref prefix) => write!(f, "IrcMessage({:?}, {:?}, {})",
                prefix.as_slice(), self.command.as_slice(), arg_string.as_slice()),
            None => write!(f, "IrcMessage({:?}, {})",
                self.command.as_slice(), arg_string.as_slice())
        }
    }
}


#[test]
fn test_irc_message() {
    match IrcMessage::from_str("") {
        Ok(_) => {
            fail!("empty string is invalid")
        },
        Err(_) => ()
    };

    match IrcMessage::from_str(":") {
        Ok(_) => {
            fail!("empty string is invalid")
        },
        Err(_) => ()
    };

    match IrcMessage::from_str(" ") {
        Ok(_) => {
            fail!("empty string is invalid")
        },
        Err(_) => ()
    };

    match IrcMessage::from_str("PING server1") {
        Ok(message) => {
            assert_eq!(message.prefix, None);
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 1);
        },
        Err(_) => fail!("failed to parse")
    };


    match IrcMessage::from_str("PING server1 server2") {
        Ok(message) => {
            assert_eq!(message.prefix, None);
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 2);
        },
        Err(_) => fail!("failed to parse")
    };

    match IrcMessage::from_str(":somewhere PING server1") {
        Ok(message) => {
            assert_eq!(message.prefix, Some(String::from_str("somewhere")));
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 1);
        },
        Err(_) => fail!("failed to parse")
    };
    
    match IrcMessage::from_str(":somewhere PING server1 server2") {
        Ok(message) => {
            assert_eq!(message.prefix, Some(String::from_str("somewhere")));
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 2);
            assert_eq!(message.args[0].as_slice(), "server1");
            assert_eq!(message.args[1].as_slice(), "server2");
        },
        Err(_) => fail!("failed to parse")
    };

    match IrcMessage::from_str(":somewhere PING server1 :server2") {
        Ok(message) => {
            assert_eq!(message.prefix, Some(String::from_str("somewhere")));
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 2);
            assert_eq!(message.args[0].as_slice(), "server1");
            assert_eq!(message.args[1].as_slice(), "server2");
        },
        Err(_) => fail!("failed to parse")
    };

    match IrcMessage::from_str(":somewhere PING :server1 server2") {
        Ok(message) => {
            assert_eq!(message.prefix, Some(String::from_str("somewhere")));
            assert_eq!(message.command.as_slice(), "PING");
            assert_eq!(message.args.len(), 1);
            assert_eq!(message.args[0].as_slice(), "server1 server2");
        },
        Err(_) => fail!("failed to parse")
    };
}

#[allow(dead_code)]
enum IrcProtocolMessage {
    Ping(String, Option<String>),
    Pong(String),
    Notice(String),
    IrcNumeric(int, String),
    Unknown(String, String)
}
