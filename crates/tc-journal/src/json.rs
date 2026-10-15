//! Canonical JSON (`docs/STORAGE.md` §2.5): the one byte form of every change.
//!
//! - Object keys sorted by their UTF-8 bytes, no key twice, no whitespace.
//! - Integers only, from −2⁶³ to 2⁶⁴−1, written without leading zeros, `+` or `-0`.
//! - Strings exactly as given. Escapes are `\"`, `\\`, `\b`, `\f`, `\n`, `\r`, `\t`, and
//!   `\u00xx` (lowercase hex) for the other characters below U+0020; every other character
//!   is raw UTF-8.
//! - At most [`MAX_DEPTH`] nested arrays and objects.
//!
//! [`Json`] cannot hold a float, so every value it holds has exactly one canonical text,
//! and [`parse`] accepts exactly the texts [`Json::to_canonical`] writes.

use std::collections::BTreeMap;
use std::fmt;

/// The deepest nesting of arrays and objects a change may hold (rule V7).
pub const MAX_DEPTH: usize = 64;

/// A JSON value with integers only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Json {
    Null,
    Bool(bool),
    Int(JsonInt),
    Str(String),
    Arr(Vec<Json>),
    Obj(JsonMap),
}

/// An object. Iteration order is the canonical key order.
pub type JsonMap = BTreeMap<String, Json>;

/// An integer from −2⁶³ to 2⁶⁴−1: every `i64` and every `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonInt(i128);

impl JsonInt {
    pub const MIN: i128 = i64::MIN as i128;
    pub const MAX: i128 = u64::MAX as i128;

    pub fn new(value: i128) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
    }

    pub fn get(self) -> i128 {
        self.0
    }
}

macro_rules! int_from {
    ($($t:ty),*) => {$(
        impl From<$t> for JsonInt {
            fn from(value: $t) -> Self {
                Self(i128::from(value))
            }
        }
        impl From<$t> for Json {
            fn from(value: $t) -> Self {
                Json::Int(JsonInt::from(value))
            }
        }
    )*};
}
int_from!(u8, u16, u32, u64, i8, i16, i32, i64);

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Json::Str(value.to_owned())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Self {
        Json::Str(value)
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Json::Bool(value)
    }
}

impl Json {
    /// The canonical bytes of this value.
    pub fn to_canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }

    /// The canonical text of this value.
    pub fn to_canonical_string(&self) -> String {
        let mut out = String::new();
        self.write_str(&mut out);
        out
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        let mut text = String::new();
        self.write_str(&mut text);
        out.extend_from_slice(text.as_bytes());
    }

    fn write_str(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(n) => out.push_str(&n.0.to_string()),
            Json::Str(s) => write_string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_str(out);
                }
                out.push(']');
            }
            Json::Obj(map) => {
                out.push('{');
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write_str(out);
                }
                out.push('}');
            }
        }
    }

    /// The nesting depth: 0 for a scalar, 1 for an array or object of scalars.
    pub fn depth(&self) -> usize {
        match self {
            Json::Arr(items) => 1 + items.iter().map(Json::depth).max().unwrap_or(0),
            Json::Obj(map) => 1 + map.values().map(Json::depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&JsonMap> {
        match self {
            Json::Obj(map) => Some(map),
            _ => None,
        }
    }

    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "a boolean",
            Json::Int(_) => "an integer",
            Json::Str(_) => "a string",
            Json::Arr(_) => "an array",
            Json::Obj(_) => "an object",
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Why bytes are not canonical JSON.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, thiserror::Error)]
#[error("byte {at}: {problem}")]
pub struct JsonError {
    pub at: usize,
    pub problem: JsonProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, thiserror::Error)]
pub enum JsonProblem {
    #[error("not UTF-8")]
    NotUtf8,
    #[error("the text ends too early")]
    UnexpectedEnd,
    #[error("unexpected {0:?}")]
    Unexpected(char),
    #[error("text after the value")]
    TrailingText,
    #[error("a float; the format holds integers only")]
    Float,
    #[error("an integer out of range or not in canonical form")]
    Integer,
    #[error("an escape that canonical JSON does not write")]
    Escape,
    #[error("a control character that canonical JSON escapes")]
    RawControl,
    #[error("object keys out of order or repeated")]
    KeyOrder,
    #[error("nested deeper than {MAX_DEPTH} levels")]
    TooDeep,
}

/// Parses canonical JSON. Anything [`Json::to_canonical`] would not write is refused.
pub fn parse(bytes: &[u8]) -> Result<Json, JsonError> {
    let text = std::str::from_utf8(bytes).map_err(|e| JsonError {
        at: e.valid_up_to(),
        problem: JsonProblem::NotUtf8,
    })?;
    let mut parser = Parser { text, at: 0 };
    let value = parser.value(0)?;
    if parser.at != text.len() {
        return Err(parser.error(JsonProblem::TrailingText));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn error(&self, problem: JsonProblem) -> JsonError {
        JsonError {
            at: self.at,
            problem,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn unexpected(&self) -> JsonError {
        match self.text[self.at..].chars().next() {
            Some(c) => self.error(JsonProblem::Unexpected(c)),
            None => self.error(JsonProblem::UnexpectedEnd),
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.unexpected())
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'-' | b'0'..=b'9') => self.integer(),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            _ => Err(self.unexpected()),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(JsonProblem::TooDeep));
        }
        self.expect(b'{')?;
        let mut map = JsonMap::new();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Obj(map));
        }
        let mut previous: Option<String> = None;
        loop {
            let key_at = self.at;
            let key = self.string()?;
            if previous
                .as_ref()
                .is_some_and(|p| p.as_bytes() >= key.as_bytes())
            {
                return Err(JsonError {
                    at: key_at,
                    problem: JsonProblem::KeyOrder,
                });
            }
            self.expect(b':')?;
            let value = self.value(depth)?;
            previous = Some(key.clone());
            map.insert(key, value);
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Obj(map));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(JsonProblem::TooDeep));
        }
        self.expect(b'[')?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth)?);
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            let Some(stop) = rest.find(|c: char| c == '"' || c == '\\' || u32::from(c) < 0x20)
            else {
                self.at = self.text.len();
                return Err(self.error(JsonProblem::UnexpectedEnd));
            };
            out.push_str(&rest[..stop]);
            self.at += stop;
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => out.push(self.escape()?),
                _ => return Err(self.error(JsonProblem::RawControl)),
            }
        }
    }

    fn escape(&mut self) -> Result<char, JsonError> {
        let start = self.at;
        let bytes = self.text.as_bytes();
        let fail = || JsonError {
            at: start,
            problem: JsonProblem::Escape,
        };
        let c = match bytes.get(start + 1) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                let hex = bytes.get(start + 2..start + 6).ok_or_else(fail)?;
                if &hex[..2] != b"00" {
                    return Err(fail());
                }
                let digit = |b: u8| match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    _ => None,
                };
                let value =
                    (digit(hex[2]).ok_or_else(fail)? << 4) | digit(hex[3]).ok_or_else(fail)?;
                let c = char::from(value);
                if value >= 0x20 || matches!(c, '\u{8}' | '\u{c}' | '\n' | '\r' | '\t') {
                    return Err(fail());
                }
                self.at = start + 6;
                return Ok(c);
            }
            _ => return Err(fail()),
        };
        self.at = start + 2;
        Ok(c)
    }

    fn integer(&mut self) -> Result<Json, JsonError> {
        let start = self.at;
        let bytes = self.text.as_bytes();
        let negative = bytes[start] == b'-';
        let digits_start = start + usize::from(negative);
        let mut end = digits_start;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        let fail = |problem| JsonError { at: start, problem };
        if matches!(bytes.get(end), Some(b'.' | b'e' | b'E')) {
            return Err(fail(JsonProblem::Float));
        }
        let digits = &self.text[digits_start..end];
        let canonical = match digits.as_bytes() {
            [] => false,
            [b'0'] => !negative,
            [b'0', ..] => false,
            _ => true,
        };
        if !canonical || digits.len() > 20 {
            return Err(fail(JsonProblem::Integer));
        }
        let magnitude: i128 = digits.parse().map_err(|_| fail(JsonProblem::Integer))?;
        let value = if negative { -magnitude } else { magnitude };
        let value = JsonInt::new(value).ok_or_else(|| fail(JsonProblem::Integer))?;
        self.at = end;
        Ok(Json::Int(value))
    }
}

/// A JSON value from outside the journal that this format cannot hold: a float, or an
/// integer outside −2⁶³ to 2⁶⁴−1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the number {number} is not an integer the journal can hold")]
pub struct NotAnInteger {
    pub number: String,
}

impl TryFrom<&serde_json::Value> for Json {
    type Error = NotAnInteger;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        Ok(match value {
            serde_json::Value::Null => Json::Null,
            serde_json::Value::Bool(b) => Json::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Json::from(i)
                } else if let Some(u) = n.as_u64() {
                    Json::from(u)
                } else {
                    return Err(NotAnInteger {
                        number: n.to_string(),
                    });
                }
            }
            serde_json::Value::String(s) => Json::Str(s.clone()),
            serde_json::Value::Array(items) => {
                Json::Arr(items.iter().map(Json::try_from).collect::<Result<_, _>>()?)
            }
            serde_json::Value::Object(map) => Json::Obj(
                map.iter()
                    .map(|(k, v)| Ok((k.clone(), Json::try_from(v)?)))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

impl From<&Json> for serde_json::Value {
    fn from(value: &Json) -> Self {
        match value {
            Json::Null => serde_json::Value::Null,
            Json::Bool(b) => serde_json::Value::Bool(*b),
            Json::Int(n) => match i64::try_from(n.0) {
                Ok(i) => serde_json::Value::from(i),
                // JsonInt holds at most u64::MAX.
                Err(_) => serde_json::Value::from(u64::try_from(n.0).unwrap_or(u64::MAX)),
            },
            Json::Str(s) => serde_json::Value::String(s.clone()),
            Json::Arr(items) => serde_json::Value::Array(items.iter().map(Into::into).collect()),
            Json::Obj(map) => {
                serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), v.into())).collect())
            }
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(text: &str) {
        let value = parse(text.as_bytes()).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(value.to_canonical_string(), text);
    }

    #[test]
    fn canonical_texts_round_trip() {
        for text in [
            "null",
            "true",
            "0",
            "-1",
            "18446744073709551615",
            "-9223372036854775808",
            "\"\"",
            "\"a\\\"b\\\\c\\b\\f\\n\\r\\t\\u0000\\u001f\u{7f}é\u{2028}/\"",
            "[]",
            "{}",
            "{\"a\":[1,{\"b\":null}],\"b\":\"x\"}",
            "{\"Z\":1,\"a\":2,\"é\":3}",
        ] {
            round(text);
        }
    }

    #[test]
    fn everything_else_is_refused() {
        for (text, problem) in [
            (" 1", JsonProblem::Unexpected(' ')),
            ("1 ", JsonProblem::TrailingText),
            ("[1, 2]", JsonProblem::Unexpected(' ')),
            ("1.0", JsonProblem::Float),
            ("1e3", JsonProblem::Float),
            ("01", JsonProblem::Integer),
            ("-0", JsonProblem::Integer),
            ("+1", JsonProblem::Unexpected('+')),
            ("18446744073709551616", JsonProblem::Integer),
            ("-9223372036854775809", JsonProblem::Integer),
            ("\"\\u0041\"", JsonProblem::Escape),
            ("\"\\u000A\"", JsonProblem::Escape),
            ("\"\\u000a\"", JsonProblem::Escape),
            ("\"\\/\"", JsonProblem::Escape),
            ("\"\n\"", JsonProblem::RawControl),
            ("{\"b\":1,\"a\":2}", JsonProblem::KeyOrder),
            ("{\"a\":1,\"a\":2}", JsonProblem::KeyOrder),
            ("\"abc", JsonProblem::UnexpectedEnd),
            ("[", JsonProblem::UnexpectedEnd),
            ("nul", JsonProblem::Unexpected('n')),
        ] {
            let error = parse(text.as_bytes()).unwrap_err();
            assert_eq!(error.problem, problem, "{text:?}");
        }
        assert_eq!(
            parse(b"\"\xff\"").unwrap_err().problem,
            JsonProblem::NotUtf8
        );
    }

    #[test]
    fn depth_is_limited_to_64() {
        let deep = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(deep(MAX_DEPTH).as_bytes()).is_ok());
        assert_eq!(
            parse(deep(MAX_DEPTH + 1).as_bytes()).unwrap_err().problem,
            JsonProblem::TooDeep
        );
        assert_eq!(
            parse(deep(MAX_DEPTH).as_bytes()).unwrap().depth(),
            MAX_DEPTH
        );
        let deep_obj = format!("{}1{}", "{\"a\":".repeat(65), "}".repeat(65));
        assert_eq!(
            parse(deep_obj.as_bytes()).unwrap_err().problem,
            JsonProblem::TooDeep
        );
    }

    #[test]
    fn serde_values_convert_without_floats() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"b":[1,-2,18446744073709551615],"a":{"c":null,"d":true}}"#)
                .unwrap();
        let json = Json::try_from(&value).unwrap();
        assert_eq!(
            json.to_canonical_string(),
            r#"{"a":{"c":null,"d":true},"b":[1,-2,18446744073709551615]}"#
        );
        assert_eq!(serde_json::Value::from(&json), value);
        let float: serde_json::Value = serde_json::from_str("[0.5]").unwrap();
        assert!(Json::try_from(&float).is_err());
    }
}
