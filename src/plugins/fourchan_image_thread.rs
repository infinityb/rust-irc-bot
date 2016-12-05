use std::io::Write;

use hyper;
use irc::{server, client, IrcMsg};

use command_mapper::{
    RustBotPlugin,
    Replier,
};
use yotsuba_apiclient::{
    ImageLocator,
    ImageNameSearch2,
    FourchanApi,
    Thread,
    ThreadLocator,
};

pub struct FourchanImageThreadPlugin;

impl FourchanImageThreadPlugin {
    pub fn new() -> FourchanImageThreadPlugin {
        FourchanImageThreadPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "fourchan-image-thread"
    }
}

impl RustBotPlugin for FourchanImageThreadPlugin {
    fn on_message(&mut self, m: &mut Replier, msg: &IrcMsg) {
        use std::str;
        use std::time::Duration;
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
        println!("ok looking up {}", body);

        let reply_target = str::from_utf8(privmsg.get_target()).unwrap();
        let searchlist: Vec<_> = body.split(char::is_whitespace).filter_map(|word| {
            ImageLocator::parse_fourchan_url(word).map(ImageNameSearch2).ok()
        }).collect();

        println!("ok found: {:?}", searchlist);
        if searchlist.len() == 0 {
            return;
        }

        let mut client = hyper::Client::new();
        // client.set_redirect_policy(FollowNone);
        client.set_read_timeout(Some(Duration::new(10, 0)));

        let api = FourchanApi::with_client(client);
        
        // make this threaded ....
        // let mut emitted = HashSet::new();
        let mut out = Vec::new();
        for search in searchlist.iter() {
            println!("running search {:?}", search);
            let val_res = api.execute(search)
                .map_err(|e| format!("lookup {:?} -> {:?}", search, e))
                .and_then(|val| format_thread(&search.0, &val).map_err(Into::into));
            match val_res {
                Ok(val) => {
                    out.push(val);         
                }
                Err(err) => println!("error: {}", err),
            }
            println!("ok search term finished");
        }

        // reply_target
        for line in out.iter() {
            let _ = m.reply(&client::PrivmsgBuf::new(
                reply_target.as_bytes(),
                line.as_bytes(),
            ).unwrap());
        }
    }
}

fn format_thread(search: &ImageLocator, thread: &Thread) -> Result<String, &'static str> {
    let thread_no = try!(thread.post_ids().next().ok_or("thread_no missing"));
    let mut post_id = None;
    for (pid, iname) in thread.posts_with_image_names() {
        if search.image_name == iname {
            post_id = Some(pid);
        }
    }
    let post_id = try!(post_id.ok_or("post_id missing"));
    let post = try!(thread.get_post(post_id).ok_or("root post missing"));
    let filename = try!(post.get_filename().ok_or("filename missing"));

    let thread_loc = ThreadLocator {
        board: search.board.clone(),
        thread_no: thread_no as i64,
    };

    let mut out = Vec::new();
    write!(&mut out, "{} from {}#p{}", filename, thread_loc, post_id).unwrap();
    if let Some(sub) = thread.get_subject() {
        write!(&mut out, " - {}", sub).unwrap();
    }

    Ok(String::from_utf8(out).unwrap())
}
