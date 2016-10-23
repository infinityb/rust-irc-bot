use std::fmt;
use std::error::Error as StdError;

use hyper;

pub mod util;

pub struct FourchanApi {
    client: hyper::Client,
}

impl FourchanApi {
    pub fn with_client(client: hyper::Client) -> Self {
        FourchanApi {
            client: client,
        }
    }

    pub fn execute<E>(&self, command: &E) -> Result<E::Value, ApiError>
        where E: ApiCommand
    {
        use hyper::header::Connection;

        let url = command.hyper_url();
        let req = self.client.request(command.hyper_method(), &url);
        let req = req.header(Connection::close());

        let resp = try!(req.send().map_err(ApiError::Hyper));
        command.process_result(resp)
    }
}

#[derive(Debug)]
pub enum ApiError {
    Hyper(hyper::Error),
    Unknown(Box<StdError>),
    NotFound,
}

#[derive(Debug)]
pub struct ImageLocator {
    board: String,
    image_name: String,
}

#[derive(Debug)]
pub struct ImageNameSearch(pub ImageLocator);

pub trait ApiCommand {
    type Value;

    fn hyper_method(&self) -> hyper::method::Method;
    fn hyper_url(&self) -> String;
    fn process_result(&self, result: hyper::client::Response) -> Result<Self::Value, ApiError>;
}

impl ApiCommand for ImageNameSearch {
    type Value = ThreadLocator;

    fn hyper_method(&self) -> hyper::method::Method {
        hyper::method::Method::Head
    }

    fn hyper_url(&self) -> String {
        format!("https://4chan-api.yshi.org/board/{}/find-thread?image={}", self.0.board, self.0.image_name)
    }

    fn process_result(&self, result: hyper::client::Response) -> Result<ThreadLocator, ApiError> {
        use hyper::header::Location;

        // let _ = io::copy(&mut result, &mut ::std::io::sink());
        let location = try!(result.headers.get::<Location>().ok_or(ApiError::NotFound));

        ThreadLocator::parse_from_location(&location.0)
    }
}

pub struct ThreadLocator {
    board: String,
    thread_no: i64,
}

impl fmt::Display for ThreadLocator {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "https://boards.4chan.org/{}/thread/{}", self.board, self.thread_no)
    }
}

impl ThreadLocator {
    pub fn parse_from_location(url_part: &str) -> Result<Self, ApiError> {
        if !url_part.starts_with("/board/") {
            return Err(ApiError::Unknown(Box::new(StrangeResponse)));
        }

        let mut part_iter = url_part[7..].splitn(3, '/');

        let board: String;
        let thread_no: i64;

        match part_iter.next() {
            Some(b) => board = b.to_string(),
            None => return Err(ApiError::Unknown(Box::new(StrangeResponse))),
        }
        match part_iter.next() {
            Some(thread) if thread == "thread" => (),
            _ => return Err(ApiError::Unknown(Box::new(StrangeResponse))),
        }
        match part_iter.next() {
            Some(t) => thread_no = try!(t.parse().map_err(|_| {
                ApiError::Unknown(Box::new(StrangeResponse))
            })),
            None => return Err(ApiError::Unknown(Box::new(StrangeResponse))),
        }

        Ok(ThreadLocator {
            board: board,
            thread_no: thread_no,
        })
    }
}

impl ImageLocator {
    pub fn parse_fourchan_url(url: &str) -> Result<Self, Box<StdError>> {
        #[derive(Debug)]
        struct ParseError;

        impl fmt::Display for ParseError {
            fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
                write!(f, "ParseError")
            }
        }

        impl StdError for ParseError {
            fn description(&self) -> &str {
                "parse error"
            }
        }

        let noschema;
        match util::strip_schema(url) {
            Some(ns) => noschema = ns,
            None => return Err(Box::new(ParseError)),
        }
        
        // noschema now looks like "i.4cdn.org/a/1477189421419.webm"
        let board: String;
        let image_name: String;

        let mut part_iter = noschema.splitn(3, '/');
        match part_iter.next() {
            Some(ref h) if util::is_image_host(*h) => (),
            _ => return Err(Box::new(ParseError)),
        }
        match part_iter.next() {
            Some(b) => board = b.to_string(),
            _ => return Err(Box::new(ParseError)),
        }
        match part_iter.next() {
            Some(n) => image_name = n.to_string(),
            _ => return Err(Box::new(ParseError)),
        }

        Ok(ImageLocator {
            board: board,
            image_name: image_name,
        })
    }
}

#[derive(Debug)]
pub struct StrangeResponse;

impl fmt::Display for StrangeResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "StrangeResponse: unhandled response from server")
    }
}

impl StdError for StrangeResponse {
    fn description(&self) -> &str {
        "StrangeResponse: unhandled response from server"
    }
}
