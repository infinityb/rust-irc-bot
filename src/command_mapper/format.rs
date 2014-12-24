use std::string;
use std::collections::BTreeMap;

#[deriving(Show, PartialEq, Eq)]
pub enum FormatParseError {
    EmptyFormat,
    InvalidAtom(String),
    BrokenFormat,
}
pub type FormatResult<T> = Result<T, FormatParseError>;

#[deriving(Show, PartialEq, Eq)]
pub enum ValueParseError {
    Mismatch(&'static str),
    MessageTooShort,
    MessageTooLong,
}
pub type ValueResult<T> = Result<T, ValueParseError>;

#[deriving(Show, PartialEq, Eq, Clone, Copy)]
enum AtomType {
    Literal,
    String,
    WholeNumeric
}

#[deriving(Show, PartialEq, Eq, Clone)]
enum Atom {
    // Literal(value)
    Literal(string::String),
    // Formatted(name, kind)
    Formatted(string::String, AtomType),
    // Rest(name)
    Rest(string::String),
}

#[deriving(Show, PartialEq, Eq, Clone)]
enum Value {
    Literal(string::String),
    String(string::String),
    WholeNumeric(string::String)
}

impl Value {
    fn parse(kind: AtomType, input: &str) -> ValueResult<Value> {
        match kind {
            AtomType::Literal => Ok(Value::Literal(input.to_string())),
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
        Some(idx) => Ok((from[..idx], from[idx..])),
        None => Ok((from, ""))
    }
}

fn consume_literal<'a>(from: &'a str, literal: &str) -> ValueResult<(&'a str, &'a str)> {
    if from.starts_with(literal) {
        let length = literal.len();
        Ok((from[..length], from[length..]))
    } else {
        Err(ValueParseError::Mismatch("literal mismatch"))
    }
}

impl Atom {
    fn consume<'a>(&self, input: &'a str) -> ValueResult<(Value, &'a str)> {
        match *self {
            Atom::Literal(ref val) => {
                let (lit, rest) = try!(consume_literal(input, val[]));
                Ok((Value::Literal(lit.to_string()), rest))
            },
            Atom::Formatted(_, kind) => {
                let (lit, rest) = try!(consume_token(input));
                Ok((try!(Value::parse(kind, lit)), rest))
            },
            Atom::Rest(_) => {
                Ok((try!(Value::parse(AtomType::String, input)), ""))
            }
        }
    }
}

#[deriving(Show)]
pub struct Format {
    atoms: Vec<Atom>
}

#[deriving(Show, Clone)]
pub struct CommandPhrase {
    pub command: string::String,
    pub original_command: string::String,
    args: BTreeMap<string::String, Value>
}

impl CommandPhrase {
    pub fn get<T: ValueExtract>(&self, key: &str) -> Option<T> {
        match self.args.get(&key.to_string()) {
            Some(value) => ValueExtract::value_extract(value),
            None => None
        }
    }
}

trait ValueExtract {
    fn value_extract(val: &Value) -> Option<Self>;
}

impl ValueExtract for string::String {
    fn value_extract(val: &Value) -> Option<string::String> {
        match *val {
            Value::String(ref str_val) => Some(str_val.clone()),
            _ => None
        }
    }
}

impl ValueExtract for u64 {
    fn value_extract(val: &Value) -> Option<u64> {
        match *val {
            Value::WholeNumeric(ref str_val) => from_str(str_val[]),
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
                        "first atom must be literal".into_cow().into_owned()))
                }
            },
            Err(err) => Err(err)
        }
    }

    pub fn parse(&self, input: &str) -> ValueResult<CommandPhrase> {
        println!("{} is parsing <<{}>>", self, input);
        let original_input = input[];
        let input = input[];
        let mut args_map: BTreeMap<string::String, Value> = BTreeMap::new();

        let command = match self.atoms[0] {
            Atom::Literal(ref literal) => literal.to_string(),
            _ => return Err(ValueParseError::Mismatch("first atom must be literal"))
        };
        let mut remaining = input;

        for atom in self.atoms.iter() {

            if remaining == "" {
                return Err(ValueParseError::MessageTooShort)
            }
            println!("atom = {}, matching against ``{}``", atom, remaining);
            let value = match atom.consume(remaining) {
                Ok((value, tmp)) => {
                    remaining = tmp;
                    value
                },
                Err(err) => return Err(err)
            };
            let name = match *atom {
                Atom::Literal(_) => continue,
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
        if remaining != "" {
            return Err(ValueParseError::MessageTooLong)
        }
        Ok(CommandPhrase {
            command: command.trim_right_chars(' ').to_string(),
            original_command: original_input.to_string(),
            args: args_map,
        })
    }
}

// use self::atom_parser::parse_atom;

pub mod atom_parser {
    use super::{Atom, AtomType, FormatResult, FormatParseError};

    static ASCII_ALPHANUMERIC: [u8, ..62] = [
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
            Some(idx) => (atom[..idx], Some(atom[1 + idx ..])),
            None => (atom, None)
        };
        let format_kind = match format_spec {
            Some("") => return Err(FormatParseError::InvalidAtom(
                "atom has empty format specifier".into_cow().into_owned())),
            Some("s") => AtomType::String,
            Some("d") => AtomType::WholeNumeric,
            Some(spec) => return Err(FormatParseError::InvalidAtom(
                format!("atom has unknown format specifier `{}'", spec).into_cow().into_owned())),
            None => AtomType::String
        };
        Ok(Atom::Formatted(name.to_string(), format_kind))
    }

    #[deriving(Copy)]
    enum State {
        Zero,
        InLiteral,
        InVariable,
        InRestVariable,
        ForceEnd,
        Errored,
    }

    struct AtomParser {
        byte_idx: uint,
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

        fn push_byte(&mut self, byte: u8) {
            use self::State::{
                Zero, InLiteral, InVariable, InRestVariable, ForceEnd, Errored
            };

            let new_state = match (self.state, byte) {
                (State::Zero, b'{') => InVariable,
                (State::Zero, cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InLiteral
                }

                (State::InVariable, b'}') => {
                    let atom_res = {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        parse_var_atom(string[])
                    };
                    match atom_res {
                        Ok(atom) => {
                            self.atoms.push(atom);
                            self.cur_atom.clear();
                            Zero
                        },
                        Err(err) => {
                            self.error = Some(err);
                            println!("??? from InVariable");
                            State::Errored
                        }
                    }
                },
                (State::InVariable, b'*') if self.cur_atom.len() == 0 => {
                    InRestVariable
                },
                (State::InVariable, b':') if self.cur_atom.len() > 0 => {
                    self.cur_atom.push(b':');
                    InVariable
                },
                (State::InVariable, cur_byte) if is_ascii_alphanumeric(cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InVariable
                },
                (State::InVariable, _) => {
                    self.error = Some(FormatParseError::BrokenFormat);
                    println!("BrokenFormat from InVariable");
                    Errored
                },

                (State::InRestVariable, b'}') => {
                    {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        self.atoms.push(Atom::Rest(string.into_owned()));
                    }
                    self.cur_atom.clear();
                    ForceEnd
                },
                (State::InRestVariable, cur_byte) if is_ascii_alphanumeric(cur_byte) => {
                    self.cur_atom.push(cur_byte);
                    InRestVariable
                },
                (State::InRestVariable, _) => {
                    self.error = Some(FormatParseError::BrokenFormat);
                    println!("BrokenFormat from InRestVariable");
                    Errored
                },

                (State::InLiteral, b'{') => {
                    {
                        // These should be fine unless we break parse_atom ...
                        let string = String::from_utf8_lossy(self.cur_atom.as_slice());
                        self.atoms.push(Atom::Literal(string.into_owned()));
                    }
                    self.cur_atom.clear();
                    InVariable
                },
                (State::InLiteral, cur_byte)  => {
                    self.cur_atom.push(cur_byte);
                    InLiteral
                },
                
                (State::Errored, _) => State::Errored,
                (State::ForceEnd, _) => {
                    println!("BrokenFormat from ForceEnd");
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
                Atom::Literal("deer ".to_string()),
                Atom::Formatted("a".to_string(), AtomType::String),
            ));

            let atoms = parse_atoms("deer {a} {*b}").ok().unwrap();
            assert_eq!(atoms, vec!(
                Atom::Literal("deer ".to_string()),
                Atom::Formatted("a".to_string(), AtomType::String),
                Atom::Literal(" ".to_string()),
                Atom::Rest("b".to_string()),
            ));

            assert!(parse_atoms("deer {a} {*b}xxx").is_err());

            match parse_atoms("deer {a:s} {*b}") {
                Ok(ok) => (),
                Err(err) => assert!(false, format!("{}", err))
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
            Err(err) => panic!("parse failure: {}", err)
        };

        assert_eq!(fmt.atoms.len(), 6);
        assert_eq!(
            fmt.atoms[0],
            Atom::Literal("articles ".to_string()));
        assert_eq!(
            fmt.atoms[1],
            Atom::Formatted("foo".to_string(), AtomType::String));
        assert_eq!(
            fmt.atoms[2],
            Atom::Literal(" ".to_string()));
        assert_eq!(
            fmt.atoms[3],
            Atom::Formatted("category".to_string(), AtomType::String));
        assert_eq!(
            fmt.atoms[4],
            Atom::Literal(" ".to_string()));
        assert_eq!(
            fmt.atoms[5],
            Atom::Formatted("id".to_string(), AtomType::WholeNumeric));
    }
    
    match Format::from_str("") {
        Ok(_) => panic!("empty string must not succeed"),
        Err(FormatParseError::EmptyFormat) => (),
        Err(err) => panic!("wrong error for empty: {}", err),
    };
    
    match Format::from_str("{category:s} articles") {
        Ok(_) => panic!("first atom must be literal"),
        Err(_) => ()
    };
    
    {
        let fmt_str = "articles {foo} {*rest}";
        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {}", err)
        };
        let cmdlet = match fmt.parse("articles bar test article argument") {
            Ok(cmdlet) => cmdlet,
            Err(err) => panic!("parse failure: {}", err)
        };
        assert_eq!(cmdlet.command[], "articles");
        assert_eq!(
            cmdlet.args["foo".to_string()],
            Value::String("bar".to_string()));
        assert_eq!(
            cmdlet.args["rest".to_string()],
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
            Err(err) => panic!("parse failure: {}", err)
        };

        assert!(fmt.parse("articles").is_err());
        let cmdlet = match fmt.parse(cmd_str) {
            Ok(cmdlet) => cmdlet,
            Err(err) => panic!("parse failure: {}", err)
        };
        assert_eq!(cmdlet.command[], "articles");
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
            Err(err) => panic!("wrong error for empty: {}", err),
        };
    }
}
