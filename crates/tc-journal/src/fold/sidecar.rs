//! Reading the tC4-shaped sidecars back into records (§11.1 X2): what an outside edit of
//! `checking/alignments/<BOOK>.json` or `checking/<tool>/<BOOK>.json` changed. Every field
//! the projection writes is read back, so rendering then decoding gives every record
//! again (`sidecar_decode_render`); the flags the fold computes (`invalid`, `invalidated`,
//! `verseEdits`, `userInvalid`, `status` of an invalidated record) are ignored.

use crate::canon;
use crate::ids::{ChapterNo, GitOid, Md5, VerseNo};
use crate::json::{Json, JsonMap};
use crate::keys::{CheckKey, ToolId};
use crate::record::{
    AlignmentGroup, AlignmentValue, CheckValue, DecisionRecord, Judgement, Record, ResourceRef,
    Selection, SourceWord, TargetWord, UserStatus,
};
use serde_json::{Map, Value};
use tc_core::BookId;

/// Why a sidecar cannot be read: the person sees it and the file's bytes are kept (X2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{at}: {problem}")]
pub struct SidecarError {
    pub at: String,
    pub problem: String,
}

pub(super) fn fail(at: impl Into<String>, problem: impl Into<String>) -> SidecarError {
    SidecarError {
        at: at.into(),
        problem: problem.into(),
    }
}

fn json(v: &Value, at: &str) -> Result<Json, SidecarError> {
    Json::try_from(v).map_err(|e| fail(at, e.to_string()))
}

fn obj<'a>(v: &'a Value, at: &str) -> Result<&'a Map<String, Value>, SidecarError> {
    v.as_object().ok_or_else(|| fail(at, "expected an object"))
}

fn string(m: &Map<String, Value>, key: &str, at: &str) -> Result<String, SidecarError> {
    m.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| fail(at, format!("{key} is not a string")))
}

fn number(m: &Map<String, Value>, key: &str, at: &str) -> Result<u32, SidecarError> {
    m.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| fail(at, format!("{key} is not a count")))
}

fn flag(m: &Map<String, Value>, key: &str) -> bool {
    m.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn extra(m: &Map<String, Value>, known: &[&str], at: &str) -> Result<JsonMap, SidecarError> {
    let mut out = JsonMap::new();
    for (k, v) in m {
        if !known.contains(&k.as_str()) {
            out.insert(k.clone(), json(v, at)?);
        }
    }
    Ok(out)
}

fn md5(m: &Map<String, Value>, key: &str, at: &str) -> Result<Md5, SidecarError> {
    Md5::parse(&string(m, key, at)?).ok_or_else(|| fail(at, format!("{key} is not an md5")))
}

fn resource(v: &Value, at: &str) -> Result<ResourceRef, SidecarError> {
    let m = obj(v, at)?;
    Ok(ResourceRef {
        repo_path: string(m, "repoPath", at)?.into(),
        sha: GitOid::parse(&string(m, "sha", at)?)
            .ok_or_else(|| fail(at, "sha is not a commit"))?,
    })
}

fn target_word(v: &Value, at: &str) -> Result<TargetWord, SidecarError> {
    let m = obj(v, at)?;
    Ok(TargetWord {
        word: string(m, "word", at)?,
        occurrence: number(m, "occurrence", at)?,
        occurrences: number(m, "occurrences", at)?,
        extra: extra(m, &["word", "occurrence", "occurrences"], at)?,
    })
}

fn source_word(v: &Value, at: &str) -> Result<SourceWord, SidecarError> {
    let m = obj(v, at)?;
    let opt = |key: &str| m.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(SourceWord {
        word: string(m, "word", at)?,
        strong: opt("strong"),
        lemma: opt("lemma"),
        morph: opt("morph"),
        occurrence: number(m, "occurrence", at)?,
        occurrences: number(m, "occurrences", at)?,
        extra: extra(
            m,
            &[
                "word",
                "strong",
                "lemma",
                "morph",
                "occurrence",
                "occurrences",
            ],
            at,
        )?,
    })
}

fn list<T>(
    m: &Map<String, Value>,
    key: &str,
    at: &str,
    read: impl Fn(&Value, &str) -> Result<T, SidecarError>,
) -> Result<Vec<T>, SidecarError> {
    m.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| fail(at, format!("{key} is not a list")))?
        .iter()
        .map(|v| read(v, at))
        .collect()
}

/// Every alignment record of an alignments sidecar.
pub fn decode_alignments(book: BookId, bytes: &[u8]) -> Result<Vec<Record>, SidecarError> {
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| fail("the file", e.to_string()))?;
    let doc = obj(&doc, "the file")?;
    let chapters = obj(
        doc.get("chapters")
            .ok_or_else(|| fail("the file", "no chapters"))?,
        "chapters",
    )?;
    let mut out = Vec::new();
    for (chapter_label, verses) in chapters {
        let chapter: u16 = chapter_label
            .parse()
            .map_err(|_| fail(chapter_label.as_str(), "the chapter is not a number"))?;
        for (label, record) in obj(verses, chapter_label)? {
            let at = format!("{chapter_label}:{label}");
            let mut bounds = label.split('-');
            let first: u16 = bounds
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| fail(&at, "the verse is not a number"))?;
            let last: u16 = bounds
                .next()
                .map(|v| v.parse().map_err(|_| fail(&at, "the span is not a number")))
                .transpose()?
                .unwrap_or(first);
            let m = obj(record, &at)?;
            let groups = list(m, "alignments", &at, |g, at| {
                let g = obj(g, at)?;
                Ok(AlignmentGroup {
                    top_words: list(g, "topWords", at, source_word)?,
                    bottom_words: list(g, "bottomWords", at, target_word)?,
                    extra: extra(g, &["topWords", "bottomWords"], at)?,
                })
            })?;
            let (c, v) = match m.get("anchor") {
                Some(anchor) => {
                    let a = obj(anchor, &at)?;
                    (
                        u16::try_from(number(a, "chapter", &at)?).unwrap_or(0),
                        u16::try_from(number(a, "verse", &at)?).unwrap_or(0),
                    )
                }
                None => (chapter, first),
            };
            let span_end = match m.get("spanEnd") {
                Some(_) => u16::try_from(number(m, "spanEnd", &at)?).unwrap_or(0),
                None => last,
            };
            let value = AlignmentValue {
                span_end: VerseNo::new(span_end).ok_or_else(|| fail(&at, "verse 0"))?,
                alignments: groups,
                word_bank: list(m, "wordBank", &at, target_word)?,
                target_verse_md5: md5(m, "targetVerseMd5", &at)?,
                source_version: string(m, "sourceVersion", &at)?.into(),
                source: m.get("source").map(|s| resource(s, &at)).transpose()?,
                done: flag(m, "done"),
                marked_invalid: flag(m, "markedInvalid"),
                dismissed: flag(m, "dismissed"),
                extra: extra(
                    m,
                    &[
                        "alignments",
                        "wordBank",
                        "invalid",
                        "targetVerseMd5",
                        "sourceVersion",
                        "done",
                        "source",
                        "markedInvalid",
                        "dismissed",
                        "anchor",
                        "spanEnd",
                    ],
                    &at,
                )?,
            };
            out.push(Record::Alignment {
                book,
                chapter: ChapterNo::new(c).ok_or_else(|| fail(&at, "chapter 0"))?,
                verse: VerseNo::new(v).ok_or_else(|| fail(&at, "verse 0"))?,
                value,
            });
        }
    }
    Ok(out)
}

fn status(text: &str) -> Option<UserStatus> {
    match text {
        "valid" => Some(UserStatus::Valid),
        "invalid" => Some(UserStatus::Invalid),
        "todo" => Some(UserStatus::Todo),
        _ => None,
    }
}

/// Every decision record of a decisions sidecar: `checking/<tool>/<BOOK>.json`, or
/// `checking/<tool>/OBS.json` for the stories (`book` is then `None`).
pub fn decode_decisions(
    tool: ToolId,
    book: Option<BookId>,
    bytes: &[u8],
) -> Result<Vec<Record>, SidecarError> {
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| fail("the file", e.to_string()))?;
    let doc = obj(&doc, "the file")?;
    // The document's `resource` is the resolution, a computed field: an unusable pin has no
    // sha there (§7.7), and then every decision carries its own `pinnedResource`.
    let document_resource = doc
        .get("resource")
        .and_then(|r| resource(r, "resource").ok());
    let decisions = doc
        .get("decisions")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("the file", "no decisions"))?;
    let mut out = Vec::new();
    for (i, record) in decisions.iter().enumerate() {
        let at = format!("decision {}", i + 1);
        let m = obj(record, &at)?;
        let context = match json(
            m.get("contextId")
                .ok_or_else(|| fail(&at, "no contextId"))?,
            &at,
        )? {
            Json::Obj(map) => map,
            _ => return Err(fail(&at, "contextId is not an object")),
        };
        let key: CheckKey = match m.get("checkKey") {
            Some(k) => canon::check_key_from_json(&json(k, &at)?)
                .ok_or_else(|| fail(&at, "checkKey is not a check key"))?,
            None => super::project::derive_key(tool, book, &context)
                .ok_or_else(|| fail(&at, "the contextId names no check"))?,
        };
        let selections = match m.get("selections") {
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .map(|s| {
                        let s = obj(s, &at)?;
                        Ok(Selection {
                            text: string(s, "text", &at)?,
                            occurrence: number(s, "occurrence", &at)?,
                            occurrences: number(s, "occurrences", &at)?,
                            extra: extra(s, &["text", "occurrence", "occurrences"], &at)?,
                        })
                    })
                    .collect::<Result<Vec<_>, SidecarError>>()?,
            ),
            _ => None,
        };
        let invalidated = flag(m, "invalidated");
        let user_status = match m.get("userStatus").and_then(Value::as_str) {
            Some(s) => status(s),
            None if !invalidated => m.get("status").and_then(Value::as_str).and_then(status),
            None => None,
        };
        let resource = match (m.get("pinnedResource"), &document_resource) {
            (Some(r), _) => resource(r, &at)?,
            (None, Some(r)) => r.clone(),
            (None, None) => return Err(fail(&at, "no resource")),
        };
        let decision = DecisionRecord {
            context,
            category: m.get("category").and_then(Value::as_str).map(Into::into),
            judgement: Judgement {
                selections,
                nothing_to_select: flag(m, "nothingToSelect"),
                status: user_status,
                verse_text_md5: md5(m, "verseTextMd5", &at)?,
            },
            comment: m.get("comments").and_then(Value::as_str).map(str::to_owned),
            bookmarked: flag(m, "reminders"),
            resource,
            imported_invalid: flag(m, "importedInvalid"),
            dismissed: flag(m, "dismissed"),
            extra: extra(
                m,
                &[
                    "contextId",
                    "category",
                    "selections",
                    "comments",
                    "reminders",
                    "nothingToSelect",
                    "verseEdits",
                    "invalidated",
                    "status",
                    "userInvalid",
                    "modifiedTimestamp",
                    "userStatus",
                    "verseTextMd5",
                    "pinnedResource",
                    "importedInvalid",
                    "dismissed",
                    "checkKey",
                ],
                &at,
            )?,
        };
        out.push(Record::Check {
            key,
            value: CheckValue::Decision(decision),
        });
    }
    Ok(out)
}

/// The records a USFM book holds (§11.1 X2, §13.5): its header, its chapter fronts, one
/// verse record per verse (a span's first verse holds the text, the others `Covered`), and
/// its layout when the order is not numeric. Verses the file lacks are not records here;
/// the caller compares with what the project holds.
pub fn records_from_usfm(book: BookId, parts: &tc_usfm::codec::BookParts) -> Vec<Record> {
    use crate::record::{ChapterFront, LayoutItem, UsfmBytes, VerseValue};
    let mut out = vec![Record::BookHeader {
        book,
        value: UsfmBytes(parts.header.clone()),
    }];
    for (&c, chapter) in &parts.chapters {
        let Some(chapter_no) = ChapterNo::new(c) else {
            continue;
        };
        if let Some(front) = &chapter.front {
            out.push(Record::ChapterFront {
                book,
                chapter: chapter_no,
                value: ChapterFront::Present(UsfmBytes(front.clone())),
            });
        }
        for (&v, part) in &chapter.verses {
            let Some(verse) = VerseNo::new(v) else {
                continue;
            };
            let value = match part {
                tc_usfm::codec::VersePart::Text(t) => match VerseNo::new(t.last) {
                    Some(last) => VerseValue::Text {
                        marker: t.marker.clone(),
                        words: t.words.clone(),
                        layout: t.layout.clone(),
                        last,
                        from: None,
                    },
                    None => continue,
                },
                tc_usfm::codec::VersePart::Covered { by } => match VerseNo::new(*by) {
                    Some(by) => VerseValue::Covered { by },
                    None => continue,
                },
            };
            out.push(Record::Verse {
                book,
                chapter: chapter_no,
                verse,
                value,
            });
        }
    }
    if let Some(layout) = &parts.layout {
        let items: Vec<LayoutItem> = layout
            .iter()
            .filter_map(|item| match item {
                tc_usfm::codec::LayoutItem::Chapter(c) => {
                    ChapterNo::new(*c).map(LayoutItem::Chapter)
                }
                tc_usfm::codec::LayoutItem::Verse(c, v) => {
                    Some(LayoutItem::Verse(ChapterNo::new(*c)?, VerseNo::new(*v)?))
                }
            })
            .collect();
        out.push(Record::BookLayout {
            book,
            value: Some(items),
        });
    }
    out
}
