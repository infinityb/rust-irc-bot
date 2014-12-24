extern crate "dbus-rs" as dbus;

use dbus::{Connection, BusType};

use irc::IrcMessage;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};

static TANK_MEMBER_TITLES: &'static [&'static str] = &[
    "Commander", "Driver", "Gunner", "Loader", "Radio Operator"
];

static MAHJONG_MEMBER_TITLES: &'static [&'static str] = &[
    "Vanguard", "Sergeant", "Lieutenant", "Vice-Captain", "Captain"
];


pub struct WaifuPlugin;

impl WaifuPlugin {
    pub fn new() -> WaifuPlugin {
        WaifuPlugin
    }

    pub fn get_plugin_name() -> &'static str {
        "waifu"
    }
}

enum WaifuCommand {
    Waifu,
    Harem,
    TankTeam,
    MahjongTeam,
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<WaifuCommand> {
    match m.command().command[] {
        "waifu" => Some(WaifuCommand::Waifu),
        "harem" => Some(WaifuCommand::Harem),
        "maitankteam" => Some(WaifuCommand::TankTeam),
        "maimahjongteam" => Some(WaifuCommand::MahjongTeam),
        _ => None
    }
}


mod waifuapi {
    use dbus::{Connection, Message, MessageItem};

    fn convert_get_random(items: Vec<MessageItem>) -> Result<Vec<(String, String)>, ()> {
        let mut out = Vec::new();
        for item in items.iter() {
            if let MessageItem::Array(ref pair, _) = *item {
                if let [MessageItem::Str(ref series), MessageItem::Str(ref chara)] = pair.as_slice() {
                    out.push((series.clone(), chara.clone()));
                } else {
                    return Err(());
                }
            } else {
                return Err(());
            }
        }
        Ok(out)
    }

    pub fn get_random(conn: &Connection, num: i32) -> Result<Vec<(String, String)>, &'static str> {
        let mut methcall = Message::new_method_call(
            "org.yasashiisyndicate.waifuserv", "/org/yasashiisyndicate/waifuserv",
            "org.yasashiisyndicate.waifuserv", "GetRandom").unwrap();
        methcall.append_items(&[MessageItem::Int32(num)]);
        Ok(match conn.send_with_reply_and_block(methcall, 2000) {
            Ok(mut rr) => match convert_get_random(rr.get_items()) {
                Ok(resp) => resp,
                Err(()) => return Err("Error processing response from Waifu service"),
            },
            Err(_) => return Err("Waifu service appears to be down"),
        })
    }
}

fn format_single(resp: &(String, String)) -> String {
    let (ref series, ref chara) = *resp;
    format!("\x02{}\x02 ({})", chara, series)
}

fn format_many(resp: &[(String, String)]) -> String {
    let format_parts: Vec<_> = resp.iter().map(|tup| format_single(tup)).collect();
    format_parts.connect(", ")
}

fn format_single_with_title(resp: &(String, String), title: &str) -> String {
    let (ref series, ref chara) = *resp;
    format!("{}: \x02{}\x02 ({})", title, chara, series)
}

fn format_many_with_titles(resp: &[(String, String)], titles: &[&str]) -> String {
    let format_parts: Vec<_> = resp.iter().zip(titles.iter())
        .map(|(tup, &title)| format_single_with_title(tup, title)).collect();
    format_parts.connect(", ")
}


impl RustBotPlugin for WaifuPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("waifu").unwrap());
        conf.map_format(Format::from_str("harem").unwrap());
        conf.map_format(Format::from_str("maitankteam").unwrap());
        conf.map_format(Format::from_str("maimahjongteam").unwrap());
    }

    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, _: &IrcMessage) {
        let c = Connection::get_private(BusType::System).unwrap();
        match parse_command(m) {
            Some(WaifuCommand::Waifu) => {
                m.reply(match waifuapi::get_random(&c, 1) {
                    Ok(resp) => format_many(resp.as_slice()),
                    Err(errmsg) => format!("err: {}", errmsg),
                });
            },
            Some(WaifuCommand::Harem) => {
                m.reply(match waifuapi::get_random(&c, 5) {
                    Ok(resp) => format_many(resp.as_slice()),
                    Err(errmsg) => format!("err: {}", errmsg),
                });
            },
            Some(WaifuCommand::TankTeam) => {
                m.reply(match waifuapi::get_random(&c, TANK_MEMBER_TITLES.len() as i32) {
                    Ok(resp) => format_many_with_titles(resp.as_slice(), TANK_MEMBER_TITLES),
                    Err(errmsg) => format!("err: {}", errmsg),
                });
            },
            Some(WaifuCommand::MahjongTeam) => {
                m.reply(match waifuapi::get_random(&c, MAHJONG_MEMBER_TITLES.len() as i32) {
                    Ok(resp) => format_many_with_titles(resp.as_slice(), MAHJONG_MEMBER_TITLES),
                    Err(errmsg) => format!("err: {}", errmsg),
                });
            },
            None => return
        }
    }
}
