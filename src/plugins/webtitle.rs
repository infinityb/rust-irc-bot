use std::io;
use std::time::Duration;
use url::Url;
use hyper;
use hyper::header::{qitem, ContentType, ContentEncoding, AcceptEncoding, Encoding, UserAgent};
use hyper::mime::{Mime, TopLevel, SubLevel};
use html5ever::parse_document;
use html5ever::rcdom::{Text, Element, RcDom, Handle};
use html5ever::tendril::TendrilSink;
use irc::{server, client, IrcMsg};

use ::utils::SlowReadProtect;

const REQUEST_HEAD_SIZE: u64 = 1 * 1024 * 1024;

use command_mapper::{
    RustBotPlugin,
    Replier,
};

pub struct WebTitlePlugin;

impl WebTitlePlugin {
    pub fn new() -> WebTitlePlugin {
        WebTitlePlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "webtitle"
    }
}

impl RustBotPlugin for WebTitlePlugin {
    fn on_message(&mut self, m: &mut Replier, msg: &IrcMsg) {
        use std::str;
        // use hyper::client::RedirectPolicy::FollowNone;

        let privmsg;
        let body;
        if let Ok(privmsg_tmp) = msg.as_tymsg::<&server::Privmsg>() {
            privmsg = privmsg_tmp;
            if let Ok(body_tmp) = str::from_utf8(privmsg.get_body_raw()) {
                body = body_tmp;
            } else {
                return;
            }
        } else {
            return;
        }
        let reply_target = str::from_utf8(privmsg.get_target()).unwrap();
        let urls = body
            .split(char::is_whitespace)
            .filter(is_valid_url)
            .collect::<Vec<&str>>();

        for url in urls.iter() {
            match get_title(url) {
                Ok(title) => {
                    let mut message = String::from("\x0f");
                    message.extend(title.chars().take(200));
                    let _ = m.reply(&client::PrivmsgBuf::new(
                        reply_target.as_bytes(),
                        message.as_bytes(),
                    ).unwrap());
                }
                Err(ecode) => println!("error: {}", ecode),
            }
        }
    }
}


fn is_valid_url(url_candidate: &&str) -> bool {
    Url::parse(*url_candidate).is_ok()
}

const USER_AGENT: &'static str = "Mozilla/5.0 (X11; Linux; rv:37.0) Servo/1.0 Firefox/37.0";
const USE_HEAD: bool = false;

fn get_title(url: &str) -> Result<String, String> {
    use std::io::{self, Read};

    let mut client = hyper::Client::new();
    client.set_read_timeout(Some(Duration::new(5, 0)));
    client.set_write_timeout(Some(Duration::new(5, 0)));
    
    if USE_HEAD {
        let req = client.head(url)
            .header(UserAgent(USER_AGENT.into()))
            .header(accept_html())
            .header(AcceptEncoding(vec![
                qitem(Encoding::Gzip),
                qitem(Encoding::Deflate),
                qitem(Encoding::Identity),
            ]));

        let resp = try!(req.send().map_err(|e| format!("HEAD req send: {}", e)));
        if !is_html(&resp) && !absurd_webserver(&resp) {
            return Err("HEAD: not text/html or not acceptable".into());
        }
        drop(resp);
    }

    let req = client.get(url)
        .header(UserAgent(USER_AGENT.into()))
        .header(accept_html())
        .header(AcceptEncoding(vec![
            qitem(Encoding::Gzip),
            qitem(Encoding::Deflate),
            qitem(Encoding::Identity),
        ]));

    let resp = try!(req.send().map_err(|e| format!("GET req send: {}", e)));
    if !is_html(&resp) {
        return Err("GET: not text/html or not acceptable".into());
    }

    let mut buf = Vec::with_capacity(20 * 1024);
    let reader: Box<io::Read> = try!(read_body(resp));
    let mut reader = SlowReadProtect::new(reader, 120_000);

    try!(reader.read_to_end(&mut buf).map_err(|e| format!("GET read error: {}", e)));

    let dom = try!(parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut io::Cursor::new(&buf[..]))
        .map_err(|e| format!("html5ever read: {}", e)));

    return walk_title(dom.document).ok_or("bad walk".into());

    fn accept_html() -> hyper::header::Accept {
        use hyper::header::Accept;
        
        Accept(vec![
            qitem(Mime(TopLevel::Text, SubLevel::Html, vec![])),
        ])
    }
}


// Amazon doesn't allow HEAD on their pages
fn absurd_webserver(resp: &hyper::client::Response) -> bool {
    use hyper::status::StatusCode;

    resp.status == StatusCode::MethodNotAllowed
}

fn is_html(resp: &hyper::client::Response) -> bool {
    if !resp.status.is_success() {
        return false;
    }
    return resp.headers.get::<ContentType>()
        .map(ctype_is_html)
        .unwrap_or(false);

    fn ctype_is_html(ctype: &ContentType) -> bool {
        let ContentType(Mime(ref tl, ref sl, _)) = *ctype;
        *tl == TopLevel::Text && *sl == SubLevel::Html
    }
}

fn read_body(resp: hyper::client::Response) -> Result<Box<io::Read>, String> {
    use std::io::Read;
    use flate2::read::{DeflateDecoder, GzDecoder};
    enum _Encoding {
        Gzip,
        Deflate,
        Identity,
    }

    let encoding = resp.headers.get::<ContentEncoding>()
        .map(|x| match x.0.get(0) {
            Some(&Encoding::Gzip) => Some(_Encoding::Gzip),
            Some(&Encoding::Deflate) => Some(_Encoding::Deflate),
            Some(&Encoding::Identity) => Some(_Encoding::Identity),
            _ => None,
        })
        .unwrap_or(Some(_Encoding::Identity));

    match encoding {
        Some(_Encoding::Gzip) => {
            let rdr: GzDecoder<_> = try!(GzDecoder::new(resp)
                .map_err(|e| format!("gz decode: {}", e)));
            Ok(Box::new(rdr.take(REQUEST_HEAD_SIZE)))
        },
        Some(_Encoding::Deflate) => {
            let rdr: DeflateDecoder<_> = DeflateDecoder::new(resp);
            Ok(Box::new(rdr.take(REQUEST_HEAD_SIZE)))
        },
        Some(_Encoding::Identity) => Ok(Box::new(resp.take(REQUEST_HEAD_SIZE))),
        None => Err("bad content encoding".into())
    }
}


fn walk_title(handle: Handle) -> Option<String> {
    use std::string::String;
        
    let node = handle.borrow();
    match node.node {
        Element(ref name, _, _) if &*name.local == "title" => {
            let mut out = String::new();
            if get_text(&node.children, &mut out) {
                return Some(out);
            }
        },
        _ => ()
    }
    for child in node.children.iter() {
        if let Some(text) = walk_title(child.clone()) {
            return Some(text);
        }
    }
    return None;

    fn get_text(handles: &[Handle], out: &mut String) -> bool {
        let mut emitted = false;
        for handle in handles.iter() {
            let node = handle.borrow();
            match node.node {
                Text(ref st) => {
                    emitted = true;
                    out.extend(compact_spaces(&*st));
                },
                Element(_, _, _) => {
                    emitted |= get_text(&node.children, out);
                },
                _ => (),
            }
        }
        emitted
    }

    fn compact_spaces<'a>(inp: &'a str) -> impl Iterator<Item=char> + 'a {
        let mut in_white = false;
        inp.chars().filter_map(move |c| {
            let prev_in_white = in_white;
            in_white = c.is_whitespace();
            match (in_white, prev_in_white) {
                (true, true) => None,
                (true, false) => Some(' '),
                (false, _) => Some(c),
            }
        })
    }
}
