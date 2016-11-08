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
        let searchlist: Vec<_> = body.split(' ').filter_map(|word| {
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
        let mut out = Vec::new();
        for search in searchlist.iter().take(1) {
            println!("running search {:?}", search);
            match api.execute(search) {
                Ok(val) => {
                    if let Ok(val) = format_thread(&search.0, &val) {
                        out.push(val);
                    }                   
                }
                Err(e) => println!("lookup {:?} -> {:?}", search, e),
            }
            println!("ok search term finished");
        }

        // reply_target
        let msg_out = out.join(" ");
        let _ = m.reply(&client::PrivmsgBuf::new(
            reply_target.as_bytes(),
            msg_out.as_bytes(),
        ).unwrap());
    }
}

fn format_thread(search: &ImageLocator, thread: &Thread) -> Result<String, ()> {
    let thread_no = try!(thread.post_ids().next().ok_or(()));
    let mut post_id = None;
    for (pid, iname) in thread.posts_with_image_names() {
        if search.image_name == iname {
            post_id = Some(pid);
        }
    }
    let post_id = match post_id {
        Some(post_id) => post_id,
        None => return Err(()),
    };
    let post = match thread.get_post(post_id) {
        Some(post) => post,
        None => return Err(()),
    };
    let filename = match post.get_filename() {
        Some(filename) => filename,
        None => return Err(()),
    };

    let thread_loc = ThreadLocator {
        board: search.board.clone(),
        thread_no: thread_no as i64,
    };

    let mut out = Vec::new();
    // write!(&mut out, "{} from {}#p{}", filename, thread_loc, post_id).unwrap();
    // if let Some(sub) = thread.get_subject() {
    //     write!(&mut out, "- {}", sub).unwrap();
    // }

    Ok(String::from_utf8(out).unwrap())
}
