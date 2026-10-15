//! tC4's §5.1 alignment record (`checking/alignments/<BOOK>.json`, one verse) to and from
//! an [`AlignmentValue`].
//!
//! ```json
//! {"alignments": [{"topWords": [{"strong": "G39720", "lemma": "Παῦλος", "morph": "Gr,N,,,,,NMS,",
//!                                "occurrence": 1, "occurrences": 1, "word": "Παῦλος"}],
//!                  "bottomWords": [{"occurrence": 1, "occurrences": 1, "word": "Pablo"}]}],
//!  "wordBank": [{"occurrence": 1, "occurrences": 5, "word": "de"}],
//!  "invalid": false, "targetVerseMd5": "f8ae…", "sourceVersion": "dcs::unfoldingWord/el-x-koine_ugnt@v0.34",
//!  "done": true}
//! ```
//!
//! Reading parses at the boundary and refuses what it cannot represent rather than drop
//! it: occurrences must be whole numbers from 1 (a string of digits is read as tC4's
//! `normalizeOccurrences` reads it), and a target word may carry no field besides `word`,
//! `occurrence` and `occurrences`. A source word's other string fields are kept in
//! [`SourceWord::extra`]. Record fields this version does not know go to
//! [`AlignmentValue::extra`].
//!
//! The span end and the original-language pin are not in the record: the caller gives
//! them (the span from the verse key, such as `9-10`; the pin from the project).

use crate::Error;
use crate::value::{AlignmentValue, Basis, Group};
use serde_json::{Map, Value};
use tc_usfm::{SourceWord, TargetWord};

const RECORD_FIELDS: [&str; 6] = [
    "alignments",
    "wordBank",
    "invalid",
    "targetVerseMd5",
    "sourceVersion",
    "done",
];

/// Read one verse record. `basis.source_version` is replaced by the record's
/// `sourceVersion`.
pub fn from_record(record: &Value, basis: Basis) -> Result<AlignmentValue, Error> {
    let object = record
        .as_object()
        .ok_or_else(|| Error::Record("a record must be an object".into()))?;
    let alignments = array(object, "alignments")?
        .iter()
        .map(group)
        .collect::<Result<_, _>>()?;
    let word_bank = array(object, "wordBank")?
        .iter()
        .map(target_word)
        .collect::<Result<_, _>>()?;
    let marked_invalid = match object.get("invalid") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(Error::Record(format!(
                "invalid must be a boolean, not {other}"
            )));
        }
    };
    let done = match object.get("done") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(Error::Record(format!(
                "done must be a boolean, not {other}"
            )));
        }
    };
    let target_verse_md5 = string(object, "targetVerseMd5")?.parse()?;
    let source_version = string(object, "sourceVersion")?.to_owned();
    let extra = object
        .iter()
        .filter(|(k, _)| !RECORD_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(AlignmentValue {
        span_end: basis.span_end,
        alignments,
        word_bank,
        target_verse_md5,
        source_version,
        source: basis.source,
        done,
        marked_invalid,
        dismissed: false,
        extra,
    })
}

/// Project a value as a tC4 record, with `invalid` as computed for it. Keys follow the
/// order of tC4's samples; `done` appears only when true; `extra` fields follow.
pub fn to_record(value: &AlignmentValue, invalid: bool) -> Value {
    let mut out = Map::new();
    out.insert(
        "alignments".into(),
        Value::Array(
            value
                .alignments
                .iter()
                .map(|g| {
                    let mut a = Map::new();
                    a.insert(
                        "topWords".into(),
                        Value::Array(g.sources.iter().map(source_json).collect()),
                    );
                    a.insert(
                        "bottomWords".into(),
                        Value::Array(g.targets.iter().map(target_json).collect()),
                    );
                    Value::Object(a)
                })
                .collect(),
        ),
    );
    out.insert(
        "wordBank".into(),
        Value::Array(value.word_bank.iter().map(target_json).collect()),
    );
    out.insert("invalid".into(), Value::Bool(invalid));
    out.insert(
        "targetVerseMd5".into(),
        Value::String(value.target_verse_md5.to_string()),
    );
    out.insert(
        "sourceVersion".into(),
        Value::String(value.source_version.clone()),
    );
    if value.done {
        out.insert("done".into(), Value::Bool(true));
    }
    for (k, v) in &value.extra {
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

fn array<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Vec<Value>, Error> {
    object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Record(format!("{key} must be an array")))
}

fn string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Record(format!("{key} must be a string")))
}

fn group(value: &Value) -> Result<Group, Error> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::Record("an alignment must be an object".into()))?;
    Ok(Group {
        sources: array(object, "topWords")?
            .iter()
            .map(source_word)
            .collect::<Result<_, _>>()?,
        targets: array(object, "bottomWords")?
            .iter()
            .map(target_word)
            .collect::<Result<_, _>>()?,
    })
}

/// A whole number from 1, given as a number or a string of digits.
fn occurrence(object: &Map<String, Value>, key: &str) -> Result<u32, Error> {
    let bad = || Error::Record(format!("{key} must be a whole number from 1"));
    let n = match object.get(key) {
        Some(Value::Number(n)) => n.as_u64().ok_or_else(bad)?,
        Some(Value::String(s)) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse().map_err(|_| bad())?
        }
        _ => return Err(bad()),
    };
    u32::try_from(n).ok().filter(|n| *n >= 1).ok_or_else(bad)
}

fn word_object(value: &Value) -> Result<&Map<String, Value>, Error> {
    value
        .as_object()
        .ok_or_else(|| Error::Record("a word must be an object".into()))
}

fn target_word(value: &Value) -> Result<TargetWord, Error> {
    let object = word_object(value)?;
    if let Some(key) = object
        .keys()
        .find(|k| !matches!(k.as_str(), "word" | "occurrence" | "occurrences"))
    {
        return Err(Error::Record(format!(
            "a target word has a field {key:?} that would be lost"
        )));
    }
    Ok(TargetWord {
        text: string(object, "word")?.to_owned(),
        occurrence: occurrence(object, "occurrence")?,
        occurrences: occurrence(object, "occurrences")?,
    })
}

fn source_word(value: &Value) -> Result<SourceWord, Error> {
    let object = word_object(value)?;
    let optional = |key: &str| -> Result<Option<String>, Error> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(other) => Err(Error::Record(format!(
                "{key} must be a string, not {other}"
            ))),
        }
    };
    let mut extra = Vec::new();
    for (key, v) in object {
        if matches!(
            key.as_str(),
            "word" | "strong" | "lemma" | "morph" | "occurrence" | "occurrences"
        ) {
            continue;
        }
        let Value::String(s) = v else {
            return Err(Error::Record(format!(
                "a source word's field {key:?} is not a string and would be lost"
            )));
        };
        extra.push((key.clone(), s.clone()));
    }
    Ok(SourceWord {
        content: string(object, "word")?.to_owned(),
        strong: optional("strong")?,
        lemma: optional("lemma")?,
        morph: optional("morph")?,
        occurrence: occurrence(object, "occurrence")?,
        occurrences: occurrence(object, "occurrences")?,
        extra,
    })
}

fn source_json(word: &SourceWord) -> Value {
    let mut out = Map::new();
    for (key, value) in [
        ("strong", &word.strong),
        ("lemma", &word.lemma),
        ("morph", &word.morph),
    ] {
        if let Some(value) = value {
            out.insert(key.into(), Value::String(value.clone()));
        }
    }
    out.insert("occurrence".into(), word.occurrence.into());
    out.insert("occurrences".into(), word.occurrences.into());
    out.insert("word".into(), Value::String(word.content.clone()));
    for (key, value) in &word.extra {
        out.insert(key.clone(), Value::String(value.clone()));
    }
    Value::Object(out)
}

fn target_json(word: &TargetWord) -> Value {
    let mut out = Map::new();
    out.insert("occurrence".into(), word.occurrence.into());
    out.insert("occurrences".into(), word.occurrences.into());
    out.insert("word".into(), Value::String(word.text.clone()));
    Value::Object(out)
}
