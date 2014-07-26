#![feature(struct_variant)] 

extern crate debug;

use std::comm::channel;
use std::io::{stdout};
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

struct IrcProcessingEngine;

struct IrcConnection {
    conn: TcpStream,
    // state tracking
}

impl IrcConnection {
    fn new() -> IoResult<IrcConnection> {
        match TcpStream::connect("127.0.0.1", 6667) {
            Ok(stream) => Ok(IrcConnection { conn: stream }),
            Err(err) => Err(err)
        }
    }
}

enum IrcProtocolMessage<'a> {
    Ping { data: Box<&'a str> },
    Pong { data: Box<&'a str> },
    Notice { data: Box<&'a str> },
    IrcNumeric { num: int, data: Box<&'a str> },
    Unknown { name: Box<&'a str>, data: Box<&'a str> }
}

fn parse_irc_numeric<'a>(command: &str, data: &'a str) -> IrcProtocolMessage<'a> {
    IrcNumeric { num: 0, data: box data }
}

fn is_numeric(command: &str) -> bool {
    return false;
}

fn reader_parser(str: &String) -> () {  // IrcProtocolMessage {
    let parts: Vec<&str> = str.as_slice().splitn(' ', 2).collect();
    if (parts.len() != 3) {
        fail!("Got a totally weird line: {}", str);
    }
    let (server, command, rest) = (parts[0], parts[1], parts[2]);
    let command_parsed = match command {
        "PING" => Ping { data: box rest },
        "PONG" => Pong { data: box rest },
        "NOTICE" => Notice { data: box rest },
        _ => {
            if is_numeric(command) {
                parse_irc_numeric(command, rest)
            } else {
                Unknown { name: box command, data: box rest }
            }
        }
    };

    println!("{:?} {:?}", server, command_parsed);
}

fn spawn_reader_thread(reader: BufferedReader<TcpStream>) -> Receiver<IrcProtocolMessage> {
    let (tx, rx) = sync_channel(0);
    spawn(proc() {
        let mut reader = reader;
        loop {
            match reader.read_line() {
                Ok(string) => {
                    tx.send(reader_parser(&string));
                }
                Err(err) => fail!("{}", err)
            }
        }
    });
    rx
}

fn main() {
    let mut conn = match TcpStream::connect("127.0.0.1", 6667) {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    let mut stdout = stdout();  
    let rx = spawn_reader_thread(BufferedReader::new(conn.clone()));
    
    let mut writer = LineBufferedWriter::new(conn.clone());
    writer.write_str("NICK nick\n");
    writer.write_str("USER paul 8 *: Paul Mutton\n");


    loop {
        stdout.write_str(format!("{}", rx.recv()));
    }
    println!("{:?}", conn);
}


