use std::collections::HashMap;
use std::collections::hash_map;
use std::default::Default;
use std::cmp::Ordering;
use std::fmt;

use ::rand::{thread_rng, Rng, Rand};
use ::rand::distributions::{Sample, Range};

use irc::legacy::{
    ChannelId,
    User,
    UserId,
};

use irc::{IrcMsg, server};
use irc::legacy::MessageEndpoint::{
    KnownChannel,
    KnownUser,
};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
    Token,
};

const CMD_GREED: Token = Token(0);
const CMD_GREED_STATS: Token = Token(1);

type ScorePrefix = [u8; 6];
type ScoreRec = (usize, ScorePrefix, i32);

static SCORING_TABLE: [ScoreRec; 28] = [
    (6, [1, 2, 3, 4, 5, 6], 1200),
    (6, [2, 2, 3, 3, 4, 4],  800),
    (6, [1, 1, 1, 1, 1, 1], 8000),
    (5, [1, 1, 1, 1, 1, 0], 4000),
    (4, [1, 1, 1, 1, 0, 0], 2000),
    (3, [1, 1, 1, 0, 0, 0], 1000),
    (1, [1, 0, 0, 0, 0, 0],  100),
    (6, [2, 2, 2, 2, 2, 2], 1600),
    (5, [2, 2, 2, 2, 2, 0],  800),
    (4, [2, 2, 2, 2, 0, 0],  400),
    (3, [2, 2, 2, 0, 0, 0],  200),
    (6, [3, 3, 3, 3, 3, 3], 2400),
    (5, [3, 3, 3, 3, 3, 0], 1200),
    (4, [3, 3, 3, 3, 0, 0],  600),
    (3, [3, 3, 3, 0, 0, 0],  300),
    (6, [4, 4, 4, 4, 4, 4], 3200),
    (5, [4, 4, 4, 4, 4, 0], 1600),
    (4, [4, 4, 4, 4, 0, 0],  800),
    (3, [4, 4, 4, 0, 0, 0],  400),
    (6, [5, 5, 5, 5, 5, 5], 4000),
    (5, [5, 5, 5, 5, 5, 0], 2000),
    (4, [5, 5, 5, 5, 0, 0], 1000),
    (3, [5, 5, 5, 0, 0, 0],  500),
    (1, [5, 0, 0, 0, 0, 0],   50),
    (6, [6, 6, 6, 6, 6, 6], 4800),
    (5, [6, 6, 6, 6, 6, 0], 2400),
    (4, [6, 6, 6, 6, 0, 0], 1200),
    (3, [6, 6, 6, 0, 0, 0],  600),
];

struct RollResult([u8; 6]);


#[inline]
fn is_prefix(rec: &ScoreRec, roll_res: &RollResult, start_idx: usize) -> bool {
    let RollResult(ref roll_data) = *roll_res;
    let (prefix_len, ref roll_target, _) = *rec;

    if roll_data.len() < start_idx + prefix_len {
        return false;
    }
    for idx in 0..prefix_len {
        if roll_data[idx + start_idx] != roll_target[idx] {
            return false;
        }
    }
    true
}

impl RollResult {
    fn get_scores(&self) -> Vec<&'static ScoreRec> {
        let RollResult(ref roll) = *self;
        let mut idx = 0;
        let mut score_comps = Vec::new();
        while idx < roll.len() {
            let mut idx_incr = 1;
            for score_rec in SCORING_TABLE.iter() {
                if is_prefix(score_rec, self, idx) {
                    let (prefix_len, _, _) = *score_rec;
                    idx_incr = prefix_len;
                    score_comps.push(score_rec);
                    break;
                }
            }
            idx += idx_incr;
        }
        score_comps
    }

    fn total_score(&self) -> i32 {
        let mut sum = 0;
        for score in self.get_scores().iter() {
            let (_, _, score_val) = **score;
            sum += score_val;
        }
        sum
    }

    fn format_score_component_bare(score_pref: &ScorePrefix) -> String {
        let mut rolls = String::new();
        for value in score_pref.iter() {
            if *value == 0 {
                break
            }
            rolls.push_str(&format!("{}, ", value));
        }
        rolls.pop(); rolls.pop();
        format!("{}", rolls)
    }


    fn format_score_component(score_components: &ScoreRec) -> String {
        let (_, ref prefix_data, _) = *score_components;
        RollResult::format_score_component_bare(prefix_data)
    }


    fn format_score(score_components: &Vec<&ScoreRec>) -> String {
        let mut output = String::new();
        for tuple in score_components.iter() {
            let (_, _, score) = **tuple;
            let roll_res = RollResult::format_score_component(*tuple);
            output.push_str(&format!("[{} => {}], ", roll_res, score));
        }
        output.pop(); output.pop();
        output
    }
}

impl Rand for RollResult {
    fn rand<R: Rng>(rng: &mut R) -> RollResult {
        let mut out: ScorePrefix = [0u8; 6];
        let mut between = Range::new(1u8, 7u8);
        for val in out.iter_mut() {
            *val = between.sample(rng);
        }
        out.sort();
        RollResult(out)
    }
}

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let RollResult(ref roll) = *self;
        write!(f, "[{}] => [{}] for {} points",
            RollResult::format_score_component_bare(roll),
            RollResult::format_score(&self.get_scores()),
            self.total_score())
    }
}

pub struct GreedPlugin {
    games: HashMap<ChannelId, GreedPlayResult>,
    userstats: HashMap<UserId, UserStats>,
}

enum GreedCommandType {
    Greed,
    GreedStats
}

struct GreedPlayResult {
    user_id: UserId,
    user_nick: String,
    roll: RollResult,
}

fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<GreedCommandType> {
    match m.command().token {
        CMD_GREED => Some(GreedCommandType::Greed),
        CMD_GREED_STATS => Some(GreedCommandType::GreedStats),
        _ => None
    }
}

struct UserStats {
    games: u32,
    wins: u32,
    score_sum: i32,
    opponent_score_sum: i32
}

impl Default for UserStats {
    fn default() -> UserStats {
        UserStats {
            games: 0,
            wins: 0,
            score_sum: 0,
            opponent_score_sum: 0,
        }
    }
}

impl fmt::Display for UserStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{} wins over {} games; points: {}",
            self.wins, self.games, self.score_sum - self.opponent_score_sum)
    }
}


impl GreedPlugin {
    pub fn new() -> GreedPlugin {
        GreedPlugin {
            games: HashMap::new(),
            userstats: HashMap::new(),
        }
    }

    pub fn get_plugin_name() -> &'static str {
        "greed"
    }
    
    fn dispatch_cmd_greed_stats(&mut self, m: &CommandMapperDispatch, msg: &server::Privmsg) {
        let user_id = match m.source {
            KnownUser(user_id) => user_id,
            _ => return
        };
        m.reply(match self.userstats.get(&user_id) {
            Some(stats) => format!("{}: {}", msg.source_nick(), stats),
            None => format!("{}: You haven't played any games yet", msg.source_nick())
        }.as_ref());
    }

    fn add_userstats_roll(&mut self, uid: UserId, win: bool, self_score: i32, opp_score: i32) {
        let cur_user = match self.userstats.entry(uid) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => entry.insert(Default::default())
        };
        cur_user.games += 1;
        cur_user.wins += if win { 1 } else { 0 };
        cur_user.score_sum += self_score;
        cur_user.opponent_score_sum += opp_score;
    }

    fn dispatch_cmd_greed(&mut self, m: &CommandMapperDispatch, msg: &server::Privmsg) {
        let (user_id, channel_id) = match (m.source.clone(), m.target.clone()) {
            (KnownUser(uid), KnownChannel(cid)) => (uid, cid),
            _ => return
        };

        let source_nick = msg.source_nick();

        let prev_play_opt: Option<GreedPlayResult> = match self.games.entry(channel_id) {
            hash_map::Entry::Vacant(entry) => {
                let roll = thread_rng().gen::<RollResult>();
                m.reply(&format!("{}: {}", source_nick, roll));
                entry.insert(GreedPlayResult {
                    user_id: user_id,
                    user_nick: source_nick.to_string(),
                    roll: roll
                });
                None
            },
            hash_map::Entry::Occupied(entry) => {
                if entry.get().user_id == user_id {
                    m.reply(&format!("You can't go twice in a row, {}", source_nick));
                    None
                } else {
                    Some(entry.remove())
                }
            }
        };
        if let Some(prev_play) = prev_play_opt {
            let roll = thread_rng().gen::<RollResult>();
            m.reply(&format!("{}: {}", source_nick, roll));

            let prev_play_nick = m.get_state().resolve_user(prev_play.user_id)
                .and_then(|user: &User| Some(user.get_nick().to_string()))
                .unwrap_or_else(|| format!("{} (deceased)", prev_play.user_nick));
            let prev_play_score = prev_play.roll.total_score();
            let cur_play_score = roll.total_score();
            let cmp_result = prev_play_score.cmp(&cur_play_score);
            let (prev_user_wins, cur_user_wins) = match cmp_result {
                Ordering::Less => (false, true),
                Ordering::Equal => (false, false),
                Ordering::Greater => (true, false)
            };
            let score_diff = (prev_play_score - cur_play_score).abs();
            let response = match cmp_result {
                 Ordering::Less => format!("{} wins {} points from {}!",
                    source_nick, score_diff, prev_play_nick),
                 Ordering::Equal => format!("{} and {} tie.", source_nick, prev_play_nick),
                 Ordering::Greater => format!("{} wins {} points from {}!",
                    prev_play_nick, score_diff, source_nick),
            };
            m.reply(&response);
            self.add_userstats_roll(user_id, cur_user_wins,
                cur_play_score, prev_play_score);
            self.add_userstats_roll(prev_play.user_id, prev_user_wins,
                prev_play_score, cur_play_score);
        }
    }
}

impl RustBotPlugin for GreedPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(CMD_GREED, Format::from_str("greed").unwrap());
        conf.map_format(CMD_GREED_STATS, Format::from_str("greed-stats").unwrap());
    }
    
    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, msg: &IrcMsg) {
        let privmsg;
        match msg.as_tymsg::<&server::Privmsg>() {
            Ok(p) => privmsg = p,
            Err(_) => return,
        }

        match parse_command(m) {
            Some(GreedCommandType::Greed) => self.dispatch_cmd_greed(m, privmsg),
            Some(GreedCommandType::GreedStats) => self.dispatch_cmd_greed_stats(m, privmsg),
            None => ()
        }
    }
}
