//! translationCore 3 projects (`docs/STORAGE.md` §13.4): one or several project zips become one
//! new project, as tC4's tC3 importer makes it (`src/data/import/tc3.ts` at d310bf3, with
//! e79a2795's #796 rule). It imports the current state: the text, the alignments and the
//! decisions. tC3's check history (`checkData`) gives no change of its own, as in tC4.
//!
//! - Text: tC3's own USFM export of the chapter JSON, byte for byte
//!   ([`tc_usfm::tc3_book_usfm`]), then the codec. The records must give the text back byte
//!   for byte, or nothing is imported (P9).
//! - Alignments: `apps/translationCore/alignmentData/<book>/<ch>.json`, one tC4 §5.1 record
//!   per verse, read by `decode_alignment_record` as tC4 wrote it. A record is marked invalid
//!   when tC3 kept an invalid marker, or when it holds a link and its words are not the verse's
//!   words (the text was edited outside tC3; #796). It is kept as tC3 held it.
//! - Decisions: the check index items that hold the translator's work, read by
//!   `decode_decisions`, against the resource the caller pins for each tool.
//! - Resource versions: [`Tc3Import::versions`] lists what each project names; the app looks
//!   them up (a network step) and passes the pins to [`Tc3Import::records`].

use crate::{ImportRefusal, SourceFile};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use tc_core::BookId;
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::fold::{Writer, decode_alignment_record, decode_decisions, records_from_usfm};
use tc_journal::keys::ToolId;
use tc_journal::record::{AlignmentValue, BookPresence, PresentHow, ResourceRef, Scope};
use tc_journal::{ChapterNo, Record, VerseNo};
use tc_usfm::codec::{BookParts, VersePart};
use unicode_normalization::UnicodeNormalization;

const APP: &str = "apps/translationCore";
const TOOLS: [(&str, ToolId); 2] = [
    ("translationNotes", ToolId::TranslationNotes),
    ("translationWords", ToolId::TranslationWords),
];

/// The projects read: what the review page shows and the records they make.
#[derive(Debug, Clone, PartialEq)]
pub struct Tc3Import {
    /// The target language's code.
    pub language: String,
    /// `<language>_<resource>`, lower case: the name tC4 offers.
    pub name: String,
    /// The manifest's `target_language.name`; `None` when it names none (tC4 #832).
    pub language_name: Option<String>,
    /// The manifest's `target_language.direction`; `None` when it is neither `ltr` nor `rtl`.
    pub direction: Option<Direction>,
    /// Each license the projects name, in order; `""` is a project with none. One entry is
    /// the license; several are a question for the person.
    pub licenses: Vec<String>,
    pub contributors: Vec<String>,
    /// The gateway language and its owner the decisions were made in.
    pub gateway: (String, String),
    pub books: Vec<Tc3Book>,
    /// The resource versions the projects name, for the app to look up.
    pub versions: Vec<VersionRequest>,
}

/// The direction a tC3 manifest says the target language reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// One book of the import.
#[derive(Debug, Clone, PartialEq)]
pub struct Tc3Book {
    pub book: BookId,
    /// tC3's export of the book, byte for byte.
    pub usfm: String,
    alignments: Vec<(ChapterNo, VerseNo, AlignmentValue)>,
    /// tC4's §5.2 decision records, by tool, before the resource is known.
    decisions: Vec<(ToolId, Vec<Value>)>,
    /// What the import leaves out, in words: decisions on a check this version has no check
    /// for (a translationNotes row tied to every occurrence (`-1`), which no check list
    /// holds, `tc-check` `SkipReason::EveryOccurrence`), and alignments no verse of the text
    /// can hold. The report lists them; the source keeps them.
    pub not_kept: Vec<String>,
}

impl Tc3Book {
    /// The alignment records, as imported: `marked_invalid` where #796 says so.
    pub fn alignments(&self) -> &[(ChapterNo, VerseNo, AlignmentValue)] {
        &self.alignments
    }

    /// How many decisions of `tool` the book holds.
    pub fn decision_count(&self, tool: ToolId) -> usize {
        self.decisions
            .iter()
            .filter(|(t, _)| *t == tool)
            .map(|(_, d)| d.len())
            .sum()
    }
}

/// Which pin a resource version is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    TranslationNotes,
    TranslationWords,
    OriginalNewTestament,
    OriginalOldTestament,
}

/// A version a project names for one slot of one book; the candidates are tried newest
/// first. No candidate: the project names no version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRequest {
    pub slot: Slot,
    pub book: BookId,
    /// `(repo path, tag)`, such as `("git.door43.org/unfoldingWord/en_tn", "v87")`.
    pub candidates: Vec<(String, String)>,
}

/// The resources the decisions are stored against: the pins the new project gets for the
/// translationNotes tool and for the translationWords tool's links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionPins {
    pub translation_notes: ResourceRef,
    pub translation_words: ResourceRef,
}

/// The files of one zip, in the zip's order.
type Files = Vec<(String, Vec<u8>)>;

fn get<'a>(files: &'a Files, name: &str) -> Option<&'a [u8]> {
    files
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, b)| b.as_slice())
}

fn unzip(file: &SourceFile) -> Result<Files, ImportRefusal> {
    let truncated = || ImportRefusal::Truncated {
        file: file.name.clone(),
    };
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(&file.bytes)).map_err(|_| truncated())?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| truncated())?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        // The zip reader checks each entry's CRC-32 as it ends: tC3 zips have no other check.
        entry
            .read_to_end(&mut bytes)
            .map_err(|_| ImportRefusal::CorruptEntry {
                file: file.name.clone(),
                entry: name.clone(),
            })?;
        out.push((name, bytes));
    }
    Ok(out)
}

/// The files under the project folder: the export is flat, a re-zipped copy may sit under
/// one top-level folder.
fn project_files(files: Files) -> Option<Files> {
    if get(&files, "manifest.json").is_some() {
        return Some(files);
    }
    let folder = files.iter().find_map(|(n, _)| {
        let folder = n.strip_suffix("/manifest.json")?;
        (!folder.contains('/')
            && files
                .iter()
                .any(|(m, _)| m.starts_with(&format!("{folder}/{APP}/"))))
        .then(|| folder.to_owned())
    })?;
    let prefix = format!("{folder}/");
    Some(
        files
            .into_iter()
            .filter_map(|(n, b)| Some((n.strip_prefix(&prefix)?.to_owned(), b)))
            .collect(),
    )
}

fn parse_json(file: &str, name: &str, bytes: &[u8]) -> Result<Value, ImportRefusal> {
    serde_json::from_slice(bytes).map_err(|e| ImportRefusal::Unreadable {
        file: file.to_owned(),
        problem: format!("{name}: {e}"),
    })
}

/// `String(value)` for the values tC3 writes.
fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A file name's number, as JavaScript's `Number` reads `<n>.json`'s stem.
fn stem_number(name: &str) -> Option<u32> {
    let stem = name.strip_suffix(".json")?;
    (!stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()))
        .then(|| stem.parse().ok())
        .flatten()
}

/// tC3's own USFM export of the book's chapter JSON (tC4 `bookUsfm`).
fn book_usfm(file: &str, files: &Files, book: &str) -> Result<Option<String>, ImportRefusal> {
    let mut chapters = Vec::new();
    for (name, bytes) in files {
        let Some(rest) = name.strip_prefix(&format!("{book}/")) else {
            continue;
        };
        let Some(number) = stem_number(rest) else {
            continue;
        };
        let verses = match parse_json(file, name, bytes)? {
            Value::Object(map) => map.into_iter().map(|(k, v)| (k, js_string(&v))).collect(),
            _ => Vec::new(),
        };
        chapters.push((number.to_string(), verses));
    }
    if chapters.is_empty() {
        return Ok(None);
    }
    let headers: Vec<Vec<(String, String)>> = match get(files, &format!("{book}/headers.json")) {
        Some(bytes) => match parse_json(file, "headers.json", bytes)? {
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                        .collect()
                })
                .collect(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    Ok(tc_usfm::tc3_book_usfm(&headers, &chapters))
}

/// Every string of `value` in NFC: journaled content is NFC (I-4), and tC3 keeps the
/// original-language words as the resource spells them (UHB's Hebrew is not NFC).
fn nfc(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(s.nfc().collect()),
        Value::Array(items) => Value::Array(items.into_iter().map(nfc).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, nfc(v))).collect()),
        other => other,
    }
}

/// The verse content (`words + layout`) the book's codec gives for `chapter:verse`.
/// Whether a tC3 alignment record holds a link the translator made. tC3 writes a record
/// with none for every verse it shows, and one keyed `front` for each chapter.
fn holds_links(data: &Value) -> bool {
    data.get("alignments")
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|g| {
                g.get("bottomWords")
                    .and_then(Value::as_array)
                    .is_some_and(|w| !w.is_empty())
            })
        })
}

fn content(parts: &BookParts, chapter: u16, verse: u16) -> Option<String> {
    match parts.chapters.get(&chapter)?.verses.get(&verse)? {
        VersePart::Text(t) => Some(tc_usfm::codec::join_verse(&t.words, &t.layout)),
        VersePart::Covered { .. } => None,
    }
}

/// The text a decision of `chapter:verse` was made on: its verse's, or the span's that
/// covers it.
fn decision_text(parts: &BookParts, chapter: u16, verse: u16) -> Option<String> {
    match parts.chapters.get(&chapter)?.verses.get(&verse)? {
        VersePart::Text(_) => content(parts, chapter, verse),
        VersePart::Covered { by } => content(parts, chapter, *by),
    }
}

/// The word, occurrence and occurrences of each word, as a multiset.
fn word_set<'a>(
    words: impl Iterator<Item = (&'a str, u32, u32)>,
) -> BTreeMap<(String, u32, u32), usize> {
    let mut out = BTreeMap::new();
    for (word, occurrence, occurrences) in words {
        *out.entry((word.to_owned(), occurrence, occurrences))
            .or_insert(0) += 1;
    }
    out
}

/// One verse's §5.1 record from tC3's (tC4 `importedAlignmentRecord`).
fn alignment(
    file: &str,
    data: &Value,
    chapter: u16,
    verse: u16,
    content: &str,
    marked: bool,
    source_version: &str,
) -> Result<(ChapterNo, VerseNo, AlignmentValue), ImportRefusal> {
    let unmerge = |word: &Value| -> Value {
        let mut word = word.clone();
        if let Value::Object(map) = &mut word {
            map.remove("type");
        }
        word
    };
    let alignments: Vec<Value> = data
        .get("alignments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|g| {
                    json!({
                        "topWords": g.get("topWords").cloned().unwrap_or(json!([])),
                        "bottomWords": g
                            .get("bottomWords")
                            .and_then(Value::as_array)
                            .map(|w| w.iter().map(unmerge).collect::<Vec<_>>())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let record = nfc(json!({
        "alignments": alignments,
        "wordBank": data.get("wordBank").cloned().unwrap_or(json!([])),
        "invalid": marked,
        "targetVerseMd5": verse_text_md5_v1(content).to_hex(),
        "sourceVersion": source_version,
    }));
    let at = format!("{chapter}:{verse}");
    let (c, v, mut value) =
        decode_alignment_record(&record, &at, chapter, verse, verse, Writer::Tc4).map_err(|e| {
            ImportRefusal::Unreadable {
                file: file.to_owned(),
                problem: format!("the alignment of {e}"),
            }
        })?;
    let has_links = value.alignments.iter().any(|g| !g.bottom_words.is_empty());
    if has_links {
        let held = word_set(
            value
                .word_bank
                .iter()
                .chain(value.alignments.iter().flat_map(|g| &g.bottom_words))
                .map(|w| (w.word.as_str(), w.occurrence, w.occurrences)),
        );
        let text: String = content.nfc().collect();
        let words = tc_usfm::target_words(&text);
        let verse_words = word_set(
            words
                .iter()
                .map(|w| (w.text.as_str(), w.occurrence, w.occurrences)),
        );
        if held != verse_words {
            value.marked_invalid = true;
        }
    }
    Ok((c, v, value))
}

/// A check index item holds the translator's work (tC4's "touched" rule).
fn touched(item: &Map<String, Value>) -> bool {
    let truthy = |key: &str| match item.get(key) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(_) => true,
    };
    [
        "selections",
        "nothingToSelect",
        "comments",
        "reminders",
        "verseEdits",
        "invalidated",
    ]
    .iter()
    .any(|k| truthy(k))
}

/// One book's decision records per tool, before the resource is known.
fn decisions(
    file: &str,
    files: &Files,
    book: &str,
    parts: &BookParts,
    not_kept: &mut Vec<String>,
) -> Result<Vec<(ToolId, Vec<Value>)>, ImportRefusal> {
    let mut out = Vec::new();
    for (dir_name, tool) in TOOLS {
        let dir = format!("{APP}/index/{dir_name}/{book}/");
        let mut category = BTreeMap::new();
        for (name, bytes) in files {
            let Some(rest) = name.strip_prefix(&format!("{dir}categoryIndex/")) else {
                continue;
            };
            let Some(cat) = rest.strip_suffix(".json") else {
                continue;
            };
            if let Value::Array(groups) = parse_json(file, name, bytes)? {
                for group in groups.iter().filter_map(Value::as_str) {
                    category.insert(group.to_owned(), cat.to_owned());
                }
            }
        }
        let mut names: Vec<&(String, Vec<u8>)> = files
            .iter()
            .filter(|(n, _)| {
                n.strip_prefix(&dir)
                    .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".json"))
            })
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        let mut records = Vec::new();
        for (name, bytes) in names {
            let Value::Array(items) = parse_json(file, name, bytes)? else {
                continue;
            };
            for item in items.iter().filter_map(Value::as_object) {
                if !touched(item) {
                    continue;
                }
                let context = item.get("contextId").cloned().unwrap_or(Value::Null);
                let reference = context.get("reference");
                let number = |key: &str| {
                    reference
                        .and_then(|r| r.get(key))
                        .and_then(|v| match v {
                            Value::Number(n) => n.as_u64(),
                            Value::String(s) => s.parse().ok(),
                            _ => None,
                        })
                        .and_then(|n| u16::try_from(n).ok())
                };
                let occurrence = context.get("occurrence").and_then(Value::as_i64);
                if occurrence.is_some_and(|n| n < 1) {
                    not_kept.push(format!(
                        "{dir_name} {} {}:{} (\"{}\", every occurrence)",
                        book.to_uppercase(),
                        reference
                            .and_then(|r| r.get("chapter"))
                            .map(js_string)
                            .unwrap_or_default(),
                        reference
                            .and_then(|r| r.get("verse"))
                            .map(js_string)
                            .unwrap_or_default(),
                        context.get("checkId").map(js_string).unwrap_or_default(),
                    ));
                    continue;
                }
                let text = match (number("chapter"), number("verse")) {
                    (Some(c), Some(v)) => decision_text(parts, c, v).unwrap_or_default(),
                    _ => String::new(),
                };
                let group = context.get("groupId").map(js_string).unwrap_or_default();
                let flag = |key: &str| item.get(key).cloned().unwrap_or(Value::Bool(false));
                let selections = match item.get("selections") {
                    Some(Value::Array(a)) if !a.is_empty() => Value::Array(a.clone()),
                    _ => Value::Bool(false),
                };
                records.push(json!({
                    "contextId": context,
                    "category": category.get(&group).cloned().unwrap_or_default(),
                    "selections": selections,
                    "comments": flag("comments"),
                    "reminders": flag("reminders"),
                    "nothingToSelect": flag("nothingToSelect"),
                    "verseEdits": flag("verseEdits"),
                    "invalidated": flag("invalidated"),
                    // tC3's own mark that the verse changed under the decision.
                    "importedInvalid": item.get("invalidated").and_then(Value::as_bool).unwrap_or(false),
                    "verseTextMd5": verse_text_md5_v1(&text).to_hex(),
                }));
            }
        }
        if !records.is_empty() {
            out.push((tool, records));
        }
    }
    Ok(out)
}

/// The `(repo path, tag)` of a tC3 resource URL (`…/archive/<tag>.zip`).
fn dcs_ref(url: Option<&Value>) -> Option<(String, String)> {
    let url = url?.as_str()?;
    let rest = url.split("git.door43.org/").nth(1)?;
    let mut parts = rest.splitn(3, '/');
    let (owner, repo, rest) = (parts.next()?, parts.next()?, parts.next()?);
    let tag = rest.strip_prefix("archive/")?.strip_suffix(".zip")?;
    (!owner.is_empty() && !repo.is_empty() && !tag.is_empty())
        .then(|| (format!("git.door43.org/{owner}/{repo}"), tag.to_owned()))
}

/// The versions one project names (tC4 `versionRequests`). `master` is a branch, not a version.
fn version_requests(manifest: &Value, book: BookId, tools: &[ToolId]) -> Vec<VersionRequest> {
    let ext = manifest.get("externalResources");
    let field = |name: &str| ext.and_then(|e| e.get(name));
    let tagged = |r: Option<(String, String)>| r.filter(|(_, v)| v != "master");
    let mut out = Vec::new();
    for tool in tools {
        let (slot, name) = match tool {
            ToolId::TranslationNotes => (Slot::TranslationNotes, "tNotesGateway"),
            _ => (Slot::TranslationWords, "tWordsGateway"),
        };
        out.push(VersionRequest {
            slot,
            book,
            candidates: tagged(dcs_ref(field(name))).into_iter().collect(),
        });
    }
    let refs: Vec<Option<(String, String)>> =
        ["tNotesOriginalLang", "tWordsOriginalLang", "waOriginalLang"]
            .iter()
            .map(|n| dcs_ref(field(n)))
            .collect();
    let old = refs
        .iter()
        .flatten()
        .any(|(path, _)| path.ends_with("/hbo_uhb"));
    let (slot, repo, prefix) = if old {
        (Slot::OriginalOldTestament, "hbo_uhb", "hbo/uhb?v=")
    } else {
        (
            Slot::OriginalNewTestament,
            "el-x-koine_ugnt",
            "el-x-koine/ugnt?v=",
        )
    };
    let mut candidates: Vec<(String, String)> = refs
        .into_iter()
        .filter_map(tagged)
        .filter(|(path, _)| path.ends_with(&format!("/{repo}")))
        .collect();
    let relation = manifest
        .get("tsv_relation")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|r| r.strip_prefix(prefix));
    if let Some(version) = relation {
        candidates.push((
            format!("git.door43.org/unfoldingWord/{repo}"),
            format!("v{version}"),
        ));
    }
    out.push(VersionRequest {
        slot,
        book,
        candidates,
    });
    out
}

struct Project {
    manifest: Value,
    book: Tc3Book,
    versions: Vec<VersionRequest>,
}

fn read_project(file: &SourceFile) -> Result<Project, ImportRefusal> {
    let files = project_files(unzip(file)?).ok_or_else(|| ImportRefusal::NoManifest {
        file: file.name.clone(),
    })?;
    let manifest = parse_json(
        &file.name,
        "manifest.json",
        get(&files, "manifest.json").unwrap_or_default(),
    )?;
    let book = manifest
        .get("project")
        .and_then(|p| p.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    let no_text = || ImportRefusal::NoText {
        file: file.name.clone(),
    };
    let book_id: BookId = book.parse().map_err(|_| no_text())?;
    let usfm = book_usfm(&file.name, &files, &book)?.ok_or_else(no_text)?;
    let parts = tc_usfm::codec::decompose(&usfm).map_err(|e| ImportRefusal::Unreadable {
        file: file.name.clone(),
        problem: e.to_string(),
    })?;
    let source_version = dcs_ref(
        manifest
            .get("externalResources")
            .and_then(|e| e.get("waOriginalLang")),
    )
    .and_then(|(path, version)| {
        let owner_repo = path.split_once('/')?.1.to_owned();
        Some(format!("dcs::{owner_repo}@{version}"))
    })
    .unwrap_or_default();
    let dir = format!("{APP}/alignmentData/{book}/");
    let mut chapter_files: Vec<(u32, &(String, Vec<u8>))> = files
        .iter()
        .filter_map(|f| Some((stem_number(f.0.strip_prefix(&dir)?)?, f)))
        .collect();
    chapter_files.sort_by_key(|(n, _)| *n);
    let mut alignments = Vec::new();
    let mut not_kept = Vec::new();
    let code = book.to_uppercase();
    for (chapter, (name, bytes)) in chapter_files {
        let Ok(chapter) = u16::try_from(chapter) else {
            not_kept.push(format!("wordAlignment {code} {chapter} (no such chapter)"));
            continue;
        };
        let Value::Object(verses) = parse_json(&file.name, name, bytes)? else {
            not_kept.push(format!(
                "wordAlignment {code} {chapter} (not a chapter of verses)"
            ));
            continue;
        };
        let mut numbered: Vec<(u16, &Value)> = Vec::new();
        for (key, data) in &verses {
            match key.parse() {
                Ok(verse) => numbered.push((verse, data)),
                Err(_) if !holds_links(data) => {}
                Err(_) => not_kept.push(format!(
                    "wordAlignment {code} {chapter}:{key} (no such verse in the text)"
                )),
            }
        }
        numbered.sort_by_key(|(n, _)| *n);
        for (verse, data) in numbered {
            // No such verse in the text, or one a span covers: nothing to key the record to.
            let Some(text) = content(&parts, chapter, verse) else {
                if holds_links(data) {
                    not_kept.push(format!(
                        "wordAlignment {code} {chapter}:{verse} (no such verse in the text)"
                    ));
                }
                continue;
            };
            let marked = get(
                &files,
                &format!("{APP}/tools/wordAlignment/invalid/{chapter}/{verse}.json"),
            )
            .is_some();
            alignments.push(alignment(
                &file.name,
                data,
                chapter,
                verse,
                &text,
                marked,
                &source_version,
            )?);
        }
    }
    let decisions = decisions(&file.name, &files, &book, &parts, &mut not_kept)?;
    let tools: Vec<ToolId> = decisions.iter().map(|(t, _)| *t).collect();
    let versions = version_requests(&manifest, book_id, &tools);
    Ok(Project {
        manifest,
        book: Tc3Book {
            book: book_id,
            usfm,
            alignments,
            decisions,
            not_kept,
        },
        versions,
    })
}

fn text_at(manifest: &Value, path: &[&str]) -> Option<String> {
    let mut v = manifest;
    for p in path {
        v = v.get(p)?;
    }
    v.as_str().map(str::to_owned)
}

/// Reads one or several tC3 project zips into one import (tC4 `parse`). A damaged zip,
/// projects in different languages or one book twice refuse the whole import.
pub fn read(files: &[SourceFile]) -> Result<Tc3Import, ImportRefusal> {
    let mut projects = files
        .iter()
        .map(read_project)
        .collect::<Result<Vec<_>, _>>()?;
    let languages: BTreeSet<String> = projects
        .iter()
        .map(|p| text_at(&p.manifest, &["target_language", "id"]).unwrap_or_default())
        .collect();
    if languages.len() > 1 {
        return Err(ImportRefusal::Languages(languages.into_iter().collect()));
    }
    let mut seen = BTreeSet::new();
    for p in &projects {
        if !seen.insert(p.book.book) {
            return Err(ImportRefusal::BookTwice {
                book: p.book.book.code().to_owned(),
            });
        }
    }
    projects.sort_by(|a, b| a.book.book.code().cmp(b.book.book.code()));
    let mut licenses: Vec<String> = Vec::new();
    for p in &projects {
        let license = text_at(&p.manifest, &["license"])
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !licenses.contains(&license) {
            licenses.push(license);
        }
    }
    let mut contributors: Vec<String> = Vec::new();
    for p in &projects {
        for key in ["translators", "checkers"] {
            for c in p
                .manifest
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let c = js_string(c);
                if !contributors.contains(&c) {
                    contributors.push(c);
                }
            }
        }
    }
    let first = projects.first().map(|p| &p.manifest);
    let name = first
        .map(|m| {
            [
                text_at(m, &["target_language", "id"]),
                text_at(m, &["resource", "id"]),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase()
        })
        .unwrap_or_default();
    let language_name = first
        .and_then(|m| text_at(m, &["target_language", "name"]))
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty());
    let direction = first
        .and_then(|m| text_at(m, &["target_language", "direction"]))
        .and_then(|d| match d.trim().to_ascii_lowercase().as_str() {
            "ltr" => Some(Direction::Ltr),
            "rtl" => Some(Direction::Rtl),
            _ => None,
        });
    let tool_text = |key: &str| {
        projects.iter().find_map(|p| {
            text_at(&p.manifest, &[key, "translationNotes"])
                .or_else(|| text_at(&p.manifest, &[key, "translationWords"]))
        })
    };
    let gateway = (
        tool_text("toolsSelectedGLs").unwrap_or_else(|| "en".to_owned()),
        tool_text("toolsSelectedOwners").unwrap_or_else(|| "unfoldingWord".to_owned()),
    );
    Ok(Tc3Import {
        language: languages.into_iter().next().unwrap_or_default(),
        name,
        language_name,
        direction,
        licenses,
        contributors,
        gateway,
        versions: projects.iter().flat_map(|p| p.versions.clone()).collect(),
        books: projects.into_iter().map(|p| p.book).collect(),
    })
}

impl Tc3Import {
    /// The records of every book: its presence, its text, its alignments and its decisions,
    /// the decisions against `pins`. Refused when a book's records would not give its text
    /// back byte for byte (P9).
    pub fn records(&self, pins: &DecisionPins) -> Result<Vec<Record>, ImportRefusal> {
        let mut out = Vec::new();
        for book in &self.books {
            let code = book.book.code().to_owned();
            let unreadable = |problem: String| ImportRefusal::Unreadable {
                file: code.clone(),
                problem,
            };
            let parts =
                tc_usfm::codec::decompose(&book.usfm).map_err(|e| unreadable(e.to_string()))?;
            if tc_usfm::codec::recompose(&parts).usfm != book.usfm {
                return Err(ImportRefusal::TextChanged { book: code });
            }
            out.push(Record::BookPresence {
                book: book.book,
                value: BookPresence::Present {
                    scope: Scope::Whole,
                    how: PresentHow::Added,
                },
            });
            out.extend(records_from_usfm(book.book, &parts));
            for (chapter, verse, value) in &book.alignments {
                out.push(Record::Alignment {
                    book: book.book,
                    chapter: *chapter,
                    verse: *verse,
                    value: value.clone(),
                });
            }
            for (tool, decisions) in &book.decisions {
                let pin = match tool {
                    ToolId::TranslationNotes => &pins.translation_notes,
                    _ => &pins.translation_words,
                };
                let file = json!({
                    "resource": {"repoPath": &*pin.repo_path, "sha": pin.sha.to_hex()},
                    "decisions": decisions,
                });
                let bytes = serde_json::to_vec(&file).map_err(|e| unreadable(e.to_string()))?;
                out.extend(
                    decode_decisions(*tool, Some(book.book), &bytes)
                        .map_err(|e| unreadable(format!("a decision: {e}")))?,
                );
            }
        }
        Ok(out)
    }
}
