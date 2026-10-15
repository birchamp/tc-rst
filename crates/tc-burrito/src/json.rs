use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::cell::RefCell;
use std::fmt;

/// Deeper nesting is refused before `serde_json` reaches its own limit of 128, so the
/// refusal can name the place. A burrito nests about eight levels.
pub(crate) const MAX_DEPTH: usize = 100;

/// Why bytes did not parse as strict JSON.
#[derive(Debug)]
pub(crate) enum StrictJsonError {
    Syntax(serde_json::Error),
    DuplicateKey {
        pointer: String,
    },
    /// The number's value cannot be held exactly, so writing it back would change it:
    /// `1e400`, or a 30-digit integer that an f64 rounds.
    UnrepresentableNumber {
        pointer: String,
    },
    /// A string escapes half of a UTF-16 surrogate pair, which is not a character.
    LoneSurrogate {
        pointer: String,
    },
    TooDeep {
        pointer: String,
    },
}

/// Splits off a leading UTF-8 byte order mark. RFC 8259 lets a parser ignore one, and
/// Windows editors add one; the writer never writes one.
pub(crate) fn strip_bom(bytes: &[u8]) -> (bool, &[u8]) {
    match bytes.strip_prefix(b"\xef\xbb\xbf") {
        Some(rest) => (true, rest),
        None => (false, bytes),
    }
}

/// Parses JSON like `serde_json::from_slice`, but refuses what a read-modify-write would
/// lose: an object with two equal keys (`serde_json` keeps the last), and a number the
/// writer would print as a different value. Nesting deeper than [`MAX_DEPTH`] is refused
/// with its place. A leading byte order mark is a syntax error here; see [`strip_bom`].
pub(crate) fn parse_strict(bytes: &[u8]) -> Result<Value, StrictJsonError> {
    let tokens = scan(bytes);
    let state = RefCell::new(Walk {
        bytes,
        numbers: tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Number)
            .map(|token| (token.start, token.end))
            .collect(),
        numbers_read: 0,
        path: Vec::new(),
        stop: None,
        failed_at: None,
    });
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    match StrictSeed(&state).deserialize(&mut deserializer) {
        Ok(value) => {
            deserializer.end().map_err(StrictJsonError::Syntax)?;
            Ok(value)
        }
        Err(error) => {
            let walk = state.into_inner();
            Err(match walk.stop {
                Some(stop) => stop,
                None => explain(error, bytes, &tokens, walk.failed_at.unwrap_or_default()),
            })
        }
    }
}

/// Names the cause of a parser error that lies inside a token: a well-formed number too
/// large for an f64, or a string with a lone surrogate escape. This only chooses which
/// refusal to report; a document the parser refused is refused either way.
fn explain(
    error: serde_json::Error,
    bytes: &[u8],
    tokens: &[Token],
    pointer: String,
) -> StrictJsonError {
    let at = byte_offset(bytes, error.line(), error.column());
    // A token ends before the next starts, so the last token starting at or before the
    // error is the only one that can contain it. The parser reports a number error just
    // after the number's last byte.
    let index = tokens.partition_point(|token| token.start <= at);
    let Some(token) = index.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return StrictJsonError::Syntax(error);
    };
    if at > token.end {
        return StrictJsonError::Syntax(error);
    }
    match token.kind {
        TokenKind::Number if bytes.get(token.start..token.end).is_some_and(overflows) => {
            StrictJsonError::UnrepresentableNumber { pointer }
        }
        TokenKind::Text {
            lone_surrogate: true,
        } => StrictJsonError::LoneSurrogate { pointer },
        _ => StrictJsonError::Syntax(error),
    }
}

/// The byte offset `serde_json` means by a line (from 1) and a column (bytes from the
/// start of that line).
fn byte_offset(bytes: &[u8], line: usize, column: usize) -> usize {
    let line_start = match line.checked_sub(2) {
        None => 0,
        Some(newlines_before) => bytes
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .nth(newlines_before)
            .map_or(bytes.len(), |(index, _)| index + 1),
    };
    line_start.saturating_add(column)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Number,
    Text { lone_surrogate: bool },
}

/// A number or string of the input, as the byte range `start..end`.
#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

/// Finds the numbers and strings of `bytes` in document order. On valid JSON these are
/// exactly the parser's numbers and strings, because no other JSON token holds a digit,
/// a '-' or a '"'. On other input the tokens only serve to explain a parser error, and
/// the scan never fails.
fn scan(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let start = index;
        let kind = match byte {
            b'"' => {
                let (end, lone_surrogate) = scan_string(bytes, start + 1);
                index = end;
                TokenKind::Text { lone_surrogate }
            }
            b'-' | b'0'..=b'9' => {
                index += bytes
                    .get(start..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                    .count();
                TokenKind::Number
            }
            _ => {
                index += 1;
                continue;
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: index,
        });
    }
    tokens
}

/// Scans a string body from `index`, just after its opening quote. Returns the index just
/// after the closing quote (or the end of input) and whether a surrogate escape has no
/// partner: a high one (`\ud800`) not directly followed by a low one, or a low one alone.
fn scan_string(bytes: &[u8], mut index: usize) -> (usize, bool) {
    let mut lone = false;
    let mut high_waiting = false;
    while let Some(&byte) = bytes.get(index) {
        let unit = match byte {
            b'"' => return (index + 1, lone || high_waiting),
            b'\\' if bytes.get(index + 1) == Some(&b'u') => {
                let unit = bytes.get(index + 2..index + 6).and_then(hex_unit);
                index += 6;
                unit
            }
            b'\\' => {
                index += 2;
                None
            }
            _ => {
                index += 1;
                None
            }
        };
        match unit {
            Some(0xD800..=0xDBFF) => {
                lone |= high_waiting;
                high_waiting = true;
            }
            Some(0xDC00..=0xDFFF) => {
                lone |= !high_waiting;
                high_waiting = false;
            }
            _ => {
                lone |= high_waiting;
                high_waiting = false;
            }
        }
    }
    (bytes.len(), lone || high_waiting)
}

fn hex_unit(digits: &[u8]) -> Option<u16> {
    digits.iter().try_fold(0u16, |unit, digit| {
        let value = char::from(*digit).to_digit(16)?;
        Some(unit << 4 | u16::try_from(value).ok()?)
    })
}

/// The exact value of a JSON number: the sign, the significant digits, and the power of
/// ten of the place just before the first of them. `-120.50` is `-0.1205e3`, so
/// `(true, "1205", 3)`. Zero has no digits.
#[derive(Debug, PartialEq, Eq)]
struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    point: i64,
}

impl Decimal {
    /// `None` unless `text` follows the JSON number grammar.
    fn parse(text: &[u8]) -> Option<Self> {
        let (negative, rest) = match text.split_first() {
            Some((b'-', rest)) => (true, rest),
            _ => (false, text),
        };
        let (int, rest) = split_digits(rest);
        if int.is_empty() || (int.len() > 1 && int.first() == Some(&b'0')) {
            return None;
        }
        let (fraction, rest) = match rest.split_first() {
            Some((b'.', rest)) => match split_digits(rest) {
                ([], _) => return None,
                found => found,
            },
            _ => (&[][..], rest),
        };
        let exponent = match rest.split_first() {
            None => 0,
            Some((b'e' | b'E', rest)) => {
                let (sign, rest) = match rest.split_first() {
                    Some((b'-', rest)) => (-1, rest),
                    Some((b'+', rest)) => (1, rest),
                    _ => (1, rest),
                };
                match split_digits(rest) {
                    ([], _) | (_, [_, ..]) => return None,
                    // Capped far beyond any f64 exponent, so a huge one still compares
                    // unequal to what the parser made of it.
                    (digits, []) => {
                        sign * digits.iter().fold(0i64, |total, digit| {
                            (total * 10 + i64::from(digit - b'0')).min(1 << 40)
                        })
                    }
                }
            }
            Some(_) => return None,
        };
        let all: Vec<u8> = int.iter().chain(fraction).copied().collect();
        let leading = all.iter().take_while(|digit| **digit == b'0').count();
        let trailing = all.iter().rev().take_while(|digit| **digit == b'0').count();
        if leading == all.len() {
            return Some(Self {
                negative,
                digits: Vec::new(),
                point: 0,
            });
        }
        let digits = all.get(leading..all.len() - trailing)?.to_vec();
        let point = i64::try_from(int.len())
            .ok()?
            .checked_sub(i64::try_from(leading).ok()?)?
            .checked_add(exponent)?;
        Some(Self {
            negative,
            digits,
            point,
        })
    }
}

/// Whether `text` is a JSON number beyond the f64 range, which the parser refuses.
fn overflows(text: &[u8]) -> bool {
    Decimal::parse(text).is_some()
        && std::str::from_utf8(text)
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .is_some_and(f64::is_infinite)
}

fn split_digits(text: &[u8]) -> (&[u8], &[u8]) {
    let count = text.iter().take_while(|byte| byte.is_ascii_digit()).count();
    text.split_at(count)
}

/// Writes `value` as pretty JSON with the keys of every object sorted by their UTF-8 bytes,
/// two-space indent and a trailing newline. Arrays keep their order. Equal values give
/// equal bytes whatever order their keys were inserted in, so two devices that save the
/// same metadata produce identical files and git sees no change.
pub fn to_canonical_json(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(&mut out, value, 0);
    out.push(b'\n');
    out
}

fn write_value(out: &mut Vec<u8>, value: &Value, depth: usize) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(number) => out.extend_from_slice(number.to_string().as_bytes()),
        Value::String(text) => write_string(out, text),
        Value::Array(items) if items.is_empty() => out.extend_from_slice(b"[]"),
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                newline(out, depth + 1);
                write_value(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(b']');
        }
        Value::Object(map) if map.is_empty() => out.extend_from_slice(b"{}"),
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push(b'{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                newline(out, depth + 1);
                write_string(out, key);
                out.extend_from_slice(b": ");
                write_value(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(b'}');
        }
    }
}

fn newline(out: &mut Vec<u8>, depth: usize) {
    out.push(b'\n');
    out.extend(std::iter::repeat_n(b' ', depth * 2));
}

/// Escapes the way `serde_json` does: quote, backslash and C0 controls only. Everything
/// else, including non-ASCII text, is written as raw UTF-8.
fn write_string(out: &mut Vec<u8>, text: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(b'"');
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        let escape: &[u8] = match byte {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x08 => b"\\b",
            0x0c => b"\\f",
            0x00..=0x1f => &[],
            _ => continue,
        };
        out.extend_from_slice(&text.as_bytes()[start..index]);
        if escape.is_empty() {
            out.extend_from_slice(b"\\u00");
            out.push(HEX[usize::from(byte >> 4)]);
            out.push(HEX[usize::from(byte & 0x0f)]);
        } else {
            out.extend_from_slice(escape);
        }
        start = index + 1;
    }
    out.extend_from_slice(&text.as_bytes()[start..]);
    out.push(b'"');
}

/// Escapes one JSON Pointer reference token (RFC 6901).
pub(crate) fn pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

struct Walk<'b> {
    bytes: &'b [u8],
    /// The byte ranges of the input's numbers, in the order the parser reads them.
    numbers: Vec<(usize, usize)>,
    numbers_read: usize,
    /// The keys and indexes from the root to the value being read.
    path: Vec<String>,
    /// A refusal the walk made itself; it explains the error the parser returns.
    stop: Option<StrictJsonError>,
    /// Where the parser was when it failed: the innermost value being read.
    failed_at: Option<String>,
}

impl Walk<'_> {
    fn pointer(&self) -> String {
        self.path
            .iter()
            .map(|token| format!("/{}", pointer_token(token)))
            .collect()
    }

    fn note_failure(&mut self) {
        if self.failed_at.is_none() {
            self.failed_at = Some(self.pointer());
        }
    }

    fn refuse<E: de::Error>(&mut self, stop: StrictJsonError) -> E {
        self.stop = Some(stop);
        E::custom("refused by the strict reader")
    }

    fn enter<E: de::Error>(&mut self) -> Result<(), E> {
        if self.path.len() >= MAX_DEPTH {
            let pointer = self.pointer();
            return Err(self.refuse(StrictJsonError::TooDeep { pointer }));
        }
        Ok(())
    }

    /// The text of the next number the parser reads.
    fn next_number(&mut self) -> Option<&[u8]> {
        let (start, end) = *self.numbers.get(self.numbers_read)?;
        self.numbers_read += 1;
        self.bytes.get(start..end)
    }
}

#[derive(Clone, Copy)]
struct StrictSeed<'s, 'b>(&'s RefCell<Walk<'b>>);

impl<'de> DeserializeSeed<'de> for StrictSeed<'_, '_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictSeed<'_, '_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    // The parser hands over an integer only when the text is one that fits, so it is exact.
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        self.0.borrow_mut().next_number();
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        self.0.borrow_mut().next_number();
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        // JSON text cannot spell NaN or infinity, so this refuses nothing the parser accepts.
        let number =
            Number::from_f64(v).ok_or_else(|| E::custom("a number JSON cannot represent"))?;
        let mut walk = self.0.borrow_mut();
        let read = walk.next_number().map(Decimal::parse);
        let written = Decimal::parse(number.to_string().as_bytes());
        // `None` means the scan and the parser disagree, which valid JSON cannot cause.
        if read.is_some_and(|read| read != written) {
            let pointer = walk.pointer();
            return Err(walk.refuse(StrictJsonError::UnrepresentableNumber { pointer }));
        }
        Ok(Value::Number(number))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        self.0.borrow_mut().enter()?;
        let mut items = Vec::new();
        loop {
            self.0.borrow_mut().path.push(items.len().to_string());
            let next = seq.next_element_seed(self);
            let mut walk = self.0.borrow_mut();
            if next.is_err() {
                walk.note_failure();
            }
            walk.path.pop();
            drop(walk);
            match next? {
                Some(item) => items.push(item),
                None => return Ok(Value::Array(items)),
            }
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        self.0.borrow_mut().enter()?;
        let mut map = Map::new();
        loop {
            let key = match access.next_key::<String>() {
                Ok(Some(key)) => key,
                Ok(None) => return Ok(Value::Object(map)),
                Err(error) => {
                    self.0.borrow_mut().note_failure();
                    return Err(error);
                }
            };
            let mut walk = self.0.borrow_mut();
            walk.path.push(key.clone());
            if map.contains_key(&key) {
                let pointer = walk.pointer();
                return Err(walk.refuse(StrictJsonError::DuplicateKey { pointer }));
            }
            drop(walk);
            let value = access.next_value_seed(self);
            let mut walk = self.0.borrow_mut();
            if value.is_err() {
                walk.note_failure();
            }
            walk.path.pop();
            drop(walk);
            map.insert(key, value?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strict_parse_matches_serde_json_on_ordinary_documents() {
        let text = br#"{"b": [1, -2, 3.5, "x", null, true], "a": {"c": {}}}"#;
        let strict = parse_strict(text).unwrap();
        assert_eq!(strict, serde_json::from_slice::<Value>(text).unwrap());
    }

    #[test]
    fn strict_parse_names_the_duplicate_key() {
        let text = br#"{"ingredients": {"a/b": {"size": 1}, "a/b": {"size": 2}}}"#;
        match parse_strict(text) {
            Err(StrictJsonError::DuplicateKey { pointer }) => {
                assert_eq!(pointer, "/ingredients/a~1b");
            }
            other => panic!("expected a duplicate key, got {other:?}"),
        }
        let in_array = br#"[{"x": 1}, {"y": 1, "y": 2}]"#;
        assert!(matches!(
            parse_strict(in_array),
            Err(StrictJsonError::DuplicateKey { pointer }) if pointer == "/1/y"
        ));
    }

    #[test]
    fn strict_parse_names_numbers_it_cannot_keep() {
        let at = |text: &str| match parse_strict(text.as_bytes()) {
            Err(StrictJsonError::UnrepresentableNumber { pointer }) => Some(pointer),
            _ => None,
        };
        assert_eq!(at(r#"{"a": [1, 1e400]}"#).as_deref(), Some("/a/1"));
        assert_eq!(
            at("{\n  \"a\": 1,\n  \"b~/\": -1e400\n}").as_deref(),
            Some("/b~0~1")
        );
        assert_eq!(
            at("[0.1, 2, 123456789012345678901234567890]").as_deref(),
            Some("/2")
        );
        assert_eq!(at("1e400").as_deref(), Some(""));
        // Errors next to a number are not blamed on it.
        for not_json in ["[1 2]", "[1, x, 1e400]", "[01]", "[1.]", "{} 1e400"] {
            assert!(
                matches!(
                    parse_strict(not_json.as_bytes()),
                    Err(StrictJsonError::Syntax(_))
                ),
                "{not_json}"
            );
        }
    }

    #[test]
    fn strict_parse_names_lone_surrogates_and_deep_nesting() {
        assert!(matches!(
            parse_strict(br#"{"a": {"b": "x\udc00"}}"#),
            Err(StrictJsonError::LoneSurrogate { pointer }) if pointer == "/a/b"
        ));
        assert!(matches!(
            parse_strict(br#"[{"\ud800": 1}]"#),
            Err(StrictJsonError::LoneSurrogate { pointer }) if pointer == "/0"
        ));
        assert_eq!(
            parse_strict(br#""\ud83d\ude00""#).unwrap(),
            Value::String("\u{1f600}".to_owned())
        );
        let nested = |levels: usize| format!("{}{}", "[".repeat(levels), "]".repeat(levels));
        assert!(parse_strict(nested(MAX_DEPTH).as_bytes()).is_ok());
        assert!(matches!(
            parse_strict(nested(MAX_DEPTH + 1).as_bytes()),
            Err(StrictJsonError::TooDeep { pointer }) if pointer == "/0".repeat(MAX_DEPTH)
        ));
    }

    #[test]
    fn strict_parse_refuses_trailing_garbage() {
        assert!(matches!(
            parse_strict(b"{} x"),
            Err(StrictJsonError::Syntax(_))
        ));
    }

    #[test]
    fn canonical_json_sorts_keys_and_matches_serde_json_escaping() {
        let value = json!({"b": 1, "a": ["z", {"y": "\u{1}\"\\\n", "x": "é—"}], "c": {}, "d": []});
        let bytes = to_canonical_json(&value);
        let text = String::from_utf8(bytes).unwrap();
        let mut sorted = Map::new();
        for key in ["a", "b", "c", "d"] {
            sorted.insert(key.to_owned(), value[key].clone());
        }
        let mut inner = Map::new();
        inner.insert("x".into(), json!("é—"));
        inner.insert("y".into(), json!("\u{1}\"\\\n"));
        sorted["a"][1] = Value::Object(inner);
        let expected = serde_json::to_string_pretty(&Value::Object(sorted)).unwrap() + "\n";
        assert_eq!(text, expected);
    }
}
