//! Reading a tC4 project's JSON and timestamps strictly, and comparing JSON as tC4 does.

use std::cell::RefCell;
use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Where a JSON object repeats a key: the dotted path of the object and the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DuplicateKey {
    pub at: String,
    pub key: String,
}

/// Why a JSON file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StrictError {
    /// `serde_json` keeps the last of two equal keys without a word, which would drop a
    /// record before any comparison could see it.
    Duplicate(DuplicateKey),
    Syntax(String),
}

/// `bytes` as JSON, refusing a repeated key in any object at any depth.
pub(crate) fn read_json_strict(bytes: &[u8]) -> Result<Value, StrictError> {
    let duplicate = RefCell::new(None);
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let read = StrictValue {
        at: String::new(),
        duplicate: &duplicate,
    }
    .deserialize(&mut reader)
    .and_then(|value| reader.end().map(|()| value));
    match (read, duplicate.into_inner()) {
        (_, Some(duplicate)) => Err(StrictError::Duplicate(duplicate)),
        (Ok(value), None) => Ok(value),
        (Err(error), None) => Err(StrictError::Syntax(error.to_string())),
    }
}

/// A JSON value read at the dotted path `at`.
struct StrictValue<'a> {
    at: String,
    duplicate: &'a RefCell<Option<DuplicateKey>>,
}

impl StrictValue<'_> {
    fn below(&self, member: &str) -> Self {
        StrictValue {
            at: if self.at.is_empty() {
                member.to_owned()
            } else {
                format!("{}.{member}", self.at)
            },
            duplicate: self.duplicate,
        }
    }
}

impl<'de> DeserializeSeed<'de> for StrictValue<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictValue<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number JSON cannot hold"))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = seq.next_element_seed(self.below(&out.len().to_string()))? {
            out.push(item);
        }
        Ok(Value::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if out.contains_key(&key) {
                *self.duplicate.borrow_mut() = Some(DuplicateKey {
                    at: self.at.clone(),
                    key,
                });
                return Err(de::Error::custom("a repeated key"));
            }
            let value = map.next_value_seed(self.below(&key))?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
}

pub(crate) use crate::js::parse_utc_millis;

/// `at` in tC4's `toISOString` form, with milliseconds: what tc-journal writes as a
/// decision's `modifiedTimestamp`. The tests prove the parser against it.
#[cfg(test)]
pub(crate) fn format_utc_millis(at: i64) -> String {
    crate::feedback::iso_utc_millis(tc_api::ids::WallMs(at))
}

/// `value` as compact JSON with every object's keys sorted: tC4's `canonical`, which keys
/// decision records by their `contextId` and compares documents.
pub(crate) fn canonical(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::from(key.as_str()).to_string());
                out.push(':');
                if let Some(v) = map.get(key) {
                    write_canonical(v, out);
                }
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// The first path at which `a` and `b` differ, `""` for the roots; `None` when they are
/// equal. Object member order does not count; array order does.
pub(crate) fn first_difference(a: &Value, b: &Value) -> Option<String> {
    fn walk(a: &Value, b: &Value, at: &str) -> Option<String> {
        let below = |member: &str| {
            if at.is_empty() {
                member.to_owned()
            } else {
                format!("{at}.{member}")
            }
        };
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
                keys.sort();
                keys.dedup();
                keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => walk(p, q, &below(k)),
                    _ => Some(below(k)),
                })
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
                .iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (p, q))| walk(p, q, &below(&i.to_string()))),
            _ if a == b => None,
            _ => Some(at.to_owned()),
        }
    }
    walk(a, b, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn strict_json_refuses_duplicate_keys_at_any_depth() {
        let dup = |text: &str| match read_json_strict(text.as_bytes()) {
            Err(StrictError::Duplicate(d)) => Some((d.at, d.key)),
            _ => None,
        };
        assert_eq!(dup(r#"{"a":1,"a":2}"#), Some((String::new(), "a".into())));
        assert_eq!(
            dup(r#"{"chapters":{"1":{"1":{},"2":{},"1":{}}}}"#),
            Some(("chapters.1".into(), "1".into()))
        );
        assert_eq!(
            dup(r#"{"decisions":[{"x":1},{"y":1,"y":true}]}"#),
            Some(("decisions.1".into(), "y".into()))
        );
        // serde_json alone keeps the last value without a word.
        let lenient: Value = serde_json::from_str(r#"{"a":1,"a":2}"#).unwrap();
        assert_eq!(lenient["a"], 2);
        assert!(matches!(
            read_json_strict(b"{\"a\":1} x"),
            Err(StrictError::Syntax(_))
        ));
        assert!(matches!(
            read_json_strict(b"{\"a\":"),
            Err(StrictError::Syntax(_))
        ));
        assert_eq!(
            read_json_strict(br#"{"a":{"b":[1,-2,3.5,null,true,"x"]},"b":{"a":1}}"#).unwrap(),
            serde_json::json!({"a":{"b":[1,-2,3.5,null,true,"x"]},"b":{"a":1}})
        );
    }

    fn json_value() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::from),
            any::<u64>().prop_map(Value::from),
            (-1.0e9f64..1.0e9).prop_map(|f| Number::from_f64(f).map_or(Value::Null, Value::Number)),
            "\\PC{0,8}".prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 48, 6, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::btree_map("\\PC{0,6}", inner, 0..6)
                    .prop_map(|m| Value::Object(m.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #[test]
        fn strict_json_reads_what_serde_reads(value in json_value(), pretty in any::<bool>()) {
            let bytes = if pretty {
                serde_json::to_vec_pretty(&value).unwrap()
            } else {
                serde_json::to_vec(&value).unwrap()
            };
            let lenient: Value = serde_json::from_slice(&bytes).unwrap();
            prop_assert_eq!(read_json_strict(&bytes).unwrap(), lenient);
        }
    }

    #[test]
    fn utc_millis_round_trips_tc4_timestamps() {
        assert_eq!(parse_utc_millis("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_utc_millis("2020-01-01T00:00:00.000Z"),
            Some(1_577_836_800_000)
        );
        assert_eq!(
            parse_utc_millis("2026-07-02T15:40:56.000Z"),
            parse_utc_millis("2026-07-02T15:40:56Z")
        );
        assert_eq!(
            parse_utc_millis("2024-02-29T23:59:59.999Z"),
            Some(1_709_251_199_999)
        );
        for bad in [
            "2023-02-29T00:00:00.000Z",
            "2026-13-01T00:00:00.000Z",
            "2026-07-02T24:00:00.000Z",
            "2026-07-02T15:40:56.0Z",
            "2026-07-02T15:40:56.000+00:00",
            "2026-07-02 15:40:56.000Z",
            "2026-07-02T15:40:56.000",
            "",
        ] {
            assert_eq!(parse_utc_millis(bad), None, "{bad}");
        }
        for at in [
            0,
            1_577_836_800_000,
            1_709_251_199_999,
            1_783_007_256_000,
            253_402_300_799_999,
        ] {
            let text = format_utc_millis(at);
            assert_eq!(parse_utc_millis(&text), Some(at), "{text}");
        }
        assert_eq!(
            format_utc_millis(1_783_007_256_000),
            "2026-07-02T15:47:36.000Z"
        );
    }

    #[test]
    fn canonical_sorts_keys_and_keeps_array_order() {
        let a = serde_json::json!({"b":[2,1],"a":{"d":1,"c":"x"}});
        assert_eq!(canonical(&a), r#"{"a":{"c":"x","d":1},"b":[2,1]}"#);
        let b = serde_json::json!({"a":{"c":"x","d":1},"b":[2,1]});
        assert_eq!(first_difference(&a, &b), None);
        let c = serde_json::json!({"a":{"c":"x","d":2},"b":[2,1]});
        assert_eq!(first_difference(&a, &c).as_deref(), Some("a.d"));
        let d = serde_json::json!({"a":{"c":"x","d":1},"b":[1,2]});
        assert_eq!(first_difference(&a, &d).as_deref(), Some("b.0"));
        let e = serde_json::json!({"a":{"c":"x","d":1}});
        assert_eq!(first_difference(&a, &e).as_deref(), Some("b"));
    }
}
