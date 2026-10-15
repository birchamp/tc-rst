use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::cell::RefCell;
use std::fmt;

/// Why bytes did not parse as strict JSON.
#[derive(Debug)]
pub(crate) enum StrictJsonError {
    Syntax(serde_json::Error),
    DuplicateKey { pointer: String },
}

/// Parses JSON like `serde_json::from_slice`, but refuses an object with two equal keys.
/// `serde_json` keeps the last of them, so a git merge or another tool that writes a key
/// twice would otherwise lose one value without a trace.
pub(crate) fn parse_strict(bytes: &[u8]) -> Result<Value, StrictJsonError> {
    let state = RefCell::new(Walk::default());
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    match StrictSeed(&state).deserialize(&mut deserializer) {
        Ok(value) => {
            deserializer.end().map_err(StrictJsonError::Syntax)?;
            Ok(value)
        }
        Err(error) => match state.into_inner().duplicate {
            Some(pointer) => Err(StrictJsonError::DuplicateKey { pointer }),
            None => Err(StrictJsonError::Syntax(error)),
        },
    }
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

#[derive(Default)]
struct Walk {
    path: Vec<String>,
    duplicate: Option<String>,
}

#[derive(Clone, Copy)]
struct StrictSeed<'s>(&'s RefCell<Walk>);

impl<'de> DeserializeSeed<'de> for StrictSeed<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictSeed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        // JSON text cannot spell NaN or infinity, so this refuses nothing the parser accepts.
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number JSON cannot represent"))
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
        let mut items = Vec::new();
        loop {
            self.0.borrow_mut().path.push(items.len().to_string());
            let next = seq.next_element_seed(self);
            self.0.borrow_mut().path.pop();
            match next? {
                Some(item) => items.push(item),
                None => return Ok(Value::Array(items)),
            }
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if map.contains_key(&key) {
                let mut walk = self.0.borrow_mut();
                let pointer: String = walk
                    .path
                    .iter()
                    .chain(std::iter::once(&key))
                    .map(|token| format!("/{}", pointer_token(token)))
                    .collect();
                walk.duplicate = Some(pointer);
                return Err(de::Error::custom("duplicate key"));
            }
            self.0.borrow_mut().path.push(key.clone());
            let value = access.next_value_seed(self);
            self.0.borrow_mut().path.pop();
            map.insert(key, value?);
        }
        Ok(Value::Object(map))
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
