#[cfg(test)]
mod tests {
    use state::{State, BotUserId};
    use std::io::{IoResult, BufReader};
    use irc::{
        BundlerManager,
        IrcMessage,
        JoinBundlerTrigger,
        WhoBundlerTrigger,
    };

    const TEST_SESSION_STATETRACKER: &'static [u8] =
    	include_bin!("../testdata/news_transcript.txt");

    #[derive(Show)]
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

        it("swagever", |state| {
        	println!("ok");
        });
    }
}