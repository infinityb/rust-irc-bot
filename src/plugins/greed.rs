use std::rand::distributions::{Sample, Range};
use std::cmp::{Less, Equal, Greater};
use std::rand;

use command_mapper::{
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


type RollResult = [u8, ..6];
type ScoreRec = (uint, RollResult, int);


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


struct GreedState {
    channel: String,
    last_played: Option<(String, RollResult, int)>
}


pub struct GreedPlugin {
    states: Vec<GreedState>,
}


#[inline]
fn is_prefix(rec: &ScoreRec, roll: &RollResult, start_idx: uint) -> bool {
    let (prefix_len, ref roll_target, _) = *rec;

    if roll.len() < start_idx + prefix_len {
        return false;
    }
    for idx in range(0, prefix_len) {
        if roll[idx + start_idx] != roll_target[idx] {
            return false;
        }
    }
    true
}


fn dice_roll() -> RollResult {
    let mut rng = rand::task_rng();
    let mut out: RollResult = [0u8, ..6];
    let mut between = Range::new(1u8, 7u8);
    for val in out.mut_iter() {
        *val = between.sample(&mut rng);
    }
    out.sort();
    out
}


fn get_prefix_len(rec: &ScoreRec) -> uint {
    let (prefix_len, _, _) = *rec;
    prefix_len
}


fn get_scores(roll: &RollResult) -> Vec<&'static ScoreRec> {
    let mut idx = 0;
    let mut score_comps = Vec::new();
    while idx < roll.len() {
        let mut idx_incr = 1;
        for score_rec in SCORING_TABLE.iter() {
            // println!("is_prefix({:?}, {:?}, {:?})", score_rec, roll, idx);
            if is_prefix(score_rec, roll, idx) {
                idx_incr = get_prefix_len(score_rec);
                // println!("pushing {:?}", score_rec);
                score_comps.push(score_rec);
                break;
            }
        }
        idx += idx_incr;
    }
    score_comps
}

fn total_score(scores: &Vec<&ScoreRec>) -> int {
    let mut sum = 0;
    for score in scores.iter() {
        let (_, _, score_val) = **score;
        sum += score_val;
    }
    sum
}


fn find_or_create_state<'a>(states: &'a mut Vec<GreedState>, channel: &str) -> &'a mut GreedState {   
    let mut want_idx = None;
    for (i, state) in states.iter().enumerate() {
        if state.channel.as_slice() == channel {
            want_idx = Some(i);
        }
    }
    match want_idx {
        Some(idx) => {
            states.get_mut(idx)
        },
        None => {
            states.push(GreedState {
                channel: String::from_str(channel),
                last_played: None
            });
            states.mut_last().unwrap()
        }
    }
}


fn format_score_component_bare(roll_result: &RollResult) -> String {
    let mut rolls = String::new();
    for value in roll_result.iter() {
        if *value == 0 {
            break
        }
        rolls = rolls.append(format!("{}, ", value).as_slice());
    }
    rolls.pop_char(); rolls.pop_char();
    format!("{}", rolls.as_slice())
}


fn format_score_component(score_components: &ScoreRec) -> String {
    let (_, ref prefix_data, _) = *score_components;
    format_score_component_bare(prefix_data)
}


fn format_score(score_components: &Vec<&ScoreRec>) -> String {
    let mut output = String::new();
    for tuple in score_components.iter() {
        let (_, _, score) = **tuple;
        output = output.append(format!(
            "[{} => {}], ",
            format_score_component(*tuple).as_slice(),
            score
        ).as_slice());
    }
    output.pop_char(); output.pop_char();
    output
}


impl GreedPlugin {
    pub fn new() -> GreedPlugin {
        GreedPlugin {
            states: Vec::new()
        }
    }
}


impl RustBotPlugin for GreedPlugin {
    fn configure(&mut self, conf: &mut IrcBotConfigurator) {
        conf.map("greed");
    }

    fn start(&mut self) {
    }
    
    fn dispatch_cmd(&mut self, m: &CommandMapperDispatch, message: &IrcMessage) {
        let channel = match message.channel() {
            Some(channel) => channel,
            None => return
        };
        let source_nick = match message.source_nick() {
            Some(nickname) => nickname,
            None => return
        };

        let state = find_or_create_state(&mut self.states, channel);

        let cur_user_roll: RollResult = dice_roll();
        let score_components: Vec<&'static ScoreRec> = get_scores(&cur_user_roll);
        let score = total_score(&score_components);

        match state.last_played {
            Some(ref last_played) => {
                let (ref prev_nick, _, _) = *last_played;
                if prev_nick.as_slice() == source_nick.as_slice() {
                    m.reply(format!("You can't go twice in a row, {}", source_nick.as_slice()));
                    return;
                }
            }, 
            None => ()
        }

        m.reply(format!("[{}] => [{}] for {} points",
            format_score_component_bare(&cur_user_roll),
            format_score(&score_components).as_slice(),
            score));

        state.last_played = match state.last_played.take() {
            Some(last_played) => {
                let (prev_nick, _, prev_score) = last_played;
                // let prev_score_components: Vec<&'static ScoreRec> = get_scores(&prev_roll);

                m.reply(match prev_score.cmp(&score) {
                    Less => format!("{} wins!", source_nick),
                    Equal => format!("{} and {} tie.", source_nick, prev_nick),
                    Greater => format!("{} wins!", prev_nick)
                });
                None
            },
            None => {
                Some((source_nick, cur_user_roll, score))
            }
        }
    }
}



// fn get_score(roll: &RollResult) -> int {
//     total_score(&get_scores(roll))
// }
// #[test]
// fn test_greed_matchers() {
//     assert!(is_prefix(&SCORING_TABLE[0], &[1, 2, 3, 4, 5, 6], 0));
//     assert!(is_prefix(&SCORING_TABLE[1], &[2, 2, 3, 3, 4, 4], 0));
//     assert!(is_prefix(&SCORING_TABLE[2], &[1, 1, 1, 1, 1, 1], 0));
//     assert!(is_prefix(&SCORING_TABLE[5], &[1, 1, 1, 2, 2, 2], 0));
//     assert!(is_prefix(&SCORING_TABLE[5], &[1, 1, 1, 0, 0, 0], 0));
//     assert!(is_prefix(&SCORING_TABLE[5], &[0, 0, 0, 1, 1, 1], 3));
//     assert_eq!(get_score(&[1, 1, 1, 2, 4, 5]), 1050);
//     assert_eq!(get_score(&[1, 3, 4, 5, 5, 6]), 200);
//     assert_eq!(get_score(&[1, 2, 3, 4, 5, 6]), 1200);
//     assert_eq!(get_score(&[1, 1, 1, 2, 3, 5]), 1050);
//     assert_eq!(get_score(&[2, 2, 3, 4, 5, 6]), 50);
//     assert_eq!(get_score(&[1, 3, 4, 5, 6, 6]), 150);
//     assert_eq!(get_score(&[1, 1, 1, 3, 3, 3]), 1300);
// }

