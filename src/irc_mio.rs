use std::convert::From;
use bytes::{RingBuf, MutBuf, Buf};
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

#[derive(Debug)]
pub enum PopError {
    /// More data is needed to pop an IrcMsg
    MoreData,

    /// The protocol broke an invariant.  The connection is now
    /// in an unknown state and should be discarded.
    ProtocolError(ProtocolError),

    /// Failed to parse an IrcMsg
    Parse(ParseError),
}

impl From<ParseError> for PopError {
    fn from(e: ParseError) -> PopError {
        PopError::Parse(e)
    }
}

pub struct IrcMsgRingBuf(RingBuf);

impl IrcMsgRingBuf {
    pub fn new(capacity: usize) -> IrcMsgRingBuf {
        IrcMsgRingBuf(RingBuf::new(capacity))
    }

    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn mark(&mut self) {
        self.0.mark();
    }

    pub fn reset(&mut self) {
        self.0.reset();
    }

    pub fn bytes(&self) -> &[u8] {
        self.0.bytes()
    }

    pub fn pop_msg(&mut self) -> Result<IrcMsg, PopError> {
        // Preallocation counting phase
        let mut newline_idx = 0;
        let mut found_newline = false;

        self.mark();
        while let Some(byte) = self.0.read_byte() {
            newline_idx += 1;
            if byte == b'\n' {
                found_newline = true;
                break;
            }
        }

        // Discards data, no self.reset()
        if !found_newline {
            if MSG_MAX_LEN < newline_idx {
                return Err(PopError::ProtocolError(ProtocolError::TooLong));
            }
        }

        self.reset();
        if found_newline {
            let mut output = Vec::with_capacity(newline_idx);
            while let Some(byte) = self.0.read_byte() {
                output.push(byte);
                if byte == b'\n' {
                    break;
                }
            }
            Ok(try!(IrcMsg::new(output)))
        } else {
            Err(PopError::MoreData)
        }
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
}

impl Buf for IrcMsgRingBuf {
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


impl MutBuf for IrcMsgRingBuf {
    fn remaining(&self) -> usize {
        MutBuf::remaining(&self.0)
    }

    unsafe fn advance(&mut self, cnt: usize) {
        MutBuf::advance(&mut self.0, cnt)
    }

    unsafe fn mut_bytes(&mut self) -> &mut [u8] {
        MutBuf::mut_bytes(&mut self.0)
    }
}
