use std::string;
use std::str::MaybeOwned;
use std::collections::TreeMap;


#[deriving(Show, PartialEq, Eq)]
pub enum FormatParseError {
    EmptyFormat,
    InvalidAtom(MaybeOwned<'static>)
}
pub type FormatResult<T> = Result<T, FormatParseError>;

#[deriving(Show, PartialEq, Eq)]
pub enum ValueParseError {
    BadFormat,
}
pub type ValueResult<T> = Result<T, ValueParseError>;

#[deriving(Show, PartialEq, Eq, Clone, Copy)]
pub enum FormatKind {
    Unspecified,
    String,
    WholeNumeric
}

#[deriving(Show, PartialEq, Eq, Clone)]
pub enum Atom {
    // LiteralAtom(value)
    LiteralAtom(string::String),
    // FormattedAtom(name, kind)
    FormattedAtom(string::String, FormatKind),
    // RestAtom(name)
    RestAtom(string::String),
}

#[deriving(Show, PartialEq, Eq, Clone)]
pub enum Value {
    StringValue(string::String),
    WholeNumericValue(u64)
}

impl Value {
    fn parse(kind: FormatKind, input: &str) -> ValueResult<Value> {
        match kind {
            Unspecified => Ok(StringValue(input.to_string())),
            String => Ok(StringValue(input.to_string())),
            WholeNumeric => match from_str(input) {
                Some(val) => Ok(WholeNumericValue(val)),
                None => Err(BadFormat)
            }
        }
    }
}


impl Atom {
    pub fn get_name<'a>(&'a self) -> Option<&'a str> {
        match *self {
            LiteralAtom(_) => None,
            FormattedAtom(ref name, _) => Some(name[]),
            RestAtom(ref name) => Some(name[]),
        }
    }

    fn parse(&self, input: &str) -> FormatResult<Option<Value>> {
        let value_res = match *self {
            LiteralAtom(_) => None,
            FormattedAtom(_, kind) => Some(Value::parse(kind, input)),
            RestAtom(_) => Some(Value::parse(String, input)),
        };
        match (self.get_name(), value_res) {
            (Some(_), Some(Ok(value))) => {
                Ok(Some(value))
            },
            (Some(name), Some(Err(val_err))) => {
                Err(InvalidAtom(format!(
                    "atom {} invalid: {}",
                    name[], val_err
                ).into_maybe_owned()))
            },
            (Some(_), None) => unreachable!(),
            (None, _) => Ok(None)
        }
    }
}

#[deriving(Show)]
pub struct Format {
    atoms: Vec<Atom>
}

impl Format {
    pub fn get_command<'a>(&'a self) -> &'a str {
        match self.atoms[0] {
            LiteralAtom(ref literal) => literal[],
            _ => fail!("Malformed Format")
        }
    }
}

#[deriving(Show, Clone)]
pub struct CommandPhrase {
    pub command: string::String,
    pub original_command: string::String,
    pub args: TreeMap<string::String, Value>
}

#[inline]
fn get_token<'a>(input: &'a str) -> (&'a str, &'a str) {
    match input.find(' ') {
        Some(idx) => {
            let mut rest = input[idx+1..];
            while rest.starts_with(" ") {
                rest = rest[1..];
            }
            (input[..idx], rest)
        },
        None => (input, input[input.len()..])
    }
}

impl Format {
    // If true, this Format may (or may not) match the given input.
    // If false, this Format definitely does not match the given input.
    pub fn matches_maybe(&self, input: &str) -> bool {
        let (command, _) = get_token(input);
        command == self.get_command()
    }

    pub fn from_str(definition: &str) -> FormatResult<Format> {
        if definition == "" {
            return Err(EmptyFormat)
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
            _ => return Err(InvalidAtom(
                "first atom must be literal".into_maybe_owned()))
        };
        Ok(format)
    }

    pub fn parse(&self, input: &str) -> FormatResult<Option<CommandPhrase>> {
        let original_input = input[];
        let mut input = input[];
        let mut args_map: TreeMap<string::String, Value> = TreeMap::new();

        let command = match self.atoms[0] {
            LiteralAtom(ref literal) => {
                literal.to_string()
            },
            _ => fail!("Malformed Format")
        };

        for atom in self.atoms.iter() {
            match atom {
                &RestAtom(ref name) => {
                    args_map.insert(
                        name.to_string(),
                        StringValue(input.to_string()));
                },
                &LiteralAtom(ref value) => {
                    let (part, input_tmp) = get_token(input);
                    if part != value[] {
                        return Ok(None);
                    }
                    input = input_tmp;
                },
                _ => {
                    let (part, input_tmp) = get_token(input);
                    match (atom.get_name(), atom.parse(part)) {
                        (Some(name), Ok(Some(value))) => {
                            args_map.insert(name.to_string(), value);
                        },
                        (Some(name), Ok(None)) => {
                            fail!("named ({}) atom with Ok(None) value", name);
                        },
                        (None, Ok(_)) => (),
                        (_, Err(err)) => return Err(err)
                    };
                    input = input_tmp;
                }
            }
            
        }        
        Ok(Some(CommandPhrase {
            command: command,
            original_command: original_input.to_string(),
            args: args_map,
        }))
    }
}

fn parse_atom(atom: &str) -> FormatResult<Atom> {
    if atom.starts_with("{") {
        if !atom.ends_with("}") {
            return Err(InvalidAtom(
                "atom begins with { but doesn't end with }".into_maybe_owned()));
        }
        let atom = atom[1..atom.len()-1];

        let (name, format_spec) = match atom.find(':') {
            Some(idx) => (atom[..idx], Some(atom[1 + idx ..])),
            None => (atom, None)
        };
        let format_kind = match format_spec {
            Some("") => return Err(
                InvalidAtom("atom has empty format specifier".into_maybe_owned())),
            Some("s") => String,
            Some("d") => WholeNumeric,
            Some(spec) => return Err(InvalidAtom(
                format!("atom has unknown format specifier `{}'", spec).into_maybe_owned())),
            None => Unspecified
        };
        if name.starts_with("*") {
            if format_kind != Unspecified {
                return Err(InvalidAtom(
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
            Err(err) => fail!("parse failure: {}", err)
        };

        assert_eq!(fmt.atoms.len(), 4);
        assert_eq!(
            fmt.atoms[0],
            LiteralAtom("articles".to_string()));
        assert_eq!(
            fmt.atoms[1],
            FormattedAtom("foo".to_string(), Unspecified));
        assert_eq!(
            fmt.atoms[2],
            FormattedAtom("category".to_string(), String));
        assert_eq!(
            fmt.atoms[3],
            FormattedAtom("id".to_string(), WholeNumeric));
    }
    
    match Format::from_str("") {
        Ok(_) => fail!("empty string must not succeed"),
        Err(EmptyFormat) => (),
        Err(err) => fail!("wrong error for empty: {}", err),
    };
    
    match Format::from_str("{category:s} articles") {
        Ok(_) => fail!("first atom must be literal"),
        Err(_) => ()
    };
    
    {
        let fmt_str = "articles {foo} {*rest}";
        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => fail!("parse failure: {}", err)
        };
        let cmdlet = match fmt.parse("articles bar test article argument") {
            Ok(Some(cmdlet)) => cmdlet,
            Ok(None) => fail!("doesn't match when it should"),
            Err(err) => fail!("parse failure: {}", err)
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
        let cmd_str = "articles my_bar my_category 01234";
        let fmt_str = "articles {foo} {category:s} {id:d}";

        let fmt = match Format::from_str(fmt_str) {
            Ok(fmt) => fmt,
            Err(err) => fail!("parse failure: {}", err)
        };

        assert!(fmt.matches_maybe(cmd_str));

        let cmdlet = match fmt.parse(cmd_str) {
            Ok(Some(cmdlet)) => cmdlet,
            Ok(None) => fail!("doesn't match when it should"),
            Err(err) => fail!("parse failure: {}", err)
        };
        assert_eq!(cmdlet.command[], "articles");
        assert!(cmdlet.args.contains_key(&"foo".to_string()));
        assert!(cmdlet.args.contains_key(&"foo".to_string()));

        assert_eq!(
            cmdlet.args["foo".to_string()],
            StringValue("my_bar".to_string()));

        assert_eq!(
            cmdlet.args["category".to_string()],
            StringValue("my_category".to_string()));
    }
    {
        match Format::from_str("") {
            Ok(_) => fail!("empty string must not succeed"),
            Err(EmptyFormat) => (),
            Err(err) => fail!("wrong error for empty: {}", err),
        };
    }
}