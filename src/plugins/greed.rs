use std::collections::hashmap::{
    HashMap,
    Vacant,
    Occupied,
};
use std::rand::distributions::{Sample, Range};
use std::cmp::{Less, Equal, Greater};
use std::fmt::{Formatter, FormatError, Show};
use std::rand::{task_rng, Rng, Rand};

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator,
    Format,
};
use state::{
    BotChannelId,
    BotUserId,
    KnownChannel,
    KnownUser,
};
use irc::message::{
    IrcMessage
};


type ScorePrefix = [u8, ..6];
type ScoreRec = (uint, ScorePrefix, int);

pub static SCORING_TABLE: [ScoreRec, ..28] = [
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

struct RollResult([u8, ..6]);


#[inline]
fn is_prefix(rec: &ScoreRec, roll_res: &RollResult, start_idx: uint) -> bool {
    let RollResult(ref roll_data) = *roll_res;
    let (prefix_len, ref roll_target, _) = *rec;

    if roll_data.len() < start_idx + prefix_len {
        return false;
    }
    for idx in range(0, prefix_len) {
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

    fn total_score(&self) -> int {
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
            rolls.push_str(format!("{}, ", value).as_slice());
        }
        rolls.pop(); rolls.pop();
        format!("{}", rolls.as_slice())
    }


    fn format_score_component(score_components: &ScoreRec) -> String {
        let (_, ref prefix_data, _) = *score_components;
        RollResult::format_score_component_bare(prefix_data)
    }

    fn format_score(score_components: &Vec<&ScoreRec>) -> String {
        let mut output = String::new();
        for tuple in score_components.iter() {
            let (_, _, score) = **tuple;
            output.push_str(format!(
                "[{} => {}], ",
                RollResult::format_score_component(*tuple).as_slice(),
                score
            ).as_slice());
        }
        output.pop(); output.pop();
        output
    }
}

impl Rand for RollResult {
    fn rand<R: Rng>(rng: &mut R) -> RollResult {
        let mut out: ScorePrefix = [0u8, ..6];
        let mut between = Range::new(1u8, 7u8);
        for val in out.iter_mut() {
            *val = between.sample(rng);
        }
        out.sort();
        RollResult(out)
    }
}

impl Show for RollResult {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FormatError> {
        let RollResult(ref roll) = *self;
        write!(f, "[{}] => [{}] for {} points",
            RollResult::format_score_component_bare(roll),
            RollResult::format_score(&self.get_scores()),
            self.total_score())
    }
}

pub struct GreedPlugin {
    games: HashMap<BotChannelId, GreedPlayResult>,
    leaderboard: HashMap<BotChannelId, HashMap<BotUserId, GameHistory>>
}

pub struct GameHistory {
    user_nick: String,
    games_won: uint,
    games_lost: uint,
    points_acc: u64,
    score: i64
}

impl Show for GameHistory {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FormatError> {
        write!(f, "{} has won {} games, lost {} games and scored {} points",
            self.user_nick, self.games_won, self.games_lost,
            self.points_acc)
    }
}

enum GreedCommandType {
    Greed,
    GreedStats,
    GreedTop,
}

struct GreedPlayResult {
    user_id: BotUserId,
    user_nick: String,
    roll: RollResult,
}


impl GreedPlugin {
    pub fn new() -> GreedPlugin {
        GreedPlugin {
            games: HashMap::new(),
            leaderboard: HashMap::new(),
        }
    }

    fn dispatch_cmd_greed(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let (user_id, channel_id) = match (m.source.clone(), m.target.clone()) {
            (Some(KnownUser(uid)), Some(KnownChannel(cid))) => (uid, cid),
            _ => return
        };

        let source_nick = match message.source_nick() {
            Some(nickname) => nickname,
            None => return
        };

        match self.games.entry(channel_id) {
            Vacant(entry) => {
                let roll = task_rng().gen::<RollResult>();
                m.reply(format!("{}: {}", source_nick, roll));
                entry.set(GreedPlayResult {
                    user_id: user_id,
                    user_nick: source_nick.to_string(),
                    roll: roll
                });
            },
            Occupied(entry) => {
                {
                    let prev_play = entry.get();
                    if prev_play.user_id == user_id {
                        m.reply(format!("You can't go twice in a row, {}", source_nick));
                        return;
                    }
                }
                let prev_play = entry.take();
                let roll = task_rng().gen::<RollResult>();
                m.reply(format!("{}: {}", source_nick, roll));

                m.reply(match prev_play.roll.total_score().cmp(&roll.total_score()) {
                     Less => format!("{} wins!", source_nick),
                     Equal => format!("{} and {} tie.", source_nick, prev_play.user_nick),
                     Greater => format!("{} wins!", prev_play.user_nick)
                });
            }
        }
    }

    fn dispatch_cmd_greed_stats(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let (user_id, channel_id) = match (m.source.clone(), m.target.clone()) {
            (Some(KnownUser(uid)), Some(KnownChannel(cid))) => (uid, cid),
            _ => return
        };
        let source_nick = match message.source_nick() {
            Some(nickname) => nickname,
            None => return
        };
        let hashmap = match self.leaderboard.entry(channel_id) {
            Vacant(entry) => {
                m.reply(format!(
                    "{}: You haven't played any games in this channel",
                    source_nick));
                return
            },
            Occupied(entry) => entry.into_mut(),
        };
        let history = match hashmap.entry(user_id) {
            Vacant(entry) => {
                m.reply(format!(
                    "{}: You haven't played any games in this channel",
                    source_nick));
                return
            },
            Occupied(entry) => entry.into_mut(),
        };
        m.reply(format!("{}", history));
    }

    fn dispatch_cmd_greed_top(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let (user_id, channel_id) = match (m.source.clone(), m.target.clone()) {
            (Some(KnownUser(uid)), Some(KnownChannel(cid))) => (uid, cid),
            _ => return
        };
    }

    fn parse_command<'a>(m: &CommandMapperDispatch) -> Option<GreedCommandType> {
        let command_phrase = match m.command() {
            Some(command_phrase) => command_phrase,
            None => return None
        };
        match command_phrase.command[] {
            "greed" => Some(Greed),
            "greed-stats" => Some(GreedStats),
            "greed-top" => Some(GreedTop),
            _ => None
        }
    }
}


impl RustBotPlugin for GreedPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map_format(Format::from_str("greed").unwrap());
        conf.map_format(Format::from_str("greed-stats").unwrap());
        conf.map_format(Format::from_str("greed-top").unwrap());
    }
    
    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        match GreedPlugin::parse_command(m) {
            Some(Greed) => self.dispatch_cmd_greed(m, message),
            Some(GreedStats) => self.dispatch_cmd_greed_stats(m, message),
            Some(GreedTop) => self.dispatch_cmd_greed_top(m, message),
            None => ()
        }
    }
}
