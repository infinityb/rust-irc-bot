use command_mapper::{
    RustBotPluginApi,
    RustBotPlugin,
    CommandMapperDispatch,
    IrcBotConfigurator
};
use message::{
    IrcMessage
};


type RollResult = [u8, ..6];
type ScoreRec = (RollResult, int);


pub static SCORING_TABLE: [ScoreRec, ..28] = [
    ([1, 2, 3, 4, 5, 6], 1200),
    ([2, 2, 3, 3, 4, 4],  800),
    ([1, 1, 1, 1, 1, 1], 8000),
    ([1, 1, 1, 1, 1, 0], 4000),
    ([1, 1, 1, 1, 0, 0], 2000),
    ([1, 1, 1, 0, 0, 0], 1000),
    ([1, 0, 0, 0, 0, 0],  100),
    ([2, 2, 2, 2, 2, 2], 1600),
    ([2, 2, 2, 2, 2, 0],  800),
    ([2, 2, 2, 2, 0, 0],  400),
    ([2, 2, 2, 0, 0, 0],  200),
    ([3, 3, 3, 3, 3, 3], 2400),
    ([3, 3, 3, 3, 3, 0], 1200),
    ([3, 3, 3, 3, 0, 0],  600),
    ([3, 3, 3, 0, 0, 0],  300),
    ([4, 4, 4, 4, 4, 4], 3200),
    ([4, 4, 4, 4, 4, 0], 1600),
    ([4, 4, 4, 4, 0, 0],  800),
    ([4, 4, 4, 0, 0, 0],  400),
    ([5, 5, 5, 5, 5, 5], 4000),
    ([5, 5, 5, 5, 5, 0], 2000),
    ([5, 5, 5, 5, 0, 0], 1000),
    ([5, 5, 5, 0, 0, 0],  500),
    ([5, 0, 0, 0, 0, 0],   50),
    ([6, 6, 6, 6, 6, 6], 4800),
    ([6, 6, 6, 6, 6, 0], 2400),
    ([6, 6, 6, 6, 0, 0], 1200),
    ([6, 6, 6, 0, 0, 0],  600),
];


struct GreedState {
    channel: String,
    last_played: Option<(String, Vec<u16>, u16)>
}


pub struct GreedPlugin {
    states: Vec<GreedState>,
}

#[inline]
fn is_prefix(rec: &ScoreRec, roll: RollResult, start_idx: uint) -> bool {
    let mut looking_for = [0u8, ..6];
    for (idx, val) in roll.iter().skip(start_idx).enumerate() {
        looking_for[idx] = *val;
    }

    let (ref roll_target, _) = *rec;
    for (idx, roll) in looking_for.iter().enumerate() {
        if roll_target[idx] == 0 {
            // finished search
            return true;
        }
        if *roll != roll_target[idx] {
            return false;
        }

    }
    true
}


#[test]
fn test_greed_matchers() {
    assert!(is_prefix(&SCORING_TABLE[0], [1, 2, 3, 4, 5, 6], 0));
    assert!(is_prefix(&SCORING_TABLE[1], [2, 2, 3, 3, 4, 4], 0));
    assert!(is_prefix(&SCORING_TABLE[2], [1, 1, 1, 1, 1, 1], 0));

    assert!(is_prefix(&SCORING_TABLE[5], [1, 1, 1, 2, 2, 2], 0));
    assert!(is_prefix(&SCORING_TABLE[5], [1, 1, 1, 0, 0, 0], 0));
    assert!(is_prefix(&SCORING_TABLE[5], [0, 0, 0, 1, 1, 1], 3));

    assert_eq!(get_score([1, 1, 1, 2, 4, 5]), 1050);
    assert_eq!(get_score([1, 3, 4, 5, 5, 6]), 150);
    assert_eq!(get_score([1, 2, 3, 4, 5, 6]), 1200);
    assert_eq!(get_score([1, 1, 1, 2, 3, 5]), 1050);
    assert_eq!(get_score([2, 2, 3, 4, 5, 6]), 50);
    assert_eq!(get_score([1, 3, 4, 5, 6, 6]), 150);
}


fn get_prefix_len(rec: &ScoreRec) -> uint {
    let (ref roll_target, _) = *rec;
    for (idx, val) in roll_target.iter().enumerate() {
        if *val == 0 {
            return idx;
        }
    }
    roll_target.len()
}

fn get_scores(roll: RollResult) -> Vec<&'static ScoreRec> {
    let mut idx = 0;
    let mut score_comps = Vec::new();
    while idx < roll.len() {
        for score_rec in SCORING_TABLE.iter() {
            // println!("is_prefix({:?}, {:?}, {:?})", score_rec, roll, idx));
            if is_prefix(score_rec, roll, idx) {
                idx += get_prefix_len(score_rec);
                println!("pushing {:?}", score_rec);
                score_comps.push(score_rec);
                break;
            }
        }
        idx += 1;
    }
    score_comps
}

fn get_score(roll: RollResult) -> int {
    let scores = get_scores(roll);
    let mut sum = 0;
    for score in scores.iter() {
        let (_, score_val) = **score;
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

        let state = find_or_create_state(&mut self.states, channel);
        state.last_played = match state.last_played.take() {
            Some(last_played) => {
                let (ref nick, ref rolls, score) = last_played;
                println!("{} rolled {} for {}", nick, rolls, score);
                None
            },
            None => {
                println!("no previous roll. insert");
                None
            }
        }
    }
}
