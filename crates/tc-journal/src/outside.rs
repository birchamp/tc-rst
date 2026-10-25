//! Edits made outside the app (`docs/STORAGE.md` §11.1 X2 and X3): a file of the project
//! that a program other than translationCore changed, decoded into records and compared with
//! what the app wrote, as a pure function. `tc-store` reads the bytes, keeps the copy and
//! commits the plan; nothing here does I/O.
//!
//! The plan never contains a change whose value equals what the app wrote or what the
//! project already shows, so examining the same bytes twice records nothing the second time
//! (the crash rows X1 and X2 of §6.4).
//!
//! **Cost.** A heads file keeps, for each record of a book's USFM, a digest of how the written
//! file showed it ([`ShownDigest`]). A key whose decoded digest equals its base digest shows
//! what the app wrote and is not looked at again, so reconciling an edit of a whole book costs
//! the keys that differ, after one decode of the file.

use crate::canon;
use crate::change::{Cause, ExternalSource};
use crate::command::PlannedChange;
use crate::fold::{
    DerivedFile, FoldState, SidecarError, decode_alignments, decode_comments, decode_decisions,
    decode_metadata, decode_resources, decode_settings, decode_versification, records_from_usfm,
};
use crate::ids::{ChangeId, ChapterNo, CommentId, Md5, RepoPath, VerseNo};
use crate::keys::{MetaField, RecordKey};
use crate::record::{
    BookPresence, ChapterFront, CommentTarget, CommentValue, PresentHow, Record, Scope, VerseValue,
};
use std::collections::{BTreeMap, BTreeSet};
use tc_core::BookId;
use tc_usfm::codec::{BookParts, VersePart};

/// The digest of how a written file shows one record: md5 of its piece of the file after
/// [`normalize_ws`].
pub type ShownDigest = [u8; 16];

/// More changed keys than this in one file are recorded without parents (§11.1 X3).
pub const MANY_KEYS: usize = 50;

/// Line endings turned into LF and spaces and tabs before a line end (and at the end of the
/// text) removed: the differences an editor that rewrites line endings or trims lines makes
/// in every line of a file without changing a word (§11.1 X3).
pub fn normalize_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut blanks = 0usize;
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                blanks += 1;
                out.push(c);
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.truncate(out.len() - blanks);
                blanks = 0;
                out.push('\n');
            }
            other => {
                blanks = 0;
                out.push(other);
            }
        }
    }
    out.truncate(out.len() - blanks);
    out
}

/// The digest of a piece of a file, normalized.
pub fn shown_digest(piece: &str) -> ShownDigest {
    *Md5::of(normalize_ws(piece).as_bytes()).as_bytes()
}

/// The verse a span written for `v` ends at: `recompose` labels `\v v-e` with the end of the
/// run of `Covered { by: v }` verses after it, whatever `last` says (§7.5).
fn written_end(v: u16, last: u16, chapter: &tc_usfm::codec::ChapterParts) -> u16 {
    let mut end = v;
    while end < last
        && matches!(chapter.verses.get(&(end + 1)), Some(VersePart::Covered { by }) if *by == v)
    {
        end += 1;
    }
    end
}

fn verse_digest(v: u16, part: &VersePart, chapter: &tc_usfm::codec::ChapterParts) -> ShownDigest {
    match part {
        VersePart::Text(t) => {
            let mut piece = String::with_capacity(t.words.len() + t.layout.len() + 16);
            piece.push_str("T\0");
            piece.push_str(t.marker.lead());
            piece.push('\0');
            piece.push(t.marker.term().as_char());
            piece.push('\0');
            piece.push_str(&written_end(v, t.last, chapter).to_string());
            piece.push('\0');
            piece.push_str(&t.words);
            piece.push_str(&t.layout);
            shown_digest(&piece)
        }
        VersePart::Covered { by } => shown_digest(&format!("C\0{by}")),
    }
}

/// The digest of every record a book's USFM shows: its header, each chapter's front (a
/// chapter with no front shows `\c N`, as `recompose` writes it) and each verse. The book's
/// order is not digested: a file shows the order only when it is not numeric, and a layout
/// that lists a chapter twice is written in numeric order, so the decoder cannot say what
/// the renderer meant (the layout is one key, looked at when the file shows one). The
/// renderer takes it from the parts it writes and the decoder
/// from the parts it reads, with this one function, so a record the file shows as the app
/// wrote it has equal digests.
pub fn usfm_digests(book: BookId, parts: &BookParts) -> Vec<(RecordKey, ShownDigest)> {
    let mut out = Vec::with_capacity(2 + parts.chapters.len() * 40);
    out.push((RecordKey::BookHeader(book), shown_digest(&parts.header)));
    for (&c, chapter) in &parts.chapters {
        let Some(chapter_no) = ChapterNo::new(c) else {
            continue;
        };
        // `recompose` writes a chapter that has a front or a verse, and no other.
        if chapter.front.is_none() && chapter.verses.is_empty() {
            continue;
        }
        let front = match &chapter.front {
            Some(front) => shown_digest(front),
            None => shown_digest(&format!("\\c {c}\n")),
        };
        out.push((RecordKey::ChapterFront(book, chapter_no), front));
        for (&v, part) in &chapter.verses {
            if let Some(verse) = VerseNo::new(v) {
                out.push((
                    RecordKey::Verse(book, chapter_no, verse),
                    verse_digest(v, part, chapter),
                ));
            }
        }
    }
    out
}

/// A file of the project an outside edit can change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutsideFile {
    Derived(DerivedFile),
    /// `metadata.json`: the fields its records hold (§11.1 X2).
    Metadata,
}

impl OutsideFile {
    /// The file's path in the project.
    pub fn path(&self) -> String {
        match self {
            Self::Derived(file) => file.path(),
            Self::Metadata => "metadata.json".into(),
        }
    }

    fn book(&self) -> Option<BookId> {
        match self {
            Self::Derived(file) => file.book(),
            Self::Metadata => None,
        }
    }

    /// Whether the file can show a record of `key`. A scope lists more records than a file
    /// shows (a book's presence, the device profiles and checks a comment is placed by):
    /// those are not the file's content, and their absence from it is no removal.
    pub fn shows(&self, key: &RecordKey) -> bool {
        match (self, key) {
            (Self::Metadata, RecordKey::ProjectMeta(_)) => true,
            (Self::Metadata, _) => false,
            (Self::Derived(file), key) => match (file, key) {
                (
                    DerivedFile::Usfm(book),
                    RecordKey::BookHeader(b)
                    | RecordKey::ChapterFront(b, _)
                    | RecordKey::Verse(b, ..)
                    | RecordKey::BookLayout(b),
                ) => b == book,
                (DerivedFile::Alignments(book), RecordKey::Alignment(b, ..)) => b == book,
                (DerivedFile::Decisions(tool, book), RecordKey::Check(check)) => {
                    check.tool == *tool && check.book() == Some(*book)
                }
                (DerivedFile::StoryDecisions(tool), RecordKey::Check(check)) => {
                    check.tool == *tool && check.book().is_none()
                }
                (DerivedFile::Comments(_) | DerivedFile::StoryComments, RecordKey::Comment(_)) => {
                    true
                }
                (DerivedFile::Resources, RecordKey::Pins(_) | RecordKey::ResourcesExtra(_)) => true,
                (DerivedFile::Settings, RecordKey::Setting(_)) => true,
                (DerivedFile::Versification, RecordKey::Versification) => true,
                (
                    DerivedFile::Story(story),
                    RecordKey::StoryBase(s) | RecordKey::Frame(s, _) | RecordKey::StoryRef(s),
                ) => s == story,
                _ => false,
            },
        }
    }
}

/// A file decoded: the records it shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decoded {
    pub records: Vec<Record>,
    /// How the file shows each record, where the file kind has digests (a book's USFM);
    /// empty otherwise.
    pub digests: Vec<(RecordKey, ShownDigest)>,
    /// Layout the records do not hold, in parts: a story whose blank lines are not the
    /// writers' (X3's whitespace-only notice).
    pub story_reformatted: u32,
    /// Fields of `metadata.json` that the builder derives and the file shows (for the caller
    /// to compare with the document it would write).
    pub derived_fields: u32,
}

fn problem(error: SidecarError) -> String {
    error.to_string()
}

/// The id of a comment the file shows without one the app knows: the same for the same file
/// path, target and text, so examining the same bytes again finds the comment already there.
fn derived_comment_id(path: &RepoPath, target: &CommentTarget, text: &str) -> CommentId {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"outside-comment\0");
    bytes.extend_from_slice(path.as_str().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(
        canon::encode_comment_target(target)
            .to_canonical_string()
            .as_bytes(),
    );
    bytes.push(0);
    bytes.extend_from_slice(text.as_bytes());
    let digest = crate::sha::sha256(&bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    CommentId::from_bytes(id)
}

/// Decodes the bytes of `file`, found at `path`. The error is in words for the person
/// ("line 212: verse 7 appears twice"); the file's bytes are kept by the caller. An OBS
/// story is decoded by the caller (`tc-obs` owns that format).
pub fn decode_file(file: &OutsideFile, bytes: &[u8], path: &RepoPath) -> Result<Decoded, String> {
    let mut decoded = Decoded::default();
    let derived = match file {
        OutsideFile::Metadata => {
            let doc = decode_metadata(bytes).map_err(problem)?;
            decoded.derived_fields = u32::try_from(doc.derived.len()).unwrap_or(u32::MAX);
            decoded.records = doc
                .fields
                .into_iter()
                .map(|(field, value)| Record::ProjectMeta {
                    field,
                    value: Some(value),
                })
                .collect();
            return Ok(decoded);
        }
        OutsideFile::Derived(file) => file,
    };
    match derived {
        DerivedFile::Usfm(book) => {
            let parts = tc_usfm::codec::decompose_bytes(bytes).map_err(|e| e.to_string())?;
            decoded.digests = usfm_digests(*book, &parts);
            decoded.records = records_from_usfm(*book, &parts);
        }
        DerivedFile::Alignments(book) => {
            decoded.records = decode_alignments(*book, bytes).map_err(problem)?;
        }
        DerivedFile::Decisions(tool, book) => {
            decoded.records = decode_decisions(*tool, Some(*book), bytes).map_err(problem)?;
        }
        DerivedFile::StoryDecisions(tool) => {
            decoded.records = decode_decisions(*tool, None, bytes).map_err(problem)?;
        }
        DerivedFile::Comments(_) | DerivedFile::StoryComments => {
            let comments = decode_comments(derived.book(), bytes).map_err(problem)?;
            let mut seen = BTreeSet::new();
            for c in comments {
                let id =
                    c.id.unwrap_or_else(|| derived_comment_id(path, &c.target, &c.text));
                if !seen.insert(id) {
                    return Err(format!("the comment {id} appears twice"));
                }
                decoded.records.push(Record::Comment {
                    id,
                    value: CommentValue {
                        target: c.target,
                        text: c.text,
                    },
                });
            }
        }
        DerivedFile::Resources => decoded.records = decode_resources(bytes).map_err(problem)?,
        DerivedFile::Settings => decoded.records = decode_settings(bytes).map_err(problem)?,
        DerivedFile::Versification => {
            decoded.records = vec![decode_versification(bytes, None).map_err(problem)?];
        }
        DerivedFile::Conflicts => {
            return Err("conflicts.json is written by translationCore only".into());
        }
        DerivedFile::Story(_) => {
            return Err("a story is decoded by the caller".into());
        }
    }
    Ok(decoded)
}

/// The fields of a decoded `metadata.json` worth looking at: those whose value differs from
/// the document the app would write now, and those the project holds a record for
/// (`held`), whatever their value, so that a record the file keeps is not read as one it
/// lost. A file also holds fields the records do not (the defaults the builder fills in), and
/// those are not edits.
pub fn fields_not_shown(
    found: Vec<Record>,
    shown: &[Record],
    held: impl Fn(&RecordKey) -> bool,
) -> Vec<Record> {
    let shown: BTreeMap<RecordKey, String> =
        shown.iter().map(|r| (r.key(), canon_string(r))).collect();
    found
        .into_iter()
        .filter(|r| {
            let key = r.key();
            held(&key) || shown.get(&key) != Some(&canon_string(r))
        })
        .collect()
}

fn canon_string(record: &Record) -> String {
    canon::encode_record_json(record)
        .map(|j| j.to_canonical_string())
        .unwrap_or_default()
}

/// What the file was made from for one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseRecord {
    pub key: RecordKey,
    /// The heads the written bytes stand for (§2.7); empty for a key the file shows with no
    /// record behind it (a chapter with no front).
    pub heads: Vec<ChangeId>,
    /// How the written file showed it, when the heads file keeps that.
    pub shown: Option<ShownDigest>,
}

/// What the file was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The heads file's records, or, for `metadata.json`, [`FoldState::metadata_heads`] as
    /// they were when it was written.
    Heads(Vec<BaseRecord>),
    /// No heads file, no manifest entry and no usable HEAD, or a held file taken in or
    /// changed again (§6.2): the project's records stand for what was written, and every
    /// change is recorded without parents.
    Unknown,
}

/// A value read from the fold for the heads a file was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShownValue {
    Value(Box<Record>),
    /// No record was written.
    Nothing,
    /// A value the computer cannot read.
    Unreadable,
    /// Several heads that differ and are not the current merge.
    Conflicting,
}

/// What an examination of a file decided.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutsidePlan {
    /// One change per changed key, in key order, each with `Cause::External`.
    pub changes: Vec<PlannedChange>,
    /// Any change was planned without parents by the rules of §11.1 X3.
    pub unknown_base: bool,
    /// Keys that differ from what was written only in line ends or blanks at line ends.
    pub whitespace_only: u32,
    /// Records the file no longer shows that no change can remove (alignments, decisions and
    /// comments): they stay, and the file is written again with them.
    pub kept_removals: Vec<RecordKey>,
    /// The book a book file added (§7.6).
    pub book_added: Option<BookId>,
    /// Keys compared value by value; the count tests read it.
    pub keys_examined: u32,
}

/// The value that stands for "this key is not in the file", for the kinds that have one.
fn removal_of(key: &RecordKey) -> Option<Record> {
    Some(match key {
        RecordKey::Verse(book, chapter, verse) => Record::Verse {
            book: *book,
            chapter: *chapter,
            verse: *verse,
            value: VerseValue::Absent { moved_to: None },
        },
        RecordKey::ChapterFront(book, chapter) => Record::ChapterFront {
            book: *book,
            chapter: *chapter,
            value: ChapterFront::Absent,
        },
        RecordKey::BookLayout(book) => Record::BookLayout {
            book: *book,
            value: None,
        },
        RecordKey::Setting(key) => Record::Setting {
            key: key.clone(),
            value: None,
        },
        RecordKey::ProjectMeta(field) => Record::ProjectMeta {
            field: field.clone(),
            value: None,
        },
        RecordKey::ResourcesExtra(key) => Record::ResourcesExtra {
            key: key.clone(),
            value: None,
        },
        RecordKey::StoryRef(story) => Record::StoryRef {
            story: *story,
            value: None,
        },
        _ => return None,
    })
}

/// A comparison string of a record: equal strings are equal values. With `ws`, the text of
/// a verse, header, front or comment is compared after [`normalize_ws`], and a verse's words
/// and layout are compared as the one run of bytes they are in the file.
fn compare_key(record: &Record, ws: bool) -> String {
    let text = |s: &str| if ws { normalize_ws(s) } else { s.to_owned() };
    match record {
        Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    words,
                    layout,
                    last,
                    ..
                },
            ..
        } => format!(
            "T\0{}\0{:?}\0{last}\0{}",
            marker.lead(),
            marker.term(),
            text(&format!("{words}{layout}"))
        ),
        Record::Verse {
            value: VerseValue::Covered { by },
            ..
        } => format!("C\0{by}"),
        Record::Verse {
            value: VerseValue::Absent { .. },
            ..
        } => "A".to_owned(),
        Record::BookHeader { value, .. } => format!("H\0{}", text(&value.0)),
        Record::ChapterFront {
            value: ChapterFront::Present(bytes),
            ..
        } => format!("F\0{}", text(&bytes.0)),
        Record::ChapterFront {
            value: ChapterFront::Absent,
            ..
        } => "A".to_owned(),
        Record::Comment { value, .. } => format!(
            "M\0{}\0{}",
            canon::encode_comment_target(&value.target).to_canonical_string(),
            text(&value.text)
        ),
        other => canon_string(other),
    }
}

/// `found` with the line endings of `written` when `written` has CRLF and `found` none, so
/// a fixed typo is the only difference recorded (§11.1 X3).
fn crlf_back(found: Record, written: &Record) -> Record {
    let crlf = |s: &str| s.contains("\r\n");
    let back = |s: String| {
        if s.contains('\r') {
            s
        } else {
            s.replace('\n', "\r\n")
        }
    };
    match (found, written) {
        (
            Record::Verse {
                book,
                chapter,
                verse,
                value:
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        last,
                        from,
                    },
            },
            Record::Verse {
                value:
                    VerseValue::Text {
                        words: w_words,
                        layout: w_layout,
                        ..
                    },
                ..
            },
        ) if crlf(w_words) || crlf(w_layout) => Record::Verse {
            book,
            chapter,
            verse,
            value: VerseValue::Text {
                marker,
                words: back(words),
                layout: back(layout),
                last,
                from,
            },
        },
        (Record::BookHeader { book, value }, Record::BookHeader { value: written, .. })
            if crlf(&written.0) =>
        {
            Record::BookHeader {
                book,
                value: crate::record::UsfmBytes(back(value.0)),
            }
        }
        (
            Record::ChapterFront {
                book,
                chapter,
                value: ChapterFront::Present(bytes),
            },
            Record::ChapterFront {
                value: ChapterFront::Present(written),
                ..
            },
        ) if crlf(&written.0) => Record::ChapterFront {
            book,
            chapter,
            value: ChapterFront::Present(crate::record::UsfmBytes(back(bytes.0))),
        },
        (found, _) => found,
    }
}

/// The parts of `written` that a file cannot show, copied into `found` (§11.1 X3): where a
/// renumber moved a verse to or from, and the name of a versification.
fn carry_unshown(found: Record, written: &Record) -> Record {
    match (found, written) {
        (
            Record::Verse {
                book,
                chapter,
                verse,
                value:
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        last,
                        ..
                    },
            },
            Record::Verse {
                value: VerseValue::Text { from, .. },
                ..
            },
        ) => Record::Verse {
            book,
            chapter,
            verse,
            value: VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from: *from,
            },
        },
        (
            Record::Verse {
                book,
                chapter,
                verse,
                value: VerseValue::Absent { .. },
            },
            Record::Verse {
                value: VerseValue::Absent { moved_to },
                ..
            },
        ) => Record::Verse {
            book,
            chapter,
            verse,
            value: VerseValue::Absent {
                moved_to: *moved_to,
            },
        },
        (Record::Versification { value }, Record::Versification { value: written }) => {
            Record::Versification {
                value: crate::record::VersificationValue {
                    name: written.name.clone(),
                    bytes: value.bytes,
                },
            }
        }
        (found, _) => found,
    }
}

impl FoldState {
    /// The value the heads `heads` of `key` stand for: the value the written file showed
    /// (`written(k)` of §11.1 X3). Those changes are never deleted, so their records are
    /// read; when the heads are the record's current ones, its projection is (a merged
    /// record shows its merged value).
    pub fn shown_value(&self, key: &RecordKey, heads: &[ChangeId]) -> ShownValue {
        if heads.is_empty() {
            return ShownValue::Nothing;
        }
        let mut want = heads.to_vec();
        want.sort();
        want.dedup();
        let mut now = self.shown_heads(key, None);
        now.sort();
        if want == now {
            return match self.projected(key) {
                Some(record) => ShownValue::Value(Box::new(record)),
                None if self.unreadable_records().contains(key) => ShownValue::Unreadable,
                None => ShownValue::Nothing,
            };
        }
        let mut records = Vec::with_capacity(want.len());
        for head in &want {
            match self.body(head) {
                Some(body) => records.push(body.record),
                None => return ShownValue::Unreadable,
            }
        }
        let first = canon_string(&records[0]);
        if records.iter().all(|r| canon_string(r) == first) {
            ShownValue::Value(Box::new(records.swap_remove(0)))
        } else {
            ShownValue::Conflicting
        }
    }

    /// Decides what an outside edit of `file` changes (§11.1 X3). `decoded` is the file as
    /// the decoder read it, `base` what the app wrote, `source` names the file and its md5 in
    /// every change's cause, and `force_unknown` records every change without parents (the
    /// person took in a held file, or it changed again, §6.2).
    ///
    /// The rules, in order, each with a test in `tests/outside.rs`:
    ///
    /// 1. the keys are those the file shows and those the base lists;
    /// 2. a key whose digest equals its base digest is not looked at (the cost follows the
    ///    changed keys); without digests every key is;
    /// 3. `written(k)` is [`Self::shown_value`] of the base heads; one that cannot be read or
    ///    is several different heads makes the key unknown-base;
    /// 4. a value equal to `written(k)` once line endings and blanks at line ends are
    ///    normalized is not a change, and is counted when the bytes differ;
    /// 5. a changed value takes CRLF back when `written(k)` had it and the file has none;
    /// 6. what a file cannot show is carried over from `written(k)`;
    /// 7. a value the project shows already is not a change;
    /// 8. a record the file lost that has no removal value is kept and counted; one that has
    ///    is a change;
    /// 9. a removal of a value, more than [`MANY_KEYS`] changed keys, an unknown base, and a
    ///    value an older version of the record held are recorded without parents;
    /// 10. a comment the file changes is a new comment with no parents, and a conflict with
    ///     the one it edits (a comment change cannot name parents, V6);
    /// 11. a book file of a book the project does not show adds the book.
    pub fn plan_outside_edit(
        &self,
        file: &OutsideFile,
        decoded: &Decoded,
        base: &Base,
        source: ExternalSource,
        force_unknown: bool,
    ) -> OutsidePlan {
        let mut plan = OutsidePlan::default();
        let derived_base;
        let (records, base_unknown): (&[BaseRecord], bool) = match base {
            Base::Heads(records) => (records, false),
            Base::Unknown => {
                derived_base = self.current_base(file);
                (&derived_base, true)
            }
        };
        let unknown_all = base_unknown || force_unknown;
        let base_of: BTreeMap<&RecordKey, &BaseRecord> = records
            .iter()
            .filter(|b| file.shows(&b.key))
            .map(|b| (&b.key, b))
            .collect();
        let found: BTreeMap<RecordKey, &Record> =
            decoded.records.iter().map(|r| (r.key(), r)).collect();
        let found_digest: BTreeMap<&RecordKey, &ShownDigest> =
            decoded.digests.iter().map(|(k, d)| (k, d)).collect();
        // Rule 1 and 2.
        let mut examine: BTreeSet<RecordKey> = BTreeSet::new();
        for key in found.keys() {
            if let (Some(d), Some(Some(b))) =
                (found_digest.get(key), base_of.get(key).map(|b| &b.shown))
                && *d == b
            {
                continue;
            }
            examine.insert(key.clone());
        }
        for (key, b) in &base_of {
            // A file shows the order of a book only when it is not numeric (§7.5): its
            // absence from the file is no removal of the layout.
            if found.contains_key(*key) || matches!(key, RecordKey::BookLayout(_)) {
                continue;
            }
            // A digest in the base for a key the file's digests do not list: the file lost
            // the record. A key the file shows with a digest but no record behind it (a
            // chapter with no front) is equal when the digests are.
            if let (Some(d), Some(base_digest)) = (found_digest.get(*key), b.shown)
                && **d == base_digest
            {
                continue;
            }
            examine.insert((*key).clone());
        }
        plan.keys_examined = u32::try_from(examine.len()).unwrap_or(u32::MAX);

        struct Candidate {
            record: Record,
            heads: Vec<ChangeId>,
            key_unknown: bool,
            removal: bool,
            comment_edit: bool,
        }
        let mut candidates: BTreeMap<RecordKey, Candidate> = BTreeMap::new();
        for key in &examine {
            let heads: Vec<ChangeId> = base_of
                .get(key)
                .map(|b| b.heads.clone())
                .unwrap_or_default();
            let written = self.shown_value(key, &heads);
            let key_unknown = matches!(written, ShownValue::Unreadable | ShownValue::Conflicting);
            let removal_value = removal_of(key);
            let written_record = match &written {
                ShownValue::Value(r) => Some(&**r),
                _ => None,
            };
            let (found_record, removal) = match found.get(key) {
                Some(r) => ((*r).clone(), false),
                None => match &removal_value {
                    Some(r) => (r.clone(), true),
                    None => {
                        // Rule 8: an alignment, decision or comment cannot be removed by a
                        // file. It stays, and the file is written again with it.
                        if !matches!(written, ShownValue::Nothing) {
                            plan.kept_removals.push(key.clone());
                        }
                        continue;
                    }
                },
            };
            // Rule 4.
            let unchanged = match written_record {
                Some(w) => compare_key(&found_record, true) == compare_key(w, true),
                None => {
                    matches!(written, ShownValue::Nothing)
                        && removal_value.as_ref().is_some_and(|r| {
                            compare_key(&found_record, true) == compare_key(r, true)
                        })
                }
            };
            if unchanged {
                if let Some(w) = written_record
                    && compare_key(&found_record, false) != compare_key(w, false)
                {
                    plan.whitespace_only += 1;
                }
                continue;
            }
            // Rules 5 and 6.
            let mut value = found_record;
            if let Some(w) = written_record {
                value = carry_unshown(crlf_back(value, w), w);
            }
            // Rule 7.
            let projected = self.projected(key);
            let shown_equal = match &projected {
                Some(p) => compare_key(&value, true) == compare_key(p, true),
                None => removal_of(key)
                    .is_some_and(|r| compare_key(&value, true) == compare_key(&r, true)),
            };
            if shown_equal {
                continue;
            }
            // The comment rules (10): a comment the project holds, edited, is a new
            // version that names no parents.
            let comment_edit = matches!(key, RecordKey::Comment(_)) && !heads.is_empty();
            candidates.insert(
                key.clone(),
                Candidate {
                    record: value,
                    heads,
                    key_unknown,
                    removal,
                    comment_edit,
                },
            );
        }
        // A book file of an absent book adds the book (rule 11).
        let mut presence: Option<PlannedChange> = None;
        if let Some(book) = file.book()
            && !self.book(book).projects()
            && !candidates.is_empty()
        {
            plan.book_added = Some(book);
            presence = Some(PlannedChange {
                record: Record::BookPresence {
                    book,
                    value: BookPresence::Present {
                        scope: Scope::Whole,
                        how: PresentHow::Added,
                    },
                },
                parents: Vec::new(),
                cause: Cause::External {
                    source: source.clone(),
                    unknown_base: false,
                },
            });
        }
        // Rule 9.
        let removed_value = candidates
            .iter()
            .any(|(_, c)| c.removal && !c.heads.is_empty())
            || (!plan.kept_removals.is_empty() && !candidates.is_empty());
        let unknown_file = unknown_all || removed_value || candidates.len() > MANY_KEYS;
        for (key, candidate) in candidates {
            let mut unknown = unknown_file || candidate.key_unknown || candidate.comment_edit;
            if !unknown && !candidate.heads.is_empty() {
                let wanted = compare_key(&candidate.record, true);
                unknown = self.history(&key).into_iter().any(|id| {
                    !candidate.heads.contains(&id)
                        && self
                            .body(&id)
                            .is_some_and(|b| compare_key(&b.record, true) == wanted)
                });
            }
            let parents = if unknown { Vec::new() } else { candidate.heads };
            plan.unknown_base |= unknown;
            plan.changes.push(PlannedChange {
                record: candidate.record,
                parents,
                cause: Cause::External {
                    source: source.clone(),
                    unknown_base: unknown,
                },
            });
        }
        plan.changes.extend(presence);
        plan.changes.sort_by_key(|c| c.record.key());
        plan
    }

    /// The base an unknown-base examination compares with: the records the project reads for
    /// `file`, with their current heads.
    fn current_base(&self, file: &OutsideFile) -> Vec<BaseRecord> {
        let heads = match file {
            OutsideFile::Derived(file) => self.file_heads(file),
            OutsideFile::Metadata => self.metadata_heads(),
        };
        heads
            .into_iter()
            .map(|(key, heads)| BaseRecord {
                key,
                heads,
                shown: None,
            })
            .collect()
    }
}

/// The records of `parts` as the base of a book's USFM, with the digests the renderer takes:
/// what a heads file is made of.
pub fn base_records(
    heads: Vec<(RecordKey, Vec<ChangeId>)>,
    digests: Vec<(RecordKey, ShownDigest)>,
) -> Vec<BaseRecord> {
    let mut shown: BTreeMap<RecordKey, ShownDigest> = digests.into_iter().collect();
    let mut out: Vec<BaseRecord> = heads
        .into_iter()
        .map(|(key, heads)| {
            let digest = shown.remove(&key);
            BaseRecord {
                key,
                heads,
                shown: digest,
            }
        })
        .collect();
    // A key the file shows with no record behind it keeps its digest.
    out.extend(shown.into_iter().map(|(key, digest)| BaseRecord {
        key,
        heads: Vec::new(),
        shown: Some(digest),
    }));
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// The field of `metadata.json` a record stands for, for the caller's notices.
pub fn meta_field_of(record: &Record) -> Option<&MetaField> {
    match record {
        Record::ProjectMeta { field, .. } => Some(field),
        _ => None,
    }
}

/// What "Update a book from USFM" does with a file chosen from anywhere (§13.6 with no base,
/// `rulings/2026-10-22-H20.md`). Nothing is removed and nothing newer is overwritten.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlan {
    /// What comes in with no question: the book, the verses empty in the project and their
    /// alignments. One batch, cause `Import(Usfm { file_name, .. })`, written first.
    pub changes: Vec<PlannedChange>,
    /// The changes to review, each with no parents: a batch of its own, planned again after
    /// `changes` land. The fold makes one passage conflict of a batch's verse changes, so in
    /// one batch a verse to review would take the filled verses into its conflict.
    pub review: Vec<PlannedChange>,
    /// Verses empty in the project that the file's text filled, with no question.
    pub imported: Vec<RecordKey>,
    /// Records whose file value differs from the project's: each a change with no parents,
    /// so a conflict to review, and never the text shown until the person chooses.
    pub to_review: Vec<RecordKey>,
    /// Verses with text in the project and empty in the file ("left as it was: empty in the
    /// file"), and verses the file lacks or holds inside a span ("left as it was: not in the
    /// file"): never removed or restructured.
    pub left_as_it_was: Vec<RecordKey>,
    /// Verses whose alignment markup does not read apart: imported as text with the markup.
    pub markup_kept: Vec<RecordKey>,
    /// The project did not show the book, and the file adds it.
    pub book_added: bool,
}

/// A verse with words in it: Home's drafted test (`tc-app` `words_drafted`): words that are
/// not blank and not the stub `___`. Formatting or markers with no words are empty.
fn verse_has_words(record: Option<&Record>) -> bool {
    match record {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => {
            let words = tc_core::js::trim(words);
            !words.is_empty() && words != "___"
        }
        _ => false,
    }
}

fn same_heads(a: &[ChangeId], b: &[ChangeId]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort();
    b.sort();
    a == b
}

impl FoldState {
    /// Plans "Update a book from USFM" for `book` from `bytes`, the file `file_name` chosen
    /// from anywhere, so with no base (§13.6 U1-U5, H20). The file is read as the book's
    /// own USFM would be (U1), its aligned verses taken apart as an import takes them
    /// (`split_aligned`), and compared with the project by the outside-edit planner with an
    /// unknown base (U3). Then H20's rules:
    /// - a verse empty in the project and with words in the file is imported, its parents
    ///   the verse's heads, so no conflict;
    /// - a verse with words in the project and empty in the file is left as it was, and so is
    ///   a verse the file lacks or holds inside a span: never removed or restructured;
    /// - any other difference is a change with no parents, marked `unknown_base`, so a
    ///   conflict that never shows the file's text first; a record other than a verse
    ///   equal to any version in its History imports nothing;
    /// - an alignment is written only for a verse imported without a conflict, or when its
    ///   verse's text is unchanged (U4); a verse in conflict gets none.
    pub fn plan_update_book(
        &self,
        book: BookId,
        bytes: &[u8],
        file_name: &str,
    ) -> Result<UpdatePlan, String> {
        self.plan_update_book_from(book, bytes, file_name, &Base::Unknown)
    }

    /// [`FoldState::plan_update_book`] for a file whose base is known (§13.6 U2 a: the
    /// kept copy of a book file another program changed, compared with the placement it was
    /// found against). A record (a verse, the header, a chapter's front) the file changed
    /// since the base while the project kept the base's heads is imported with those heads as
    /// its parents, so no conflict; a record the file left as the base had it is nothing,
    /// whatever the project did since. Every other rule is H20's, so a record changed on both
    /// sides is still one to review.
    pub fn plan_update_book_from(
        &self,
        book: BookId,
        bytes: &[u8],
        file_name: &str,
        base: &Base,
    ) -> Result<UpdatePlan, String> {
        let mut plan = UpdatePlan::default();
        let mut parts = tc_usfm::codec::decompose_bytes(bytes)
            .map_err(|e| format!("the file does not read: {e}"))?;
        if let Some(named) = tc_usfm::seed::id_book(&parts.header)
            && named != book
        {
            return Err(format!(
                "the file is {}, not {}",
                named.english_name(),
                book.english_name()
            ));
        }
        let mut alignments = Vec::new();
        for (&c, chapter) in parts.chapters.iter_mut() {
            let Some(chapter_no) = ChapterNo::new(c) else {
                continue;
            };
            for (&v, part) in chapter.verses.iter_mut() {
                let VersePart::Text(verse) = part else {
                    continue;
                };
                let (Some(verse_no), Some(last)) = (VerseNo::new(v), VerseNo::new(verse.last))
                else {
                    continue;
                };
                let content = tc_usfm::codec::join_verse(&verse.words, &verse.layout);
                if !content.contains("\\zaln-s") {
                    continue;
                }
                match crate::alignment::split_aligned(&content, last) {
                    Some((plain, value)) => {
                        let (words, layout) = tc_usfm::codec::split_verse(&plain);
                        verse.words = words.to_owned();
                        verse.layout = layout.to_owned();
                        alignments.push(Record::Alignment {
                            book,
                            chapter: chapter_no,
                            verse: verse_no,
                            value,
                        });
                    }
                    None => plan
                        .markup_kept
                        .push(RecordKey::Verse(book, chapter_no, verse_no)),
                }
            }
        }
        let mut records = records_from_usfm(book, &parts);
        records.extend(alignments);
        let decoded = Decoded {
            records,
            ..Decoded::default()
        };
        let file = OutsideFile::Derived(DerivedFile::Usfm(book));
        // The planner's own cause is replaced below; the source only has to be well formed.
        let source = ExternalSource::LocalFile {
            path: RepoPath::parse(&file.path()).map_err(|e| e.to_string())?,
            seen_md5: Md5::of(bytes),
        };
        let unknown = matches!(base, Base::Unknown);
        let outside = self.plan_outside_edit(&file, &decoded, base, source, unknown);
        plan.book_added = outside.book_added.is_some();
        let cause = |unknown_base| {
            Cause::Import(crate::change::ImportSource::Usfm {
                file_name: file_name.into(),
                unknown_base,
            })
        };
        let mut verse_imported: BTreeSet<(ChapterNo, VerseNo)> = BTreeSet::new();
        let mut verse_review: BTreeSet<(ChapterNo, VerseNo)> = BTreeSet::new();
        let mut later = Vec::new();
        for change in outside.changes {
            let key = change.record.key();
            match &key {
                RecordKey::BookPresence(_) => plan.changes.push(PlannedChange {
                    cause: cause(false),
                    ..change
                }),
                RecordKey::Verse(_, c, v) => {
                    let project = self.projected(&key);
                    let heads = self.heads(&key);
                    let project_has = verse_has_words(project.as_ref());
                    let file_has = verse_has_words(Some(&change.record));
                    let shape = |r: Option<&Record>| match r {
                        Some(Record::Verse {
                            value: VerseValue::Text { last, .. },
                            ..
                        }) => Some(*last),
                        _ => None,
                    };
                    let (file_shape, project_shape) =
                        (shape(Some(&change.record)), shape(project.as_ref()));
                    // Filled with no question only when the project holds one version of
                    // the verse, an empty stub of the same shape: a verse in conflict keeps
                    // every side (§7.3), and a span, a covered or a removed verse is the
                    // translator's structure.
                    let fillable = !project_has && heads.len() == 1 && project_shape.is_some();
                    let wanted = compare_key(&change.record, true);
                    // Only the verse's current versions: a file that sets an older text
                    // back on purpose is a conflict (§13.6 U3), while a rerun or a
                    // replayed review finds its version already a head.
                    let is_a_head = heads.iter().any(|id| {
                        self.body(id)
                            .is_some_and(|b| compare_key(&b.record, true) == wanted)
                    });
                    if file_shape.is_none() || file_shape != project_shape {
                        // A verse the file lacks, or holds in another structure (a span, a
                        // covered verse): never a removal or a change of structure. A removal
                        // would win over an empty stub (§7.3), and as a conflict a change of
                        // structure takes the verses around it into one passage, so the
                        // file's other verses would not show. It is listed instead.
                        plan.left_as_it_was.push(key);
                    } else if project_has && !file_has {
                        plan.left_as_it_was.push(key);
                    } else if fillable && file_has {
                        verse_imported.insert((*c, *v));
                        plan.imported.push(key.clone());
                        plan.changes.push(PlannedChange {
                            parents: heads,
                            cause: cause(false),
                            record: change.record,
                        });
                    } else if file_has
                        && !change.parents.is_empty()
                        && same_heads(&change.parents, &heads)
                    {
                        // Changed in the file since its base, and the project still holds
                        // the base's heads: the file's edit, with no question.
                        verse_imported.insert((*c, *v));
                        plan.imported.push(key.clone());
                        plan.changes.push(PlannedChange {
                            parents: heads,
                            cause: cause(false),
                            record: change.record,
                        });
                    } else if is_a_head {
                    } else {
                        verse_review.insert((*c, *v));
                        plan.to_review.push(key);
                        plan.review.push(PlannedChange {
                            parents: Vec::new(),
                            cause: cause(true),
                            record: change.record,
                        });
                    }
                }
                _ => later.push(change),
            }
        }
        for change in later {
            let key = change.record.key();
            if let RecordKey::Alignment(_, c, v) = &key {
                if verse_review.contains(&(*c, *v)) {
                    continue;
                }
                if verse_imported.contains(&(*c, *v)) {
                    plan.changes.push(PlannedChange {
                        parents: self.heads(&key),
                        cause: cause(false),
                        record: change.record,
                    });
                    continue;
                }
            }
            let heads = self.heads(&key);
            if !change.parents.is_empty() && same_heads(&change.parents, &heads) {
                // As for a verse: changed in the file since its base (a header, a chapter's
                // front), and the project still holds the base's heads.
                plan.imported.push(key);
                plan.changes.push(PlannedChange {
                    parents: heads,
                    cause: cause(false),
                    record: change.record,
                });
                continue;
            }
            let wanted = compare_key(&change.record, true);
            let in_history = self.history(&key).into_iter().any(|id| {
                self.body(&id)
                    .is_some_and(|b| compare_key(&b.record, true) == wanted)
            });
            if in_history {
                continue;
            }
            plan.to_review.push(key);
            plan.review.push(PlannedChange {
                parents: Vec::new(),
                cause: cause(true),
                record: change.record,
            });
        }
        plan.changes.sort_by_key(|c| c.record.key());
        plan.review.sort_by_key(|c| c.record.key());
        Ok(plan)
    }
}

/// Which batch of "Update a book" to write ([`UpdatePlan`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePart {
    /// What comes in with no question.
    Fill,
    /// The changes to review, planned after the first part landed.
    Review,
}
