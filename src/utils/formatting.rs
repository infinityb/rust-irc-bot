use time::Duration;

pub fn duration_to_string(dur: Duration) -> String {
    let days = dur.num_days();
    let hours = dur.num_hours() % 24;
    let minutes = dur.num_minutes() % 60;
    let seconds = dur.num_seconds() % 60;

    let mut string = String::new();
    if days > 0 {
        string.push_str(&format!("{}d", days));
    }
    if hours > 0 {
        string.push_str(&format!("{}h", hours));
    }
    if minutes > 0 {
        string.push_str(&format!("{}m", minutes));
    }
    if string.len() == 0 || seconds > 0 {
        string.push_str(&format!("{}s", seconds));
    }
    string
}

#[derive(Debug)]
pub enum MaybeString<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> MaybeString<'a> {
    pub fn new(buf: &'a [u8]) -> MaybeString<'a> {
        match ::std::str::from_utf8(buf) {
            Ok(s) => MaybeString::String(s),
            Err(_) => MaybeString::Bytes(buf),
        }
    }
}
