//! Reading tC4's §5.1 alignment record (`checking/alignments/<BOOK>.json`, one verse) into
//! the journal's `AlignmentValue`, for imports of tC3 and tC4 projects (STORAGE.md §13).
//! Writing the record is the journal's projection of the value (§7.7), so this module only
//! reads.
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
//! `normalizeOccurrences` reads it), and a number with a fraction anywhere is refused,
//! since the journal holds whole numbers only. A record must keep the rules every edit
//! keeps ([`Alignment::well_formed`]): each target word once, every occurrence within its
//! count. Every one of the 3,194 records in tC4's tC3 and burrito import fixtures does.
//!
//! Nothing is dropped. A word's other fields, such as the `"type": "bottomWord"` tC3 wrote
//! on every banked word (65,313 words in tC4's tC3 import fixtures), are kept in the word's
//! own `extra` and go wherever the word goes. A card's fields besides `topWords` and
//! `bottomWords` are kept in the card's `extra`; the aligner then refuses to merge that
//! card into another, which would lose them ([`crate::EditError::CardFields`]). Record
//! fields this version does not know go to the value's `extra`. tC3's and tC4's stored
//! `invalid` is the value's `marked_invalid`.
//!
//! The span end and the original-language pin are not in the record: the caller gives
//! them (the span from the verse key, such as `9-10`; the pin from the project).

use crate::Error;
use crate::edit::Alignment;
use crate::value::Basis;
use serde_json::{Map, Value};
use tc_journal::json::{Json, JsonMap};
use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};

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
    let extra = extra(object, &RECORD_FIELDS)?;
    let value = AlignmentValue {
        span_end: basis.span_end(),
        alignments,
        word_bank,
        target_verse_md5,
        source_version: source_version.into(),
        source: basis.source,
        done,
        marked_invalid,
        dismissed: false,
        extra,
    };
    value.well_formed()?;
    Ok(value)
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

/// The members of `object` besides `known`, kept as the journal holds JSON. A number with a
/// fraction is refused: the journal holds whole numbers only, and dropping it would lose it.
fn extra(object: &Map<String, Value>, known: &[&str]) -> Result<JsonMap, Error> {
    object
        .iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .map(|(k, v)| {
            Json::try_from(v)
                .map(|v| (k.clone(), v))
                .map_err(|e| Error::Record(format!("the field {k:?} would be lost: {e}")))
        })
        .collect()
}

fn group(value: &Value) -> Result<AlignmentGroup, Error> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::Record("an alignment must be an object".into()))?;
    Ok(AlignmentGroup {
        top_words: array(object, "topWords")?
            .iter()
            .map(source_word)
            .collect::<Result<_, _>>()?,
        bottom_words: array(object, "bottomWords")?
            .iter()
            .map(target_word)
            .collect::<Result<_, _>>()?,
        extra: extra(object, &["topWords", "bottomWords"])?,
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

/// A target word, with its fields besides the three it is made of.
fn target_word(value: &Value) -> Result<TargetWord, Error> {
    let object = word_object(value)?;
    Ok(TargetWord {
        word: string(object, "word")?.to_owned(),
        occurrence: occurrence(object, "occurrence")?,
        occurrences: occurrence(object, "occurrences")?,
        extra: extra(object, &["word", "occurrence", "occurrences"])?,
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
    Ok(SourceWord {
        word: string(object, "word")?.to_owned(),
        strong: optional("strong")?,
        lemma: optional("lemma")?,
        morph: optional("morph")?,
        occurrence: occurrence(object, "occurrence")?,
        occurrences: occurrence(object, "occurrences")?,
        extra: extra(
            object,
            &[
                "word",
                "strong",
                "lemma",
                "morph",
                "occurrence",
                "occurrences",
            ],
        )?,
    })
}
