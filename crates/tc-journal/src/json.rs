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

/// An object: its members sorted by key, each key once, so iteration order is the
/// canonical key order. A sorted vector rather than a tree: the objects of a change are
/// small, and parsing canonical JSON appends each member in order, where a tree allocates
/// a node of eleven slots for every object.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct JsonMap(Vec<(String, Json)>);

type Member = (String, Json);

/// The members of an object in key order, as `(key, value)` pairs.
pub type Members<'a> =
    std::iter::Map<std::slice::Iter<'a, Member>, fn(&Member) -> (&String, &Json)>;

fn member_ref((key, value): &Member) -> (&String, &Json) {
    (key, value)
}

impl JsonMap {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn find(&self, key: &str) -> Result<usize, usize> {
        self.0.binary_search_by(|(k, _)| k.as_str().cmp(key))
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        self.find(key).ok().map(|at| &self.0[at].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        let at = self.find(key).ok()?;
        Some(&mut self.0[at].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.find(key).is_ok()
    }

    /// Sets `key` to `value` and returns the value it replaces.
    pub fn insert(&mut self, key: String, value: Json) -> Option<Json> {
        match self.find(&key) {
            Ok(at) => Some(std::mem::replace(&mut self.0[at].1, value)),
            Err(at) => {
                self.0.insert(at, (key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Json> {
        let at = self.find(key).ok()?;
        Some(self.0.remove(at).1)
    }

    /// The value at `key`, inserted from `make` when there is none.
    pub fn get_or_insert_with(&mut self, key: String, make: impl FnOnce() -> Json) -> &mut Json {
        let at = match self.find(&key) {
            Ok(at) => at,
            Err(at) => {
                self.0.insert(at, (key, make()));
                at
            }
        };
        &mut self.0[at].1
    }

    pub fn iter(&self) -> Members<'_> {
        self.0
            .iter()
            .map(member_ref as fn(&Member) -> (&String, &Json))
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = (&String, &mut Json)> {
        self.0.iter_mut().map(|(k, v)| (&*k, v))
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &Json> + ExactSizeIterator {
        self.0.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Json> {
        self.0.iter_mut().map(|(_, v)| v)
    }

    pub fn first_key_value(&self) -> Option<(&String, &Json)> {
        self.0.first().map(member_ref)
    }

    pub fn last_key_value(&self) -> Option<(&String, &Json)> {
        self.0.last().map(member_ref)
    }

    /// Keeps the members `keep` says to keep.
    pub fn retain(&mut self, mut keep: impl FnMut(&String, &mut Json) -> bool) {
        self.0.retain_mut(|(k, v)| keep(k, v));
    }
}

impl fmt::Debug for JsonMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Like a `BTreeMap`: members sorted by key, and of two members with one key the later
/// one stays. Sorting once makes building a large object from unsorted members
/// `O(n log n)`, not `O(n²)`.
impl FromIterator<(String, Json)> for JsonMap {
    fn from_iter<I: IntoIterator<Item = (String, Json)>>(iter: I) -> Self {
        let mut members: Vec<Member> = iter.into_iter().collect();
        members.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out: Vec<Member> = Vec::with_capacity(members.len());
        for member in members {
            match out.last_mut() {
                Some(last) if last.0 == member.0 => *last = member,
                _ => out.push(member),
            }
        }
        Self(out)
    }
}

impl<const N: usize> From<[(String, Json); N]> for JsonMap {
    fn from(members: [(String, Json); N]) -> Self {
        members.into_iter().collect()
    }
}

impl Extend<(String, Json)> for JsonMap {
    fn extend<I: IntoIterator<Item = (String, Json)>>(&mut self, iter: I) {
        let members = std::mem::take(&mut self.0);
        *self = members.into_iter().chain(iter).collect();
    }
}

impl IntoIterator for JsonMap {
    type Item = (String, Json);
    type IntoIter = std::vec::IntoIter<(String, Json)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a JsonMap {
    type Item = (&'a String, &'a Json);
    type IntoIter = Members<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

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
        let mut text = String::with_capacity(256);
        self.write_str(&mut text);
        out.extend_from_slice(text.as_bytes());
    }

    pub(crate) fn write_str(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(n) => write_int(n.0, out),
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
            Json::Obj(map) => write_map(map, out),
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

/// Writes an object in canonical form.
pub(crate) fn write_map(map: &JsonMap, out: &mut String) {
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

pub(crate) fn write_string(s: &str, out: &mut String) {
    out.push('"');
    let bytes = s.as_bytes();
    let mut from = 0;
    loop {
        // `plain_run` stops only at ASCII bytes, so every slice ends on a character boundary.
        let to = plain_run(bytes, from);
        out.push_str(s.get(from..to).unwrap_or_default());
        let Some(&b) = bytes.get(to) else {
            break;
        };
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            _ => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(char::from(HEX[usize::from(b >> 4)]));
                out.push(char::from(HEX[usize::from(b & 0xf)]));
            }
        }
        from = to + 1;
    }
    out.push('"');
}

/// Writes an integer in decimal without going through `fmt`.
pub(crate) fn write_int(n: i128, out: &mut String) {
    // An i128 has at most 39 digits.
    let mut digits = [0u8; 40];
    let mut at = digits.len();
    let mut m = n.unsigned_abs();
    loop {
        at -= 1;
        // A digit: `m % 10` is below 10.
        digits[at] = b'0' + (m % 10) as u8;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    if n < 0 {
        out.push('-');
    }
    for &d in &digits[at..] {
        out.push(char::from(d));
    }
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
    #[error("longer than 4 GiB")]
    TooLong,
}

/// Parses canonical JSON. Anything [`Json::to_canonical`] would not write is refused.
pub fn parse(bytes: &[u8]) -> Result<Json, JsonError> {
    parse_tape(bytes).map(|tape| tape.root().to_json())
}

/// Parses canonical JSON into a [`Tape`] that borrows its strings from `bytes`. It accepts
/// and refuses exactly what [`parse`] does, with the same errors.
pub(crate) fn parse_tape(bytes: &[u8]) -> Result<Tape<'_>, JsonError> {
    let text = std::str::from_utf8(bytes).map_err(|e| JsonError {
        at: e.valid_up_to(),
        problem: JsonProblem::NotUtf8,
    })?;
    if u32::try_from(text.len()).is_err() {
        return Err(JsonError {
            at: 0,
            problem: JsonProblem::TooLong,
        });
    }
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
        tape: Tape {
            text,
            // About one node for every eight bytes of a change's JSON.
            nodes: Vec::with_capacity(text.len() / 6 + 4),
            escaped: Vec::new(),
        },
    };
    parser.value(0)?;
    if parser.at != text.len() {
        return Err(parser.error(JsonProblem::TrailingText));
    }
    Ok(parser.tape)
}

/// A parsed canonical JSON text as a flat list of nodes in document order. Strings without
/// escapes stay in the text, so reading a change allocates the list and nothing per value.
#[derive(Debug, Clone)]
pub(crate) struct Tape<'a> {
    text: &'a str,
    nodes: Vec<Node>,
    /// Strings that held escapes, decoded.
    escaped: Vec<String>,
}

/// One value of a [`Tape`]. A container records how many items it holds and the index just
/// past its last node, so a reader skips a value in one step. An object's members are a
/// key node followed by the value's nodes.
#[derive(Debug, Clone, Copy)]
enum Node {
    Null,
    Bool(bool),
    /// An integer: its magnitude, and its sign.
    Int {
        negative: bool,
        magnitude: u64,
    },
    /// A string without escapes: where its bytes lie in the text, quotes left out.
    Str {
        start: u32,
        len: u32,
    },
    /// A string with escapes: its place in [`Tape::escaped`].
    Escaped(u32),
    Arr {
        len: u32,
        end: u32,
    },
    Obj {
        len: u32,
        end: u32,
    },
}

impl<'a> Tape<'a> {
    /// The value the text holds.
    pub(crate) fn root(&self) -> View<'_> {
        View::Tape(self, 0)
    }

    fn node(&self, at: u32) -> Node {
        self.nodes.get(at as usize).copied().unwrap_or(Node::Null)
    }

    /// The index just past the value at `at`.
    fn skip(&self, at: u32) -> u32 {
        match self.node(at) {
            Node::Arr { end, .. } | Node::Obj { end, .. } => end,
            _ => at + 1,
        }
    }

    fn str_of(&self, node: Node) -> Option<&str> {
        match node {
            Node::Str { start, len } => {
                self.text.get(start as usize..start as usize + len as usize)
            }
            Node::Escaped(i) => self.escaped.get(i as usize).map(String::as_str),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    tape: Tape<'a>,
}

impl Parser<'_> {
    fn error(&self, problem: JsonProblem) -> JsonError {
        JsonError {
            at: self.at,
            problem,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn unexpected(&self) -> JsonError {
        match self
            .tape
            .text
            .get(self.at..)
            .and_then(|rest| rest.chars().next())
        {
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

    fn literal(&mut self, word: &str, node: Node) -> Result<(), JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            self.tape.nodes.push(node);
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    /// The tape's length as a node index. The text is under 4 GiB and every node starts at
    /// a different byte, so it fits.
    fn here(&self) -> u32 {
        self.tape.nodes.len() as u32
    }

    fn value(&mut self, depth: usize) -> Result<(), JsonError> {
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => {
                let node = self.string()?;
                self.tape.nodes.push(node);
                Ok(())
            }
            Some(b'-' | b'0'..=b'9') => self.integer(),
            Some(b't') => self.literal("true", Node::Bool(true)),
            Some(b'f') => self.literal("false", Node::Bool(false)),
            Some(b'n') => self.literal("null", Node::Null),
            _ => Err(self.unexpected()),
        }
    }

    fn object(&mut self, depth: usize) -> Result<(), JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(JsonProblem::TooDeep));
        }
        self.expect(b'{')?;
        let slot = self.tape.nodes.len();
        self.tape.nodes.push(Node::Obj { len: 0, end: 0 });
        let mut len = 0;
        if self.peek() == Some(b'}') {
            self.at += 1;
        } else {
            let mut previous: Option<Node> = None;
            loop {
                let key_at = self.at;
                let key = self.string()?;
                if let Some(previous) = previous {
                    let before = self.tape.str_of(previous).unwrap_or("");
                    let this = self.tape.str_of(key).unwrap_or("");
                    if before.as_bytes() >= this.as_bytes() {
                        return Err(JsonError {
                            at: key_at,
                            problem: JsonProblem::KeyOrder,
                        });
                    }
                }
                self.tape.nodes.push(key);
                self.expect(b':')?;
                self.value(depth)?;
                len += 1;
                previous = Some(key);
                match self.peek() {
                    Some(b',') => self.at += 1,
                    Some(b'}') => {
                        self.at += 1;
                        break;
                    }
                    _ => return Err(self.unexpected()),
                }
            }
        }
        let end = self.here();
        self.tape.nodes[slot] = Node::Obj { len, end };
        Ok(())
    }

    fn array(&mut self, depth: usize) -> Result<(), JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(JsonProblem::TooDeep));
        }
        self.expect(b'[')?;
        let slot = self.tape.nodes.len();
        self.tape.nodes.push(Node::Arr { len: 0, end: 0 });
        let mut len = 0;
        if self.peek() == Some(b']') {
            self.at += 1;
        } else {
            loop {
                self.value(depth)?;
                len += 1;
                match self.peek() {
                    Some(b',') => self.at += 1,
                    Some(b']') => {
                        self.at += 1;
                        break;
                    }
                    _ => return Err(self.unexpected()),
                }
            }
        }
        let end = self.here();
        self.tape.nodes[slot] = Node::Arr { len, end };
        Ok(())
    }

    /// A string: its node, with any escaped text decoded into [`Tape::escaped`].
    fn string(&mut self) -> Result<Node, JsonError> {
        self.expect(b'"')?;
        let start = self.at;
        self.at = plain_run(self.bytes, self.at);
        match self.peek() {
            Some(b'"') => {
                self.at += 1;
                // Offsets fit: the text is under 4 GiB.
                return Ok(Node::Str {
                    start: start as u32,
                    len: (self.at - 1 - start) as u32,
                });
            }
            Some(b'\\') => {}
            Some(_) => return Err(self.error(JsonProblem::RawControl)),
            None => return Err(self.error(JsonProblem::UnexpectedEnd)),
        }
        let mut out = String::from(&self.tape.text[start..self.at]);
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    let index = self.tape.escaped.len() as u32;
                    self.tape.escaped.push(out);
                    return Ok(Node::Escaped(index));
                }
                Some(b'\\') => out.push(self.escape()?),
                Some(_) => return Err(self.error(JsonProblem::RawControl)),
                None => return Err(self.error(JsonProblem::UnexpectedEnd)),
            }
            let from = self.at;
            self.at = plain_run(self.bytes, self.at);
            // `plain_run` stops only at ASCII bytes, so both ends are character boundaries.
            out.push_str(&self.tape.text[from..self.at]);
        }
    }

    fn escape(&mut self) -> Result<char, JsonError> {
        let start = self.at;
        let bytes = self.bytes;
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

    fn integer(&mut self) -> Result<(), JsonError> {
        let start = self.at;
        let bytes = self.bytes;
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
        let digits = &bytes[digits_start..end];
        let canonical = match digits {
            [] => false,
            [b'0'] => !negative,
            [b'0', ..] => false,
            _ => true,
        };
        if !canonical || digits.len() > 20 {
            return Err(fail(JsonProblem::Integer));
        }
        let mut magnitude: u64 = 0;
        for &d in digits {
            magnitude = magnitude
                .checked_mul(10)
                .and_then(|m| m.checked_add(u64::from(d - b'0')))
                .ok_or_else(|| fail(JsonProblem::Integer))?;
        }
        if negative && magnitude > 1 << 63 {
            return Err(fail(JsonProblem::Integer));
        }
        self.at = end;
        self.tape.nodes.push(Node::Int {
            negative,
            magnitude,
        });
        Ok(())
    }
}

/// The end of the run of string bytes from `at` that need no attention: the index of the
/// first `"`, `\` or control byte, or the end. Eight bytes at a time while none is near.
fn plain_run(bytes: &[u8], mut at: usize) -> usize {
    const ONES: u64 = u64::from_le_bytes([1; 8]);
    const HIGH: u64 = u64::from_le_bytes([0x80; 8]);
    let has_zero = |w: u64| w.wrapping_sub(ONES) & !w & HIGH;
    while let Some(chunk) = bytes.get(at..at + 8) {
        let mut word = [0; 8];
        word.copy_from_slice(chunk);
        let w = u64::from_le_bytes(word);
        let stop = has_zero(w ^ (ONES * u64::from(b'"')))
            | has_zero(w ^ (ONES * u64::from(b'\\')))
            | (w.wrapping_sub(ONES * 0x20) & !w & HIGH);
        if stop != 0 {
            break;
        }
        at += 8;
    }
    while bytes
        .get(at)
        .is_some_and(|&b| b != b'"' && b != b'\\' && b >= 0x20)
    {
        at += 1;
    }
    at
}

// ---------------------------------------------------------------------------------------
// Reading parsed values
// ---------------------------------------------------------------------------------------

/// A value to read: inside a [`Tape`], or an owned [`Json`]. The decoders of `canon` read
/// both the same way.
#[derive(Debug, Clone, Copy)]
pub(crate) enum View<'a> {
    Tape(&'a Tape<'a>, u32),
    Owned(&'a Json),
}

impl<'a> View<'a> {
    pub(crate) fn as_str(self) -> Option<&'a str> {
        match self {
            View::Tape(tape, at) => tape.str_of(tape.node(at)),
            View::Owned(j) => j.as_str(),
        }
    }

    pub(crate) fn as_int(self) -> Option<i128> {
        match self {
            View::Tape(tape, at) => match tape.node(at) {
                Node::Int {
                    negative,
                    magnitude,
                } => {
                    let m = i128::from(magnitude);
                    Some(if negative { -m } else { m })
                }
                _ => None,
            },
            View::Owned(Json::Int(n)) => Some(n.get()),
            View::Owned(_) => None,
        }
    }

    pub(crate) fn as_bool(self) -> Option<bool> {
        match self {
            View::Tape(tape, at) => match tape.node(at) {
                Node::Bool(b) => Some(b),
                _ => None,
            },
            View::Owned(Json::Bool(b)) => Some(*b),
            View::Owned(_) => None,
        }
    }

    pub(crate) fn as_obj(self) -> Option<ObjView<'a>> {
        match self {
            View::Tape(tape, at) => match tape.node(at) {
                Node::Obj { len, .. } => Some(ObjView::Tape(tape, at, len)),
                _ => None,
            },
            View::Owned(Json::Obj(map)) => Some(ObjView::Owned(map)),
            View::Owned(_) => None,
        }
    }

    pub(crate) fn as_arr(self) -> Option<ArrView<'a>> {
        match self {
            View::Tape(tape, at) => match tape.node(at) {
                Node::Arr { len, .. } => Some(ArrView::Tape(tape, at, len)),
                _ => None,
            },
            View::Owned(Json::Arr(items)) => Some(ArrView::Owned(items)),
            View::Owned(_) => None,
        }
    }

    pub(crate) fn kind_name(self) -> &'static str {
        match self {
            View::Tape(tape, at) => match tape.node(at) {
                Node::Null => "null",
                Node::Bool(_) => "a boolean",
                Node::Int { .. } => "an integer",
                Node::Str { .. } | Node::Escaped(_) => "a string",
                Node::Arr { .. } => "an array",
                Node::Obj { .. } => "an object",
            },
            View::Owned(j) => j.kind_name(),
        }
    }

    /// The value as an owned [`Json`].
    pub(crate) fn to_json(self) -> Json {
        match self {
            View::Owned(j) => j.clone(),
            View::Tape(tape, at) => match tape.node(at) {
                Node::Null => Json::Null,
                Node::Bool(b) => Json::Bool(b),
                Node::Int { .. } => self
                    .as_int()
                    .and_then(JsonInt::new)
                    .map_or(Json::Null, Json::Int),
                node @ (Node::Str { .. } | Node::Escaped(_)) => {
                    Json::Str(tape.str_of(node).unwrap_or("").to_owned())
                }
                Node::Arr { len, .. } => Json::Arr(
                    ArrView::Tape(tape, at, len)
                        .iter()
                        .map(View::to_json)
                        .collect(),
                ),
                Node::Obj { len, .. } => Json::Obj(ObjView::Tape(tape, at, len).to_map()),
            },
        }
    }
}

/// An object to read.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ObjView<'a> {
    /// The tape, the object's node and its member count.
    Tape(&'a Tape<'a>, u32, u32),
    Owned(&'a JsonMap),
}

impl<'a> ObjView<'a> {
    pub(crate) fn len(self) -> usize {
        match self {
            ObjView::Tape(_, _, len) => len as usize,
            ObjView::Owned(map) => map.len(),
        }
    }

    pub(crate) fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The members in key order.
    pub(crate) fn iter(self) -> ObjIter<'a> {
        match self {
            ObjView::Tape(tape, at, len) => ObjIter::Tape {
                tape,
                next: at + 1,
                left: len,
            },
            ObjView::Owned(map) => ObjIter::Owned(map.0.iter()),
        }
    }

    pub(crate) fn get(self, key: &str) -> Option<View<'a>> {
        match self {
            ObjView::Owned(map) => map.get(key).map(View::Owned),
            ObjView::Tape(..) => find_member(self.iter(), key).map(|(value, _)| value),
        }
    }

    pub(crate) fn contains_key(self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// The object as an owned map.
    pub(crate) fn to_map(self) -> JsonMap {
        match self {
            ObjView::Owned(map) => map.clone(),
            // The tape's members are in canonical order already.
            ObjView::Tape(..) => JsonMap(
                self.iter()
                    .map(|(key, value)| (key.to_owned(), value.to_json()))
                    .collect(),
            ),
        }
    }
}

/// The member named `key` among the members `members` still holds, and the members after
/// it. Keys are sorted, so the search stops at the first key past `key`.
pub(crate) fn find_member<'a>(
    mut members: ObjIter<'a>,
    key: &str,
) -> Option<(View<'a>, ObjIter<'a>)> {
    loop {
        let (name, value) = members.next()?;
        match name.as_bytes().cmp(key.as_bytes()) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => return Some((value, members)),
            std::cmp::Ordering::Greater => return None,
        }
    }
}

/// The members of an [`ObjView`], in key order.
#[derive(Debug, Clone)]
pub(crate) enum ObjIter<'a> {
    Tape {
        tape: &'a Tape<'a>,
        next: u32,
        left: u32,
    },
    Owned(std::slice::Iter<'a, Member>),
}

impl<'a> Iterator for ObjIter<'a> {
    type Item = (&'a str, View<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ObjIter::Tape { tape, next, left } => {
                if *left == 0 {
                    return None;
                }
                let key = tape.str_of(tape.node(*next))?;
                let value = *next + 1;
                *next = tape.skip(value);
                *left -= 1;
                Some((key, View::Tape(tape, value)))
            }
            ObjIter::Owned(members) => members
                .next()
                .map(|(key, value)| (key.as_str(), View::Owned(value))),
        }
    }
}

/// An array to read.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ArrView<'a> {
    /// The tape, the array's node and its item count.
    Tape(&'a Tape<'a>, u32, u32),
    Owned(&'a [Json]),
}

impl<'a> ArrView<'a> {
    pub(crate) fn len(self) -> usize {
        match self {
            ArrView::Tape(_, _, len) => len as usize,
            ArrView::Owned(items) => items.len(),
        }
    }

    pub(crate) fn iter(self) -> ArrIter<'a> {
        match self {
            ArrView::Tape(tape, at, len) => ArrIter::Tape {
                tape,
                next: at + 1,
                left: len,
            },
            ArrView::Owned(items) => ArrIter::Owned(items.iter()),
        }
    }
}

/// The items of an [`ArrView`].
#[derive(Debug, Clone)]
pub(crate) enum ArrIter<'a> {
    Tape {
        tape: &'a Tape<'a>,
        next: u32,
        left: u32,
    },
    Owned(std::slice::Iter<'a, Json>),
}

impl<'a> Iterator for ArrIter<'a> {
    type Item = View<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ArrIter::Tape { tape, next, left } => {
                if *left == 0 {
                    return None;
                }
                let value = *next;
                *next = tape.skip(value);
                *left -= 1;
                Some(View::Tape(tape, value))
            }
            ArrIter::Owned(items) => items.next().map(View::Owned),
        }
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
