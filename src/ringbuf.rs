use std::convert::From;
use mio::buf::{self, RingBuf, MutBuf, Buf};
use irc::parse::{IrcMsg, ParseError};

pub enum PushError {
    Full,
}

const MSG_MAX_LEN: usize = 1 << 14;

#[derive(Debug)]
pub enum ProtocolError {
    /// Message is too long.
    TooLong,
}

pub enum PopError {
    /// More data is needed to pop an IrcMsg
    MoreData,

    /// The protocol broke an invariant.  The connection is now
    /// in an unknown state and must be discarded.
    ProtocolError(ProtocolError),

    /// Failed to parse an IrcMsg
    Parse(ParseError),
}

impl From<ParseError> for PopError {
    fn from(e: ParseError) -> PopError {
        PopError::Parse(e)
    }
}

pub struct IrcMsgReceiver(RingBuf);

impl IrcMsgReceiver {
    pub fn new(capacity: usize) -> IrcMsgReceiver {
        IrcMsgReceiver(RingBuf::new(capacity))
    }

    pub fn pop_msg(&mut self) -> Result<IrcMsg, PopError> {
        self.0.mark();

        // Preallocation counting phase
        let mut newline_idx = 0;
        let mut found_newline = false;

        while let Some(byte) = self.0.read_byte() {
            newline_idx += 1;
            if byte == b'\n' {
                found_newline = true;
                break;
            }
        }
        if !found_newline {
            if MSG_MAX_LEN < newline_idx {
                return Err(PopError::ProtocolError(ProtocolError::TooLong));
            }
            return Err(PopError::MoreData);
        }

        self.0.reset();

        let mut output = Vec::with_capacity(newline_idx);
        while let Some(byte) = self.0.read_byte() {
            output.push(byte);
            if byte == b'\n' {
                break;
            }
        }

        Ok(try!(IrcMsg::new(output)))
    }
}

impl MutBuf for IrcMsgReceiver {
    fn remaining(&self) -> usize {
        MutBuf::remaining(&self.0)
    }

    fn advance(&mut self, cnt: usize) {
        MutBuf::advance(&mut self.0, cnt)
    }

    fn mut_bytes(&mut self) -> &mut [u8] {
        MutBuf::mut_bytes(&mut self.0)
    }
}

// ------ //

pub struct IrcMsgSender(RingBuf);

impl IrcMsgSender {
    pub fn new(capacity: usize) -> IrcMsgSender {
        IrcMsgSender(RingBuf::new(capacity))
    }

    pub fn push_msg(&mut self, msg: &IrcMsg) -> Result<usize, PushError> {
        let msg_bytes = msg.as_bytes();
        if MutBuf::remaining(&self.0) < 2 + msg_bytes.len() {
            return Err(PushError::Full);
        }

        let mut acc = 0;
        acc += self.0.write_slice(msg_bytes);
        acc += self.0.write_slice(b"\r\n");
        Ok(acc)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Buf for IrcMsgSender {
    fn remaining(&self) -> usize {
        Buf::remaining(&self.0)
    }

    fn bytes(&self) -> &[u8] {
        Buf::bytes(&self.0)
    }

    fn advance(&mut self, cnt: usize) {
        Buf::advance(&mut self.0, cnt)
    }
}