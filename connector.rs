#![feature(struct_variant)] 
extern crate debug;


use std::comm::channel;
use std::io::{TcpStream, IoResult, LineBufferedWriter, BufferedReader};

use irc::IrcMessage;

mod irc;


struct IrcConnection {
    conn: TcpStream,
    writer: LineBufferedWriter<TcpStream>
    // state tracking
}

impl IrcConnection {
    fn connect(host: &str, port: u16) -> IoResult<IrcConnection> {
        let stream = match TcpStream::connect(host, port) {
            Ok(stream) => stream,
            Err(err) => return Err(err)
        };
        Ok(IrcConnection {
            conn: stream.clone(),
            writer: LineBufferedWriter::new(stream.clone())
        })
    }

    fn join(&mut self, channel: &str) -> () {
        match self.writer.write_str(format!("JOIN {}\n", channel).as_slice()) {
            Ok(_) => (),
            Err(err) => fail!("Error writing to IRC server: {}", err)
        }
        // consume the join noise
    }
}

fn spawn_reader_thread(reader: BufferedReader<TcpStream>) -> Receiver<IrcMessage> {
    let (tx, rx) = sync_channel(0);
    spawn(proc() {
        let mut reader = reader;
        loop {
            let string = match reader.read_line() {
                Ok(string) => string,
                Err(err) => fail!("{}", err)
            };
            match IrcMessage::from_str(string.as_slice().trim_right()) {
                Ok(message) => {
                    tx.send(message);
                }
                Err(err) => {
                    println!("Invalid IRC message: {} for {}", err, string);
                }
            };
        }
    });
    rx
}

fn main() {
    let mut conn = match IrcConnection::connect("127.0.0.1", 6667) {
        Ok(stream) => stream,
        Err(err) => fail!("{}", err)
    };
    let rx = spawn_reader_thread(BufferedReader::new(conn.conn.clone()));
    
    match conn.writer.write_str("NICK nick\n") {
        Ok(_) => (),
        Err(err) => fail!("Error writing to IRC server: {}", err)
    };
    match conn.writer.write_str("USER paul 8 *: Paul Mutton\n") {
        Ok(_) => (),
        Err(err) => fail!("Error writing to IRC server: {}", err)
    };

    loop {
        let message: IrcMessage = rx.recv();
        println!("{}", message);
        
        if message.get_command() == "001" {
            println!("CONNECTED");
            conn.join("#dicks");
        }
        if message.get_command() == "PING" {
            println!("PING!");
            // writer.write_str("PONG :
        }
    }
}
