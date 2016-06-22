use std::collections::BTreeMap;
use super::Token;


#[derive(Debug, PartialEq, Eq)]
pub enum FormatParseError {
    EmptyFormat,
    InvalidAtom(String),
    BrokenFormat,
}
pub type FormatResult<T> = Result<T, FormatParseError>;

#[derive(Debug, PartialEq, Eq)]
pub enum ValueParseError {
    Mismatch(&'static str),
    MessageTooShort,
    MessageTooLong,
}
pub type ValueResult<T> = Result<T, ValueParseError>;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AtomType {
    String,
    WholeNumeric
}

#[derive(Debug, PartialEq, Eq, Clone)]
// TODO: remove pub
pub enum Atom {
    // Literal(value)
    Literal(String),
    // Formatted(name, kind)
    Formatted(String, AtomType),
    // Rest(name)
    Rest(String),
    Whitespace,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Value {
    Literal(String),
    String(String),
    WholeNumeric(String)
}

impl Value {
    fn parse(kind: AtomType, input: &str) -> ValueResult<Value> {
        match kind {
            AtomType::String => Ok(Value::String(input.to_string())),
            AtomType::WholeNumeric => {
                // TODO: check if it is a numberish thing
                Ok(Value::WholeNumeric(input.to_string()))
            }
        }
    }
}

fn consume_token<'a>(from: &'a str) -> ValueResult<(&'a str, &'a str)> {
    match from.find(' ') {
        Some(idx) => Ok((&from[..idx], &from[idx..])),
        None => Ok((from, ""))
    }
}

fn consume_literal<'a>(from: &'a str, literal: &str) -> ValueResult<(&'a str, &'a str)> {
    let from_s = from.to_lowercase();
    if from_s.starts_with(literal) {
        let length = literal.len();
        Ok((&from[..length], &from[length..]))
    } else {
        Err(ValueParseError::Mismatch("literal mismatch"))
    }
}

fn consume_whitespace<'a>(from: &'a str) -> (&'a str, &'a str) {
    let mut idx = 0;
    while from[idx..].starts_with(" ") {
        idx += 1;
    }
    (&from[..idx], &from[idx..])
}


impl Atom {
    fn consume<'a>(&self, input: &'a str) -> ValueResult<(Option<Value>, &'a str)> {
        match *self {
            Atom::Literal(ref val) => {
                let (lit, rest) = try!(consume_literal(input, &val));
                let value = Value::Literal(lit.to_string());
                Ok((Some(value), rest))
            },
            Atom::Formatted(_, kind) => {
                let (lit, rest) = try!(consume_token(input));
                let value = try!(Value::parse(kind, lit));
                Ok((Some(value), rest))
            },
            Atom::Rest(_) => {
                let value = try!(Value::parse(AtomType::String, input));
                Ok((Some(value), ""))
            },
            Atom::Whitespace => {
                let (whitespace, rest) = consume_whitespace(input);
                if whitespace.len() == 0 {
                    return Err(ValueParseError::Mismatch("Missing whitespace"));
                }
                Ok((None, rest))
            }
        }
    }
}

#[derive(Debug)]
pub struct Format {
    atoms: Vec<Atom>
}

#[derive(Debug, Clone)]
pub struct CommandPhrase {
    pub token: Token,
    pub command: String,
    pub original_command: String,
    args: BTreeMap<String, Value>
}

impl CommandPhrase {
    pub fn get<T: ValueExtract>(&self, key: &str) -> Option<T> {
        match self.args.get(&key.to_string()) {
            Some(value) => ValueExtract::value_extract(value),
            None => None
        }
    }
}

pub trait ValueExtract: Sized {
    fn value_extract(val: &Value) -> Option<Self>;
}

impl ValueExtract for String {
    fn value_extract(val: &Value) -> Option<String> {
        match *val {
            Value::String(ref str_val) => Some(str_val.clone()),
            _ => None
        }
    }
}

impl ValueExtract for u64 {
    fn value_extract(val: &Value) -> Option<u64> {
        match *val {
            Value::WholeNumeric(ref str_val) => str_val.parse().ok(),
            _ => None
        }
    }
}

impl Format {
    pub fn from_str(definition: &str) -> FormatResult<Format> {
        match atom_parser::parse_atoms(definition) {
            Ok(atoms) => {
                match atoms[0] {
                    Atom::Literal(_) => Ok(Format { atoms: atoms }),
                    _ => return Err(FormatParseError::InvalidAtom(
                        "first atom must be literal".to_string()))
                }
            },
            Err(err) => Err(err)
        }
    }

    pub fn parse(&self, token: Token, input: &str) -> ValueResult<CommandPhrase> {
        let original_input: &str = input;
        let input: &str = input;
        let mut args_map: BTreeMap<String, Value> = BTreeMap::new();

        let command = match self.atoms[0] {
            Atom::Literal(ref literal) => literal.to_string(),
            _ => return Err(ValueParseError::Mismatch("first atom must be literal"))
        };
        let mut remaining = input;

        for atom in self.atoms.iter() {
            if remaining == "" {
                return Err(ValueParseError::MessageTooShort)
            }

            let value = match atom.consume(remaining) {
                Ok((Some(value), tmp)) => {
                    remaining = tmp;
                    value
                },
                Ok((None, tmp)) => {
                    remaining = tmp;
                    continue;
                },
                Err(err) => return Err(err)
            };
            let name = match *atom {
                Atom::Literal(_) => continue,
                Atom::Whitespace => continue,
                Atom::Formatted(ref name, _) => name.clone(),
                Atom::Rest(ref name) => name.clone(),
            };
            match value {
                Value::Literal(_) => (),
                Value::String(_) |  Value::WholeNumeric(_) => {
                    args_map.insert(name, value);
                },
            };
        }

        if !remaining.bytes().all(|x| x == b' ') {
            return Err(ValueParseError::MessageTooLong)
        }
        let cmd_phrase = CommandPhrase {
            token: token,
            command: command.trim_right_matches(' ').to_lowercase(),
            original_command: original_input.to_string(),
            args: args_map,
        };
        println!("{:?} - {:?} is consuming on {:?}", token, input, cmd_phrase);
        Ok(cmd_phrase)
    }
}

// use self::atom_parser::parse_atom;

pub mod atom_parser {
    use super::{Atom, AtomType, FormatResult, FormatParseError};

    static ASCII_ALPHANUMERIC: [u8; 62] = [
        b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9',

        b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J',
        b'K', b'L', b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T',
        b'U', b'V', b'W', b'X', b'Y', b'Z',

        b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j',
        b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's', b't',
        b'u', b'v', b'w', b'x', b'y', b'z'
    ];

    #[inline]
    fn is_ascii_alphanumeric(target: u8) -> bool {
        for &allowed in ASCII_ALPHANUMERIC.iter() {
            if target == allowed {
                return true;
            }
        }
        false
    }

    fn parse_var_atom(atom: &str) -> FormatResult<Atom> {
        let (name, format_spec) = match atom.find(':') {
            Some(idx) => (&atom[..idx], Some(&atom[1 + idx ..])),
            None => (atom, None)
        };
        let format_kind = match format_spec {
            Some("") => return Err(FormatParseError::InvalidAtom(
                "atom has empty format specifier".to_string())),
            Some("s") => AtomType::String,
            Some("d") => AtomType::WholeNumeric,
            Some(spec) => return Err(FormatParseError::InvalidAtom(
                format!("atom has unknown format specifier `{}'", spec))),
            None => AtomType::String
        };
        Ok(Atom::Formatted(name.to_string(), format_kind))
    }

    #[derive(Clone, Copy)]
    enum State {
        Zero,
        InLiteral,
        InWhitespace,
        InVariable,
        InRestVariable,
        ForceEnd,
        Errored,
    }

    struct AtomParser {
        byte_idx: usize,
        atoms: Vec<Atom>,
        state: State,
        cur_atom: Vec<u8>,
        error: Option<FormatParseError>
    }

    impl AtomParser {
        fn new() -> AtomParser {
            AtomParser {
                byte_idx: 0,
                atoms: Vec::new(),
                state: State::Zero,
                cur_atom: Vec::new(),
                error: None,
            }
        }

        fn finalize_literal(&mut self) {
            {
                // These should be fine unless we break parse_atom ...
                let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                self.atoms.push(Atom::Rest(string.into_owned()));
            }
            self.cur_atom.clear();
        }

        fn push_byte(&mut self, byte: u8) {
            use self::State::{
                Zero, InLiteral, InVariable, InRestVariable, ForceEnd, Errored, InWhitespace
            };

            let new_state = match (self.state, byte) {
                (Zero, b'{') => InVariable,
                (Zero, b' ') => InWhitespace,
                (Zero, cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InLiteral
                }

                (InVariable, b'}') => {
                    let atom_res = {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        parse_var_atom(&string)
                    };
                    match atom_res {
                        Ok(atom) => {
                            self.atoms.push(atom);
                            self.cur_atom.clear();
                            Zero
                        },
                        Err(err) => {
                            self.error = Some(err);
                            Errored
                        }
                    }
                },
                (InVariable, b'*') if self.cur_atom.len() == 0 => {
                    InRestVariable
                },
                (InVariable, b'*') => Errored,
                (InVariable, b':') if self.cur_atom.len() > 0 => {
                    self.cur_atom.push(b':');
                    InVariable
                },
                (InVariable, b':') => Errored,
                (InVariable, cur_byte) if is_ascii_alphanumeric(cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InVariable
                },
                (InVariable, _) => {
                    self.error = Some(FormatParseError::BrokenFormat);
                    Errored
                },

                (InRestVariable, b'}') => {
                    self.finalize_literal();
                    ForceEnd
                },
                (InRestVariable, cur_byte) if is_ascii_alphanumeric(cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InRestVariable
                },
                (InRestVariable, _) => {
                    self.error = Some(FormatParseError::BrokenFormat);
                    Errored
                },

                (InWhitespace, b' ') => {
                    InWhitespace
                },
                (InWhitespace, b'{') => {
                    assert_eq!(self.cur_atom.len(), 0);
                    self.atoms.push(Atom::Whitespace);
                    InVariable
                },
                (InWhitespace, cur_byte) => {
                    assert_eq!(self.cur_atom.len(), 0);
                    self.cur_atom.push(cur_byte);
                    InLiteral
                },

                (InLiteral, b' ') => {
                    {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        self.atoms.push(Atom::Literal(string.into_owned()));
                    }
                    self.cur_atom.clear();
                    InWhitespace
                },
                (InLiteral, b'{') => {
                    {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        self.atoms.push(Atom::Literal(string.into_owned()));
                    }
                    self.cur_atom.clear();
                    InVariable
                },
                (InLiteral, cur_byte)  => {
                    self.cur_atom.push(cur_byte);
                    InLiteral
                },

                (Errored, _) => Errored,
                (ForceEnd, _) => {
                    self.error = Some(FormatParseError::BrokenFormat);
                    Errored
                },
            };
            self.byte_idx += 1;
            self.state = new_state;
        }

        fn finish(mut self) -> FormatResult<Vec<Atom>> {
            match self.state {
                State::InLiteral => {
                    let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                    self.atoms.push(Atom::Literal(string.into_owned()));
                    Ok(self.atoms)
                },
                State::InWhitespace => {
                    self.atoms.pop();
                    Ok(self.atoms)
                },
                State::Zero | State::ForceEnd => Ok(self.atoms),
                State::Errored => Err(self.error.unwrap()),
                State::InVariable => Err(FormatParseError::BrokenFormat),
                State::InRestVariable => Err(FormatParseError::BrokenFormat),
            }
        }
    }

    pub fn parse_atoms(atom: &str) -> FormatResult<Vec<Atom>> {
        let mut parser = AtomParser::new();
        for &byte in atom.as_bytes().iter() {
            parser.push_byte(byte);
        }
        match parser.finish() {
            Ok(vec) => {
                if vec.len() == 0 {
                    return Err(FormatParseError::EmptyFormat)
                }
                Ok(vec)
            },
            Err(err) => Err(err),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::parse_atoms;
        use super::super::{Atom, AtomType};

        #[test]
        fn test_basics() {
            let atoms = parse_atoms("deer").ok().unwrap();
            assert_eq!(atoms, vec!(Atom::Literal("deer".to_string())));

            let atoms = parse_atoms("deer{a}").ok().unwrap();
            assert_eq!(atoms, vec!(
                Atom::Literal("deer".to_string()),
                Atom::Formatted("a".to_string(), AtomType::String),
            ));

            let atoms = parse_atoms("deer {a}").ok().unwrap();
            assert_eq!(atoms, vec!(
                Atom::Literal("deer".to_string()),
                Atom::Whitespace,
                Atom::Formatted("a".to_string(), AtomType::String),
            ));

            let atoms = parse_atoms("deer {a} {*b}").ok().unwrap();
            assert_eq!(atoms, vec!(
                Atom::Literal("deer".to_string()),
                Atom::Whitespace,
                Atom::Formatted("a".to_string(), AtomType::String),
                Atom::Whitespace,
                Atom::Rest("b".to_string()),
            ));

            assert!(parse_atoms("deer {a} {*b}xxx").is_err());

            match parse_atoms("deer {a:s} {*b}") {
                Ok(ok) => (),
                Err(err) => assert!(false, format!("{:?}", err))
            };
        }
    }
}


#[test]
fn cons_the_basics() {
    {
        let fmt_str = "articles {foo} {category:s} {id:d}";
        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {:?}", err)
        };

        assert_eq!(fmt.atoms.len(), 7);
        assert_eq!(fmt.atoms[0], Atom::Literal("articles".to_string()));
        assert_eq!(fmt.atoms[1], Atom::Whitespace);
        assert_eq!(
            fmt.atoms[2],
            Atom::Formatted("foo".to_string(), AtomType::String));
        assert_eq!(fmt.atoms[3], Atom::Whitespace);
        assert_eq!(
            fmt.atoms[4],
            Atom::Formatted("category".to_string(), AtomType::String));
        assert_eq!(fmt.atoms[5], Atom::Whitespace);
        assert_eq!(
            fmt.atoms[6],
            Atom::Formatted("id".to_string(), AtomType::WholeNumeric));
    }

    match Format::from_str("") {
        Ok(_) => panic!("empty string must not succeed"),
        Err(FormatParseError::EmptyFormat) => (),
        Err(err) => panic!("wrong error for empty: {:?}", err),
    };

    match Format::from_str("{category:s} articles") {
        Ok(_) => panic!("first atom must be literal"),
        Err(_) => ()
    };

    {
        let fmt_str = "articles {foo} {*rest}";
        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {:?}", err)
        };
        let cmdlet = match fmt.parse(Token(0), "articles bar test article argument") {
            Ok(cmdlet) => cmdlet,
            Err(err) => panic!("parse failure: {:?}", err)
        };
        assert_eq!(&cmdlet.command, "articles");
        assert_eq!(
            cmdlet.args["foo"],
            Value::String("bar".to_string()));
        assert_eq!(
            cmdlet.args["rest"],
            Value::String("test article argument".to_string()));
    }

}

#[test]
fn parse_the_basics() {
    {
        let cmd_str = "articles my_bar my_category 1234";
        let fmt_str = "articles {foo} {category:s} {id:d}";

        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {:?}", err)
        };

        assert!(fmt.parse(Token(0), "articles").is_err());
        let cmdlet = match fmt.parse(Token(0), cmd_str) {
            Ok(cmdlet) => cmdlet,
            Err(err) => panic!("parse failure: {:?}", err)
        };
        assert_eq!(&cmdlet.command, "articles");
        assert_eq!(
            cmdlet.get::<String>("foo"),
            Some("my_bar".to_string()));

        assert_eq!(
            cmdlet.get::<String>("category"),
            Some("my_category".to_string()));

        assert_eq!(cmdlet.get::<u64>("id"), Some(1234));
    }
    {
        match Format::from_str("") {
            Ok(_) => panic!("empty string must not succeed"),
            Err(FormatParseError::EmptyFormat) => (),
            Err(err) => panic!("wrong error for empty: {:?}", err),
        };
    }
    {
        let cmd_str = "articles ";
        let fmt_str = "articles";

        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {:?}", err)
        };
        if let Err(err) = fmt.parse(Token(0), cmd_str) {
            panic!("Error processing {:?} with {:?}: {:?}", cmd_str, fmt_str, err);
        }
    }
    {
        let cmd_str = "articlestest";
        let fmt_str = "articles {foo}";

        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {:?}", err)
        };
        match fmt.parse(Token(0), cmd_str) {
            Err(ValueParseError::Mismatch(_)) => (),
            p @ _ => panic!("{:?} should not parse. Got {:?}", cmd_str, p),
        };
    }
}
