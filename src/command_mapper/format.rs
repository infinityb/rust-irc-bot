use std::string;
use std::str::MaybeOwned;
use std::collections::TreeMap;
use self::Atom::{LiteralAtom, FormattedAtom, RestAtom};
use self::Value::{LiteralValue, StringValue, WholeNumericValue};
use self::AtomType::{LiteralAtomType, StringAtomType, WholeNumericAtomType};

#[deriving(Show, PartialEq, Eq)]
pub enum FormatParseError {
    EmptyFormat,
    InvalidAtom(MaybeOwned<'static>)
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
    LiteralAtomType,
    StringAtomType,
    WholeNumericAtomType
}

#[deriving(Show, PartialEq, Eq, Clone)]
enum Atom {
    // LiteralAtom(value)
    LiteralAtom(string::String),
    // FormattedAtom(name, kind)
    FormattedAtom(string::String, AtomType),
    // RestAtom(name)
    RestAtom(string::String),
}

#[deriving(Show, PartialEq, Eq, Clone)]
enum Value {
    LiteralValue(string::String),
    StringValue(string::String),
    WholeNumericValue(string::String)
}

impl Value {
    fn parse(kind: AtomType, input: &str) -> ValueResult<Value> {
        match kind {
            LiteralAtomType => Ok(LiteralValue(input.to_string())),
            StringAtomType => Ok(StringValue(input.to_string())),
            WholeNumericAtomType => {
                // TODO: check if it is a numberish thing
                Ok(WholeNumericValue(input.to_string()))
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
            LiteralAtom(ref val) => {
                let (lit, rest) = try!(consume_literal(input, val[]));
                Ok((LiteralValue(lit.to_string()), rest))
            },
            FormattedAtom(_, kind) => {
                let (lit, rest) = try!(consume_token(input));
                Ok((try!(Value::parse(kind, lit)), rest))
            },
            RestAtom(_) => {
                Ok((try!(Value::parse(StringAtomType, input)), ""))
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
    args: TreeMap<string::String, Value>
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
            StringValue(ref str_val) => Some(str_val.clone()),
            _ => None
        }
    }
}

impl ValueExtract for u64 {
    fn value_extract(val: &Value) -> Option<u64> {
        match *val {
            WholeNumericValue(ref str_val) => from_str(str_val[]),
            _ => None
        }
    }
}

impl Format {
    pub fn from_str(definition: &str) -> FormatResult<Format> {
        if definition == "" {
            return Err(FormatParseError::EmptyFormat)
        }
        let mut format = Format { atoms: vec![] };
        for node in definition.split(' ') {
            match parse_atom(node) {
                Ok(atom) => format.atoms.push(atom),
                Err(err) => return Err(err)
            };
        }
        match format.atoms[0] {
            LiteralAtom(ref literal) => {
                literal.to_string()
            },
            _ => return Err(FormatParseError::InvalidAtom(
                "first atom must be literal".into_maybe_owned()))
        };
        Ok(format)
    }

    pub fn parse(&self, input: &str) -> ValueResult<CommandPhrase> {
        println!("{} is parsing <<{}>>", self, input);
        let original_input = input[];
        let input = input[];
        let mut args_map: TreeMap<string::String, Value> = TreeMap::new();

        let command = match self.atoms[0] {
            LiteralAtom(ref literal) => literal.to_string(),
            _ => return Err(ValueParseError::Mismatch("first atom must be literal"))
        };
        let mut remaining = input;

        for atom in self.atoms.iter() {
            if remaining == "" {
                return Err(ValueParseError::MessageTooShort)
            }
            println!("atom = {}, matching against {}", atom, remaining);
            let value = match atom.consume(remaining) {
                Ok((value, tmp)) => {
                    remaining = tmp;
                    value
                },
                Err(err) => return Err(err)
            };
            remaining = remaining.trim_left_chars(' ');
            let name = match *atom {
                LiteralAtom(_) => continue,
                FormattedAtom(ref name, _) => name.clone(),
                RestAtom(ref name) => name.clone(),
            };
            match value {
                LiteralValue(_) => (),
                StringValue(_) | WholeNumericValue(_) => {
                    args_map.insert(name, value);
                },
            };
        }
        if remaining != "" {
            return Err(ValueParseError::MessageTooLong)
        }
        Ok(CommandPhrase {
            command: command,
            original_command: original_input.to_string(),
            args: args_map,
        })
    }
}

fn parse_atom(atom: &str) -> FormatResult<Atom> {
    if atom.starts_with("{") {
        if !atom.ends_with("}") {
            return Err(FormatParseError::InvalidAtom(
                "atom begins with { but doesn't end with }".into_maybe_owned()));
        }
        let atom = atom[1..atom.len()-1];

        let (name, format_spec) = match atom.find(':') {
            Some(idx) => (atom[..idx], Some(atom[1 + idx ..])),
            None => (atom, None)
        };
        let format_kind = match format_spec {
            Some("") => return Err(FormatParseError::InvalidAtom(
                "atom has empty format specifier".into_maybe_owned())),
            Some("s") => StringAtomType,
            Some("d") => WholeNumericAtomType,
            Some(spec) => return Err(FormatParseError::InvalidAtom(
                format!("atom has unknown format specifier `{}'", spec).into_maybe_owned())),
            None => StringAtomType
        };
        if name.starts_with("*") {
            if format_kind != StringAtomType {
                return Err(FormatParseError::InvalidAtom(
                    "format specifier not allowed on *atom".into_maybe_owned()));
            }
            return Ok(RestAtom(name[1..].to_string()));
        }
        return Ok(FormattedAtom(name.to_string(), format_kind));
    }
    Ok(LiteralAtom(atom.to_string()))
}


#[test]
fn cons_the_basics() {
    {
        let fmt_str = "articles {foo} {category:s} {id:d}";
        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => panic!("parse failure: {}", err)
        };

        assert_eq!(fmt.atoms.len(), 4);
        assert_eq!(
            fmt.atoms[0],
            LiteralAtom("articles".to_string()));
        assert_eq!(
            fmt.atoms[1],
            FormattedAtom("foo".to_string(), StringAtomType));
        assert_eq!(
            fmt.atoms[2],
            FormattedAtom("category".to_string(), StringAtomType));
        assert_eq!(
            fmt.atoms[3],
            FormattedAtom("id".to_string(), WholeNumericAtomType));
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
            StringValue("bar".to_string()));
        assert_eq!(
            cmdlet.args["rest".to_string()],
            StringValue("test article argument".to_string()));
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