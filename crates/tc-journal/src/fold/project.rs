//! F11: the derived files, projected lazily per book and per document (§7.5, §7.7).
//!
//! Every byte here is a function of the fold alone: no clock, machine or installed
//! resource (I12). Lists are sorted, so two devices with the same changes write the same
//! bytes (I1).

use super::attach::{self, ShownVerse, runs};
use super::books::rewound_batches;
use super::merge::{ancestors_or_self, record_arc, record_of};
use super::meta::{ChangeIdx, Form, KeyIdx, VerseShape};
use super::state::{FoldState, Projected};
use super::{Conflict, ConflictKind, Counter};
use crate::alignment::AlignedRun;
use crate::canon;
use crate::decision::{self, TargetText};
use crate::ids::{ChangeId, GitOid, RepoPath, StoryNo, WallTime};
use crate::json::Json;
use crate::keys::{CheckKey, CheckTarget, PinGroup, RecordKey, ResourcesLevel, ToolId};
use crate::pins::{Answer, Collection};
use crate::record::{
    AlignmentGroup, AlignmentValue, CarriedValue, ChapterFront, CheckValue, CommentTarget,
    DecisionRecord, LayoutItem, Record, SourceWord, TargetWord, VerseValue,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use tc_core::BookId;
use tc_usfm::codec::{self, BookParts, ChapterParts, VersePart, VerseText};

/// A file the fold derives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivedFile {
    Usfm(BookId),
    Alignments(BookId),
    Decisions(ToolId, BookId),
    Comments(BookId),
    Resources,
    Settings,
    Versification,
    Conflicts,
    Story(StoryNo),
    /// `checking/<tool>/OBS.json`: the decisions on the stories of an OBS project (§3.2).
    StoryDecisions(ToolId),
    /// `tc/comments/OBS.json`: the comments on frames and on frame checks (§3.2).
    StoryComments,
}

impl DerivedFile {
    /// The book a file of one book belongs to.
    pub fn book(&self) -> Option<BookId> {
        match self {
            Self::Usfm(book)
            | Self::Alignments(book)
            | Self::Decisions(_, book)
            | Self::Comments(book) => Some(*book),
            _ => None,
        }
    }

    /// The file a repository path names, if it is one this format derives (§3.3): the
    /// inverse of [`Self::path`].
    pub fn from_path(path: &str) -> Option<DerivedFile> {
        let rest = path.strip_prefix("ingredients/")?;
        let book = |name: &str| -> Option<BookId> { name.strip_suffix(".json")?.parse().ok() };
        let file = match rest {
            "vrs.json" => Self::Versification,
            "checking/resources.json" => Self::Resources,
            "checking/settings.json" => Self::Settings,
            "tc/conflicts.json" => Self::Conflicts,
            "tc/comments/OBS.json" => Self::StoryComments,
            _ => {
                if let Some(name) = rest.strip_prefix("tc/comments/") {
                    Self::Comments(book(name)?)
                } else if let Some(name) = rest.strip_prefix("content/") {
                    let number: u8 = name.strip_suffix(".md")?.parse().ok()?;
                    Self::Story(StoryNo::new(number)?)
                } else if let Some(name) = rest.strip_prefix("checking/alignments/") {
                    Self::Alignments(book(name)?)
                } else if let Some(tail) = rest.strip_prefix("checking/") {
                    let (tool, name) = tail.split_once('/')?;
                    let tool = ToolId::parse(tool)?;
                    if name == "OBS.json" {
                        Self::StoryDecisions(tool)
                    } else {
                        Self::Decisions(tool, book(name)?)
                    }
                } else if !rest.contains('/') {
                    Self::Usfm(rest.strip_suffix(".usfm")?.parse().ok()?)
                } else {
                    return None;
                }
            }
        };
        // Only the spelling `path` writes: `content/7.md` is no story file.
        (file.path() == path).then_some(file)
    }

    /// The repository path of the file.
    pub fn path(&self) -> String {
        match self {
            Self::Usfm(book) => format!("ingredients/{}.usfm", book.code()),
            Self::Alignments(book) => {
                format!("ingredients/checking/alignments/{}.json", book.code())
            }
            Self::Decisions(tool, book) => {
                format!("ingredients/checking/{}/{}.json", tool.name(), book.code())
            }
            Self::Comments(book) => format!("ingredients/tc/comments/{}.json", book.code()),
            Self::Resources => "ingredients/checking/resources.json".into(),
            Self::Settings => "ingredients/checking/settings.json".into(),
            Self::Versification => "ingredients/vrs.json".into(),
            Self::Conflicts => "ingredients/tc/conflicts.json".into(),
            Self::Story(story) => format!("ingredients/content/{:02}.md", story.get()),
            Self::StoryDecisions(tool) => {
                format!("ingredients/checking/{}/OBS.json", tool.name())
            }
            Self::StoryComments => "ingredients/tc/comments/OBS.json".into(),
        }
    }

    /// The `role` metadata.json lists for the file (§3.3).
    pub fn role(&self) -> Option<&'static str> {
        match self {
            Self::Usfm(_) | Self::Versification | Self::Story(_) => None,
            Self::Alignments(_) => Some("x-alignment"),
            Self::Decisions(..) | Self::StoryDecisions(_) => Some("x-check-decisions"),
            Self::Comments(_) | Self::StoryComments => Some("x-tc-comments"),
            Self::Resources => Some("x-resource-links"),
            Self::Settings => Some("x-check-settings"),
            Self::Conflicts => Some("x-tc-conflicts"),
        }
    }

    /// The `scope` metadata.json lists for the file (§3.3), given the book's scope ranges.
    pub fn scope(&self, ranges: &[String]) -> Option<serde_json::Value> {
        let book = match self {
            Self::Usfm(book) => return Some(serde_json::json!({ book.code(): ranges })),
            Self::Alignments(book) | Self::Decisions(_, book) | Self::Comments(book) => *book,
            _ => return None,
        };
        Some(serde_json::json!({ book.code(): [] }))
    }
}

/// A record and the digest of how a file shows it (`crate::outside`).
pub type RecordDigest = (RecordKey, crate::outside::ShownDigest);

/// A derived file with its bytes and, for a book's USFM, the digest of each record it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    pub file: DerivedFile,
    pub bytes: Vec<u8>,
    pub digests: Vec<RecordDigest>,
}

/// Where a carried file's bytes go (§7.7, §11.4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CarriedPlacement {
    /// The path the bytes are written at: the record's path, or a `.conflict-` or `.case-`
    /// path.
    pub path: String,
    pub record: RepoPath,
    pub oid: GitOid,
    pub size: u64,
    pub md5: crate::ids::Md5,
    /// The head whose bytes these are.
    pub head: ChangeId,
}

/// RFC 3339 with milliseconds and `Z` for a wall time.
pub(crate) fn rfc3339(at: WallTime) -> String {
    let ms = at.0.rem_euclid(1000);
    let secs = at.0.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn value(j: &Json) -> Value {
    Value::from(j)
}

fn compact(v: &Value) -> String {
    // A `Value` serializes infallibly: its keys are strings and it holds no floats.
    serde_json::to_string(v).unwrap_or_default()
}

/// `work` applied to every item on the worker pool, the results in item order. Every
/// parallel step of the fold runs on the one pool, however the steps nest: threads spawned
/// for each step, four at each level, ran sixteen at once while a book was projected, and
/// each kept an allocator heap at its high-water mark afterwards.
pub(crate) fn on_threads<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use rayon::prelude::*;
    items.par_iter().map(&work).collect()
}

/// Records decoded together on one thread.
type DecodedChunk = Vec<(ChangeIdx, Arc<Record>)>;

/// Drops `items` on the worker pool.
fn drop_on_threads<T: Send>(items: Vec<T>) {
    use rayon::prelude::*;
    items.into_par_iter().for_each(drop);
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut out = serde_json::to_string_pretty(v)
        .unwrap_or_default()
        .into_bytes();
    out.push(b'\n');
    out
}

/// The sidecar byte form (§7.7): the document opens on the first line, each record is one
/// line ending in `,` except the last, and the closing brackets are on the last line.
fn lines(head: &str, records: &[String], tail: &str) -> Vec<u8> {
    let mut out = String::with_capacity(
        head.len() + tail.len() + records.iter().map(|r| r.len() + 2).sum::<usize>() + 2,
    );
    out.push_str(head);
    out.push('\n');
    for (i, record) in records.iter().enumerate() {
        out.push_str(record);
        if i + 1 < records.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(tail);
    out.push('\n');
    out.into_bytes()
}

/// A projected decision of one (tool, collection), as the Check screens read it (§7.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedDecision {
    pub key: CheckKey,
    pub record: DecisionRecord,
    /// The state its decision file carries: [`decision::assess`] against the resolution of
    /// the (tool, collection) and the target's text.
    pub assessment: decision::Assessment,
    /// Listed under Detached: made against another resource than the resolution, or on a
    /// target the text no longer has.
    pub detached: bool,
}

/// The texts the checks of one collection are read against, by the no-text rule of §7.7:
/// what a Check screen assesses a decision with ([`FoldState::check_texts`]), the texts the
/// decision files are computed from.
pub struct CheckTexts<'a> {
    state: &'a FoldState,
    book: Option<(BookId, BookText)>,
}

impl CheckTexts<'_> {
    /// The text `target` names, or `None` when it names no text (§7.7): the target gives
    /// no evidence either way. A target of another collection has none here.
    pub fn text(&self, target: &CheckTarget) -> Option<Arc<TargetText>> {
        match (target, &self.book) {
            (CheckTarget::Frame { story, frame }, None) => self
                .state
                .frame_text(*story, *frame)
                .map(|t| Arc::new(TargetText::new(t))),
            (CheckTarget::Verse { book, .. }, Some((ours, text))) if book == ours => {
                verse_content(text, target).map(|c| text.facts(c))
            }
            _ => None,
        }
    }
}

/// The projected verse texts of a book, read once for a document.
pub(crate) struct BookText {
    /// chapter → verse → what the verse shows.
    pub chapters: BTreeMap<u16, ChapterText>,
    /// What alignments and decisions read of a text, by the text, so the files of a book,
    /// rendered on several threads, derive each verse's plain text once.
    facts: std::sync::Mutex<HashMap<String, Arc<TargetText>>>,
}

impl BookText {
    pub(crate) fn facts(&self, content: String) -> Arc<TargetText> {
        let cached = |map: &HashMap<String, Arc<TargetText>>| map.get(&content).cloned();
        if let Some(facts) = self.facts.lock().ok().and_then(|map| cached(&map)) {
            return facts;
        }
        // Derived outside the lock: the other threads keep reading meanwhile.
        let facts = Arc::new(TargetText::new(content.clone()));
        match self.facts.lock() {
            Ok(mut map) => map.entry(content).or_insert(facts).clone(),
            Err(_) => facts,
        }
    }
}

#[derive(Default)]
pub(crate) struct ChapterText {
    pub front: Option<String>,
    pub front_key: bool,
    /// verse → its part as projection writes it.
    pub verses: BTreeMap<u16, VersePart>,
    /// run start → run end.
    pub ends: BTreeMap<u16, u16>,
    /// verse → the run start it is in.
    pub run_of: BTreeMap<u16, u16>,
}

impl ChapterText {
    /// `words + layout` of the run that starts at `start`.
    pub fn content(&self, start: u16) -> Option<String> {
        match self.verses.get(&start) {
            Some(VersePart::Text(t)) => Some(format!("{}{}", t.words, t.layout)),
            _ => None,
        }
    }

    /// The content of the run that starts at `start` when tC4 reads it as drafted
    /// ([`codec::is_drafted`]); `None` for an undrafted run, which has no text.
    pub fn drafted_content(&self, start: u16) -> Option<String> {
        match self.verses.get(&start) {
            Some(VersePart::Text(t)) => {
                let content = format!("{}{}", t.words, t.layout);
                codec::is_drafted(t.marker.term(), &content).then_some(content)
            }
            _ => None,
        }
    }
}

fn verse_text(record: &Record) -> Option<VerseText> {
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
        } => Some(VerseText {
            marker: marker.clone(),
            words: words.clone(),
            layout: layout.clone(),
            last: last.get(),
        }),
        _ => None,
    }
}

impl FoldState {
    fn projected_record(&self, k: KeyIdx) -> Option<Record> {
        self.projected_arc(k).map(Arc::unwrap_or_clone)
    }

    fn projected_arc(&self, k: KeyIdx) -> Option<Arc<Record>> {
        self.count(Counter::RecordsProjected, 1);
        match &self.entry(k).projected {
            Projected::Nothing => None,
            Projected::Change(c) => record_arc(self, *c),
            Projected::Merged(record) => Some(record.clone()),
        }
    }

    /// The newest text value among a verse's heads and their ancestors (§7.5): what an
    /// incoherent covered verse shows.
    fn newest_own_text(&self, k: KeyIdx) -> Option<VerseText> {
        let mut seen: BTreeSet<ChangeIdx> = BTreeSet::new();
        for h in self.entry(k).heads.iter() {
            seen.extend(ancestors_or_self(self, *h));
        }
        let best = seen
            .into_iter()
            .filter(|c| matches!(self.meta(*c).verse(), Some(VerseShape::Text { .. })))
            .max_by_key(|c| (self.meta(*c).at, self.meta(*c).id))?;
        self.count(Counter::RecordsProjected, 1);
        record_of(self, best).and_then(|r| verse_text(&r))
    }

    /// The text of one chapter as projection writes it.
    fn chapter_text(
        &self,
        book: BookId,
        chapter: u16,
        rewound: &[crate::ids::BatchId],
    ) -> ChapterText {
        let mut out = ChapterText::default();
        let Some(entry) = self.chapters.get(&(book, chapter)) else {
            return out;
        };
        if let Some(f) = entry.front {
            out.front_key = true;
            if let Some(Record::ChapterFront {
                value: ChapterFront::Present(bytes),
                ..
            }) = self.projected_record(f)
            {
                out.front = Some(bytes.0);
            }
        }
        let mut shapes: BTreeMap<u16, VerseShape> = BTreeMap::new();
        let mut shown: BTreeMap<u16, (KeyIdx, ShownVerse)> = BTreeMap::new();
        for (&verse, &k) in &entry.verses {
            if let Some(s) = attach::shown_verse(self, k, rewound) {
                shapes.insert(verse, s.shape());
                shown.insert(verse, (k, s));
            }
        }
        let found = runs(&shapes, out.front.is_some());
        for (verse, (k, s)) in shown {
            match s.shape() {
                VerseShape::Text { .. } => {
                    let record = match &s {
                        ShownVerse::Change(_, c) => {
                            self.count(Counter::RecordsProjected, 1);
                            record_arc(self, *c)
                        }
                        ShownVerse::Merged(_, record) => Some(record.clone()),
                    };
                    if let Some(text) = record.as_deref().and_then(verse_text) {
                        out.verses.insert(verse, VersePart::Text(text));
                    }
                }
                VerseShape::Covered { by } if found.consumed.contains(&verse) => {
                    out.verses.insert(verse, VersePart::Covered { by });
                }
                VerseShape::Covered { .. } => {
                    // Incoherent: the verse shows its own newest text, labelled as itself.
                    if let Some(mut text) = self.newest_own_text(k) {
                        text.last = verse;
                        out.verses.insert(verse, VersePart::Text(text));
                    }
                }
                VerseShape::Absent { .. } => {}
            }
        }
        // Runs over the parts as written.
        let written: BTreeMap<u16, VerseShape> = out
            .verses
            .iter()
            .map(|(v, p)| {
                (
                    *v,
                    match p {
                        VersePart::Text(t) => VerseShape::Text {
                            last: t.last,
                            from: None,
                        },
                        VersePart::Covered { by } => VerseShape::Covered { by: *by },
                    },
                )
            })
            .collect();
        let found = runs(&written, true);
        out.ends = found.ends.clone();
        for (&start, &end) in &found.ends {
            for v in start..=end {
                out.run_of.insert(v, start);
            }
        }
        out
    }

    /// The projected text of every chapter of a book.
    pub(crate) fn book_text(&self, book: BookId) -> BookText {
        let rewound = rewound_batches(self, book);
        let chapters = self
            .books
            .get(&book)
            .map(|b| b.chapters.iter().copied().collect::<Vec<u16>>())
            .unwrap_or_default();
        BookText {
            chapters: chapters
                .into_iter()
                .map(|c| (c, self.chapter_text(book, c, &rewound)))
                .collect(),
            facts: Default::default(),
        }
    }

    fn layout(&self, book: BookId) -> Option<Vec<codec::LayoutItem>> {
        let k = self.books.get(&book)?.layout?;
        match self.projected_record(k)? {
            Record::BookLayout {
                value: Some(items), ..
            } => Some(
                items
                    .iter()
                    .map(|item| match item {
                        LayoutItem::Chapter(c) => codec::LayoutItem::Chapter(c.get()),
                        LayoutItem::Verse(c, v) => codec::LayoutItem::Verse(c.get(), v.get()),
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    fn parts(&self, book: BookId, only: Option<u16>) -> BookParts {
        let rewound = rewound_batches(self, book);
        let header = match (only, self.books.get(&book).and_then(|b| b.header)) {
            (None, Some(k)) => match self.projected_record(k) {
                Some(Record::BookHeader { value, .. }) => value.0,
                _ => String::new(),
            },
            _ => String::new(),
        };
        let chapters: Vec<u16> = match only {
            Some(c) => vec![c],
            None => self
                .books
                .get(&book)
                .map(|b| b.chapters.iter().copied().collect())
                .unwrap_or_default(),
        };
        let mut parts = BookParts {
            header,
            chapters: BTreeMap::new(),
            layout: self.layout(book),
        };
        for c in chapters {
            let text = self.chapter_text(book, c, &rewound);
            parts.chapters.insert(
                c,
                ChapterParts {
                    front: text.front,
                    verses: text.verses,
                },
            );
        }
        parts
    }

    /// A book's USFM (§7.5), when the book projects.
    pub fn book_usfm(&self, book: BookId) -> Option<String> {
        if !self.book(book).projects() {
            return None;
        }
        Some(codec::recompose(&self.parts(book, None)).usfm)
    }

    /// One chapter's USFM, from its marker to the byte before the next chapter's: what the
    /// editor shows after a save.
    pub fn chapter_usfm(&self, book: BookId, chapter: u16) -> Option<String> {
        if !self.book(book).projects() {
            return None;
        }
        Some(codec::recompose(&self.parts(book, Some(chapter))).usfm)
    }

    /// Every derived file the projection holds now, in path order.
    pub fn derived_files(&self) -> Vec<DerivedFile> {
        let mut out: BTreeSet<DerivedFile> = BTreeSet::new();
        for book in self.books.keys() {
            self.book_files_into(*book, &mut out);
        }
        self.project_files_into(&mut out);
        let mut files: Vec<DerivedFile> = out.into_iter().collect();
        files.sort_by_key(DerivedFile::path);
        files
    }

    /// The derived files of no book that the projection holds.
    fn project_files_into(&self, out: &mut BTreeSet<DerivedFile>) {
        if !self.pin_keys.is_empty() {
            out.insert(DerivedFile::Resources);
        }
        if self.kind == super::ProjectKind::Bible {
            out.insert(DerivedFile::Settings);
        }
        if let Some(k) = self.key_idx(&RecordKey::Versification)
            && !matches!(self.entry(k).projected, Projected::Nothing)
        {
            out.insert(DerivedFile::Versification);
        }
        if !self.conflicts.is_empty() {
            out.insert(DerivedFile::Conflicts);
        }
        for k in self.story_attached() {
            let e = self.entry(k);
            match &e.key {
                RecordKey::Check(check) if self.holds_decision(k) => {
                    out.insert(DerivedFile::StoryDecisions(check.tool));
                }
                RecordKey::Comment(_) if !matches!(e.projected, Projected::Nothing) => {
                    out.insert(DerivedFile::StoryComments);
                }
                _ => {}
            }
        }
        for k in self.story_keys.iter() {
            if let RecordKey::StoryBase(story) = &self.entry(*k).key
                && !matches!(self.entry(*k).projected, Projected::Nothing)
            {
                out.insert(DerivedFile::Story(*story));
            }
        }
    }

    /// Whether the projection holds `file` now. It costs what that file's book or the
    /// project's own records hold, never the project: a book's files are found from its own
    /// records (replaces `derived_files().contains(..)`).
    pub fn projects_file(&self, file: &DerivedFile) -> bool {
        let mut set = BTreeSet::new();
        match file.book() {
            Some(book) => self.book_files_into(book, &mut set),
            None => self.project_files_into(&mut set),
        }
        set.contains(file)
    }

    /// The projected parts of a book, as `book_usfm` composes them; `None` when the book
    /// does not project. A caller that compares a file with the projection decomposes the
    /// file into the same parts.
    pub fn book_parts(&self, book: BookId) -> Option<BookParts> {
        self.book(book).projects().then(|| self.parts(book, None))
    }

    /// The derived files of one book: [`Self::derived_files`] of that book, found from its
    /// own records only, so listing them costs what the book holds.
    pub fn book_files(&self, book: BookId) -> Vec<DerivedFile> {
        let mut out: BTreeSet<DerivedFile> = BTreeSet::new();
        self.book_files_into(book, &mut out);
        let mut files: Vec<DerivedFile> = out.into_iter().collect();
        files.sort_by_key(DerivedFile::path);
        files
    }

    fn book_files_into(&self, book: BookId, out: &mut BTreeSet<DerivedFile>) {
        let Some(entry) = self.books.get(&book) else {
            return;
        };
        if !entry.status.projects() {
            return;
        }
        out.insert(DerivedFile::Usfm(book));
        self.count(Counter::RecordsListed, entry.keys.len() as u64);
        for k in &entry.keys {
            let e = self.entry(*k);
            if matches!(e.projected, Projected::Nothing) {
                continue;
            }
            match &e.key {
                RecordKey::Alignment(..) => {
                    out.insert(DerivedFile::Alignments(book));
                }
                RecordKey::Check(check) => {
                    if self.holds_decision(*k) {
                        out.insert(DerivedFile::Decisions(check.tool, book));
                    }
                }
                RecordKey::Comment(_) => {
                    out.insert(DerivedFile::Comments(book));
                }
                _ => {}
            }
        }
    }

    /// Whether check record `k` projects a decision (not a `MovedTo`, not nothing).
    fn holds_decision(&self, k: KeyIdx) -> bool {
        match &self.entry(k).projected {
            Projected::Change(c) => matches!(self.meta(*c).form, Form::Decision(_)),
            Projected::Merged(r) => matches!(
                &**r,
                Record::Check {
                    value: CheckValue::Decision(_),
                    ..
                }
            ),
            Projected::Nothing => false,
        }
    }

    /// The bytes of a derived file, or `None` when the projection does not hold it.
    pub fn render(&self, file: &DerivedFile) -> Option<Vec<u8>> {
        self.render_with(file, &std::sync::OnceLock::new())
    }

    /// Every derived file of one book the projection holds, with its bytes: what
    /// materializing a book writes (§5.10). The book's records are read and decoded once,
    /// on up to four threads, and its files share the book's text and render on up to four
    /// threads.
    pub fn render_book(&self, book: BookId) -> Vec<(DerivedFile, Vec<u8>)> {
        self.render_book_inner(book, false)
            .into_iter()
            .map(|r| (r.file, r.bytes))
            .collect()
    }

    /// [`Self::render_book`] with the digest of each record of the book's USFM (§11.1: the
    /// heads file keeps them as the base of an outside edit).
    pub fn render_book_with_digests(&self, book: BookId) -> Vec<RenderedFile> {
        self.render_book_inner(book, true)
    }

    /// [`Self::render`] with the digests of a USFM file's records.
    pub fn render_with_digests(&self, file: &DerivedFile) -> Option<RenderedFile> {
        match file {
            DerivedFile::Usfm(book) => {
                let (bytes, digests) = self.usfm_with_digests(*book)?;
                Some(RenderedFile {
                    file: file.clone(),
                    bytes,
                    digests,
                })
            }
            other => Some(RenderedFile {
                file: other.clone(),
                bytes: self.render(other)?,
                digests: Vec::new(),
            }),
        }
    }

    fn usfm_with_digests(&self, book: BookId) -> Option<(Vec<u8>, Vec<RecordDigest>)> {
        if !self.book(book).projects() {
            return None;
        }
        let parts = self.parts(book, None);
        let digests = crate::outside::usfm_digests(book, &parts);
        Some((codec::recompose(&parts).usfm.into_bytes(), digests))
    }

    fn render_book_inner(&self, book: BookId, digests: bool) -> Vec<RenderedFile> {
        let files = self.book_files(book);
        if files.is_empty() {
            return Vec::new();
        }
        let (view, decoded) = self.with_records_of(book);
        let text = std::sync::OnceLock::new();
        if view.book(book).projects() {
            let _ = text.set(view.book_text(book));
        }
        let mut out: Vec<RenderedFile> = on_threads(&files, |f| match f {
            DerivedFile::Usfm(b) if digests => {
                view.usfm_with_digests(*b)
                    .map(|(bytes, digests)| RenderedFile {
                        file: f.clone(),
                        bytes,
                        digests,
                    })
            }
            _ => view.render_with(f, &text).map(|bytes| RenderedFile {
                file: f.clone(),
                bytes,
                digests: Vec::new(),
            }),
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by(|a, b| a.file.cmp(&b.file));
        // The decoded records are hundreds of thousands of small allocations: freed on one
        // thread they were a tenth of the projection's time.
        drop(view);
        drop_on_threads(decoded);
        out
    }

    /// A snapshot of this state holding the projected records of `book` that several of its
    /// files read (verses, chapter fronts, the header and layout) decoded, so those files
    /// read each value once, and the decoded records in chunks. Alignments, checks and
    /// comments are each read by one file, which decodes them in its own chunks and frees
    /// them as it goes.
    fn with_records_of(&self, book: BookId) -> (FoldState, Vec<DecodedChunk>) {
        let changes: Vec<ChangeIdx> = self
            .books
            .get(&book)
            .map(|b| {
                b.keys
                    .iter()
                    .filter(|k| {
                        !matches!(
                            self.entry(**k).key,
                            RecordKey::Alignment(..) | RecordKey::Check(_) | RecordKey::Comment(_)
                        )
                    })
                    .filter_map(|k| match &self.entry(*k).projected {
                        Projected::Change(c) => Some(*c),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let chunks: Vec<&[ChangeIdx]> = changes.chunks(256).collect();
        let decoded = on_threads(&chunks, |chunk| {
            chunk
                .iter()
                .filter_map(|c| Some((*c, record_arc(self, *c)?)))
                .collect::<Vec<_>>()
        });
        let mut view = self.clone();
        view.records = Some(Arc::new(decoded.iter().flatten().cloned().collect()));
        (view, decoded)
    }

    fn render_with(
        &self,
        file: &DerivedFile,
        text: &std::sync::OnceLock<BookText>,
    ) -> Option<Vec<u8>> {
        let text_of = |book: BookId| text.get_or_init(|| self.book_text(book));
        match file {
            DerivedFile::Usfm(book) => self.book_usfm(*book).map(String::into_bytes),
            DerivedFile::Alignments(book) => self.alignments_json(*book, text_of),
            DerivedFile::Decisions(tool, book) => self.decisions_json(*tool, *book, text_of),
            DerivedFile::Comments(book) => self.comments_json(*book),
            DerivedFile::Resources => self.resources_json(),
            DerivedFile::Settings => Some(self.settings_json()),
            DerivedFile::Versification => {
                let k = self.key_idx(&RecordKey::Versification)?;
                match self.projected_record(k)? {
                    Record::Versification { value } => Some(value.bytes.into_bytes()),
                    _ => None,
                }
            }
            DerivedFile::Conflicts => self.conflicts_json(),
            DerivedFile::Story(story) => self.story_markdown(*story),
            DerivedFile::StoryDecisions(tool) => self.story_decisions_json(*tool),
            DerivedFile::StoryComments => self.story_comments_json(),
        }
    }

    // ---------------------------------------------------------------------------------
    // Alignments
    // ---------------------------------------------------------------------------------

    fn alignments_json<'t>(
        &self,
        book: BookId,
        text_of: impl FnOnce(BookId) -> &'t BookText,
    ) -> Option<Vec<u8>> {
        if !self.book(book).projects() {
            return None;
        }
        let text = text_of(book);
        let ol_sha = self.pins.original_sha(book);
        // (chapter, label) → record; placed records first.
        let mut placed: BTreeMap<(u16, u16, u16), String> = BTreeMap::new();
        let mut fallback: Vec<((u16, u16), String)> = Vec::new();
        let mut taken: BTreeSet<(u16, u16)> = BTreeSet::new();
        let keys: Vec<KeyIdx> = self
            .books
            .get(&book)
            .map(|b| {
                b.keys
                    .iter()
                    .copied()
                    .filter(|k| matches!(self.entry(*k).key, RecordKey::Alignment(..)))
                    .collect()
            })
            .unwrap_or_default();
        let mut first_at_run: BTreeSet<(u16, u16)> = BTreeSet::new();
        let mut sorted = keys;
        sorted.sort_by_key(|k| self.entry(*k).key.clone());
        let key_chunks: Vec<&[KeyIdx]> = sorted.chunks(128).collect();
        let records: Vec<((u16, u16), Arc<Record>)> = on_threads(&key_chunks, |chunk| {
            chunk
                .iter()
                .filter_map(|k| {
                    let RecordKey::Alignment(_, c, v) = &self.entry(*k).key else {
                        return None;
                    };
                    Some(((c.get(), v.get()), self.projected_arc(*k)?))
                })
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect();
        // Where each record goes is decided in key order; the records, kilobytes each, are
        // then written on several threads.
        let mut plan: Vec<(&AlignmentValue, AlignmentSlot)> = Vec::with_capacity(records.len());
        for ((c, v), record) in &records {
            let (c, v) = (*c, *v);
            let Record::Alignment { value, .. } = &**record else {
                continue;
            };
            let (place, _) = attach::place(self, book, (c, v));
            let by_rule_one = self
                .anchors
                .get(&(book, c, v))
                .and_then(|set| set.iter().next().copied());
            match (place, by_rule_one) {
                (attach::Place::Run(rc, rv), Some(_)) if first_at_run.insert((rc, rv)) => {
                    let chapter = text.chapters.get(&rc);
                    let end = chapter.and_then(|t| t.ends.get(&rv)).copied().unwrap_or(rv);
                    let content = chapter.and_then(|t| t.content(rv)).unwrap_or_default();
                    // The alignment covers as many verses as it did where it was made: a
                    // text that moved keeps its alignment valid at its new number.
                    let label_end = value.moved_span_end(v, rv).unwrap_or(rv);
                    let valid = value.is_valid(&AlignedRun {
                        anchor: v,
                        run_start: rv,
                        run_end: end,
                        text_md5: text.facts(content).md5(),
                        source_sha: ol_sha,
                    });
                    taken.insert((rc, rv));
                    plan.push((
                        value,
                        AlignmentSlot::Placed {
                            at: (rc, rv, label_end),
                            invalid: !valid,
                            anchor: ((rc, rv) != (c, v)).then_some((c, v)),
                            span_end: (value.span_end.get() != label_end)
                                .then_some(value.span_end.get()),
                        },
                    ));
                }
                _ => plan.push((
                    value,
                    AlignmentSlot::Fallback {
                        at: (c, v),
                        span_end: (value.span_end.get() != v).then_some(value.span_end.get()),
                    },
                )),
            }
        }
        let chunks: Vec<&[(&AlignmentValue, AlignmentSlot)]> = plan.chunks(128).collect();
        let written = on_threads(&chunks, |chunk| {
            chunk
                .iter()
                .map(|(value, slot)| {
                    let (invalid, anchor, span_end) = match slot {
                        AlignmentSlot::Placed {
                            invalid,
                            anchor,
                            span_end,
                            ..
                        } => (*invalid, *anchor, *span_end),
                        AlignmentSlot::Fallback { span_end, .. } => (true, None, *span_end),
                    };
                    let mut record = alignment_record(value, invalid);
                    if let Some((c, v)) = anchor {
                        add_anchor(&mut record, c, v);
                    }
                    if let Some(end) = span_end {
                        add_span_end(&mut record, end);
                    }
                    (*slot, record.finish())
                })
                .collect::<Vec<_>>()
        });
        // The decoded records, several times the size of what they write, go before the
        // file is put together.
        drop(plan);
        drop(records);
        for (slot, record) in written.into_iter().flatten() {
            match slot {
                AlignmentSlot::Placed { at, .. } => {
                    placed.insert(at, record);
                }
                AlignmentSlot::Fallback { at, .. } => fallback.push((at, record)),
            }
        }
        let mut chapters: BTreeMap<u16, Vec<(u16, String)>> = BTreeMap::new();
        for ((c, v, end), record) in placed {
            let label = codec::label(v, end.max(v));
            chapters
                .entry(c)
                .or_default()
                .push((v, format!("{}:{record}", compact(&Value::from(label)))));
        }
        for ((c, v), record) in fallback {
            // A record whose own label another record holds stays in the journal and in
            // the review's Detached list; the sidecar cannot hold two records at one label.
            if !taken.insert((c, v)) {
                continue;
            }
            chapters.entry(c).or_default().push((
                v,
                format!("{}:{record}", compact(&Value::from(v.to_string()))),
            ));
        }
        let head = format!(
            "{{\"schemaVersion\":1,\"book\":{},\"chapters\":{{",
            compact(&Value::from(book.code()))
        );
        let mut records: Vec<String> = Vec::new();
        for (c, mut verses) in chapters {
            verses.sort_by_key(|(v, _)| *v);
            let n = verses.len();
            for (j, (_, line)) in verses.into_iter().enumerate() {
                let mut line = line;
                if j == 0 {
                    line = format!("{}:{{{line}", compact(&Value::from(c.to_string())));
                }
                if j + 1 == n {
                    line.push('}');
                }
                records.push(line);
            }
        }
        Some(lines(&head, &records, "}}"))
    }

    // ---------------------------------------------------------------------------------
    // Decisions
    // ---------------------------------------------------------------------------------

    fn decisions_json<'t>(
        &self,
        tool: ToolId,
        book: BookId,
        text_of: impl FnOnce(BookId) -> &'t BookText,
    ) -> Option<Vec<u8>> {
        if !self.book(book).projects() {
            return None;
        }
        let text = text_of(book);
        let answer = self.pins.answer(tool, Collection::Book(book));
        let resolved = answer.and_then(Answer::resource);
        let keys: Vec<KeyIdx> = self
            .books
            .get(&book)
            .map(|b| b.keys.clone())
            .unwrap_or_default();
        // The tool's decisions, by target verse, so the decisions of one verse share its
        // text and derivations (tN and TWL hold several per verse).
        let mut decisions: Vec<Targeted<'_>> = Vec::new();
        for k in keys {
            let RecordKey::Check(check) = &self.entry(k).key else {
                continue;
            };
            if check.tool != tool {
                continue;
            }
            let target = match &check.target {
                CheckTarget::Verse { chapter, verse, .. } => (chapter.as_str(), verse.as_str()),
                CheckTarget::Frame { .. } => ("", ""),
            };
            decisions.push((target, k));
        }
        decisions.sort_by_key(|(target, k)| (*target, *k));
        // Rendered in chunks on several threads: a book holds thousands of decisions.
        let chunks: Vec<&[Targeted<'_>]> = decisions.chunks(512).collect();
        let rendered = on_threads(&chunks, |chunk| {
            let mut out: Vec<(String, String)> = Vec::with_capacity(chunk.len());
            let mut verse: Option<(Target<'_>, Option<Arc<TargetText>>)> = None;
            for (target, k) in chunk.iter() {
                let Some(record) = self.projected_arc(*k) else {
                    continue;
                };
                let Record::Check {
                    key: check,
                    value: CheckValue::Decision(decision),
                } = &*record
                else {
                    continue;
                };
                let newest = self
                    .entry(*k)
                    .heads
                    .iter()
                    .map(|h| self.meta(*h).at)
                    .max()
                    .unwrap_or(WallTime(0));
                if verse.as_ref().is_none_or(|(t, _)| t != target) {
                    let facts = target_content(text, check).map(|c| text.facts(c));
                    verse = Some((*target, facts));
                }
                let Some((_, facts)) = &verse else {
                    continue;
                };
                let mut context = String::new();
                write_map(&decision.context, &mut context);
                let record = decision_record(
                    decision,
                    &context,
                    check,
                    resolved,
                    facts.as_deref(),
                    newest,
                );
                out.push((context, record.finish()));
            }
            out
        });
        let mut records: Vec<(String, String)> = rendered.into_iter().flatten().collect();
        records.sort();
        let mut head = Map::new();
        head.insert("schemaVersion".into(), Value::from(1));
        head.insert("tool".into(), Value::from(tool.name()));
        head.insert("book".into(), Value::from(book.code()));
        if let Some(resource) = answer.map(resolution_record) {
            head.insert("resource".into(), resource);
        }
        let mut head = compact(&Value::Object(head));
        head.pop();
        head.push_str(",\"decisions\":[");
        let records: Vec<String> = records.into_iter().map(|(_, r)| r).collect();
        Some(lines(&head, &records, "]}"))
    }

    /// The texts the checks of `collection` are read against (§7.7), read once: a Check
    /// screen assesses a decision it shows under another key than its own with them, as the
    /// decision files are computed.
    pub fn check_texts(&self, collection: Collection) -> CheckTexts<'_> {
        CheckTexts {
            state: self,
            book: match collection {
                Collection::Book(book) => Some((book, self.book_text(book))),
                Collection::Stories => None,
            },
        }
    }

    /// Every decision of `tool` on `collection` the projection holds, in key order, with the
    /// state its decision file carries (§7.7) and whether it is listed under Detached: what
    /// the Check screens open a session with ([`ProjectedDecision`]). Each target's text is
    /// read by the no-text rule, as the decision files read it.
    pub fn decisions(&self, tool: ToolId, collection: Collection) -> Vec<ProjectedDecision> {
        let (keys, text): (Vec<KeyIdx>, Option<BookText>) = match collection {
            Collection::Book(book) => {
                if !self.book(book).projects() {
                    return Vec::new();
                }
                let keys = self
                    .books
                    .get(&book)
                    .map(|b| b.keys.to_vec())
                    .unwrap_or_default();
                (keys, Some(self.book_text(book)))
            }
            Collection::Stories => (self.story_attached().collect(), None),
        };
        let resolved = self
            .pins
            .answer(tool, collection)
            .and_then(Answer::resource);
        let mut out = Vec::new();
        for k in keys {
            let Some(record) = self.projected_arc(k) else {
                continue;
            };
            let Record::Check {
                key,
                value: CheckValue::Decision(decision),
            } = &*record
            else {
                continue;
            };
            if key.tool != tool || key.target.collection() != collection {
                continue;
            }
            let target = match (&key.target, &text) {
                (CheckTarget::Frame { story, frame }, _) => self
                    .frame_text(*story, *frame)
                    .map(|t| Arc::new(TargetText::new(t))),
                (CheckTarget::Verse { .. }, Some(text)) => {
                    target_content(text, key).map(|c| text.facts(c))
                }
                (CheckTarget::Verse { .. }, None) => None,
            };
            out.push(ProjectedDecision {
                key: key.clone(),
                assessment: decision::assess(decision, resolved, target.as_deref()),
                detached: self.detached.contains(&RecordKey::Check(key.clone())),
                record: decision.clone(),
            });
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

    /// The text of frame `frame` of `story` as the project holds it: the frame's record,
    /// else the template's part; `None` when the story has no template. tC4 reads a story
    /// decision's text from the open story's frames (`frameTextIndex`, `src/state.jsx` at
    /// d310bf3), which holds a text for every frame.
    pub(crate) fn frame_text(&self, story: StoryNo, frame: crate::ids::FrameNo) -> Option<String> {
        if let Some(Record::Frame { value, .. }) = self
            .key_idx(&RecordKey::Frame(story, frame))
            .and_then(|k| self.projected_record(k))
        {
            return Some(value);
        }
        match self.projected_record(self.key_idx(&RecordKey::StoryBase(story))?)? {
            Record::StoryBase { value, .. } => template_frame(&value, frame.0),
            _ => None,
        }
    }

    /// `checking/<tool>/OBS.json` (§3.2, §7.7): the decisions on frames, resolved over the
    /// `obs-` slots of the ladder, each read against its frame's text.
    fn story_decisions_json(&self, tool: ToolId) -> Option<Vec<u8>> {
        let answer = self.pins.answer(tool, Collection::Stories);
        let resolved = answer.and_then(Answer::resource);
        let mut records: Vec<(String, String)> = Vec::new();
        for k in self.story_attached() {
            let Some(record) = self.projected_arc(k) else {
                continue;
            };
            let Record::Check {
                key: check,
                value: CheckValue::Decision(decision),
            } = &*record
            else {
                continue;
            };
            let CheckTarget::Frame { story, frame } = &check.target else {
                continue;
            };
            if check.tool != tool {
                continue;
            }
            let newest = self
                .entry(k)
                .heads
                .iter()
                .map(|h| self.meta(*h).at)
                .max()
                .unwrap_or(WallTime(0));
            let text = self.frame_text(*story, *frame).map(TargetText::new);
            let mut context = String::new();
            write_map(&decision.context, &mut context);
            let record =
                decision_record(decision, &context, check, resolved, text.as_ref(), newest);
            records.push((context, record.finish()));
        }
        if records.is_empty() {
            return None;
        }
        records.sort();
        let mut head = Map::new();
        head.insert("schemaVersion".into(), Value::from(1));
        head.insert("tool".into(), Value::from(tool.name()));
        head.insert("book".into(), Value::from("OBS"));
        if let Some(resource) = answer.map(resolution_record) {
            head.insert("resource".into(), resource);
        }
        let mut head = compact(&Value::Object(head));
        head.pop();
        head.push_str(",\"decisions\":[");
        let records: Vec<String> = records.into_iter().map(|(_, r)| r).collect();
        Some(lines(&head, &records, "]}"))
    }

    /// `tc/comments/OBS.json` (§3.2, §7.7): the comments on frames and on frame checks,
    /// sorted by (story, frame, at, id). A comment is placed at its frame while the frame is
    /// in the project; a comment on a check, at the live check its `MovedTo` chain ends at.
    fn story_comments_json(&self) -> Option<Vec<u8>> {
        let mut records: Vec<((u16, u16, WallTime, ChangeId), String)> = Vec::new();
        for k in self.story_attached() {
            let RecordKey::Comment(id) = &self.entry(k).key else {
                continue;
            };
            let Some(record) = self.projected_arc(k) else {
                continue;
            };
            let Record::Comment { value: comment, .. } = &*record else {
                continue;
            };
            let (place, on_check) = match &comment.target {
                CommentTarget::Frame { story, frame } => (Some((*story, *frame)), None),
                CommentTarget::Check(check) => {
                    let live = attach::follow_moves(self, check);
                    match live.target {
                        CheckTarget::Frame { story, frame } => (Some((story, frame)), Some(live)),
                        CheckTarget::Verse { .. } => (None, None),
                    }
                }
                CommentTarget::Verse { .. } => continue,
            };
            let placed = place.filter(|(story, frame)| self.frame_text(*story, *frame).is_some());
            let head = match &self.entry(k).projected {
                Projected::Change(c) => *c,
                _ => match self.entry(k).heads.first() {
                    Some(h) => *h,
                    None => continue,
                },
            };
            let meta = self.meta(head);
            let order = place.map_or((0, 0), |(s, f)| (u16::from(s.get()), f.0));
            let mut out = Map::new();
            out.insert("id".into(), Value::from(id.to_hex()));
            out.insert(
                "target".into(),
                canon::comment_target_json(&comment.target).map_or(Value::Null, |j| value(&j)),
            );
            out.insert(
                "placed".into(),
                placed.map_or(Value::Null, |(story, frame)| {
                    let mut p = Map::new();
                    if let Some(json) = on_check.as_ref().and_then(canon::check_key_json) {
                        p.insert("check".into(), value(&json));
                    }
                    p.insert("frame".into(), Value::from(frame.0));
                    p.insert("story".into(), Value::from(story.get()));
                    Value::Object(p)
                }),
            );
            out.insert("text".into(), Value::from(comment.text.as_str()));
            out.insert(
                "author".into(),
                attach::author(self, self.device_of(meta))
                    .map_or(Value::Null, |a| Value::from(a.as_ref())),
            );
            out.insert("at".into(), Value::from(rfc3339(meta.at)));
            records.push((
                (order.0, order.1, meta.at, meta.id),
                compact(&Value::Object(out)),
            ));
        }
        if records.is_empty() {
            return None;
        }
        records.sort();
        let records: Vec<String> = records.into_iter().map(|(_, r)| r).collect();
        Some(lines(
            "{\"schemaVersion\":1,\"book\":\"OBS\",\"comments\":[",
            &records,
            "]}",
        ))
    }

    // ---------------------------------------------------------------------------------
    // Comments and conflicts
    // ---------------------------------------------------------------------------------

    fn comments_json(&self, book: BookId) -> Option<Vec<u8>> {
        if !self.book(book).projects() {
            return None;
        }
        let keys: Vec<KeyIdx> = self
            .books
            .get(&book)
            .map(|b| {
                b.keys
                    .iter()
                    .copied()
                    .filter(|k| matches!(self.entry(*k).key, RecordKey::Comment(_)))
                    .collect()
            })
            .unwrap_or_default();
        let mut records: Vec<((u16, u16, WallTime, ChangeId), String)> = Vec::new();
        for k in keys {
            let RecordKey::Comment(id) = &self.entry(k).key else {
                continue;
            };
            let Some(record) = self.projected_arc(k) else {
                continue;
            };
            let Record::Comment { value: comment, .. } = &*record else {
                continue;
            };
            // A comment id held by changes that target several books shows with the book
            // its shown version targets.
            let target_book = match &comment.target {
                CommentTarget::Verse { book, .. } => Some(*book),
                CommentTarget::Check(check) => check.book(),
                CommentTarget::Frame { .. } => None,
            };
            if target_book != Some(book) {
                continue;
            }
            let head = match &self.entry(k).projected {
                Projected::Change(c) => *c,
                _ => match self.entry(k).heads.first() {
                    Some(h) => *h,
                    None => continue,
                },
            };
            let meta = self.meta(head);
            // A comment on a check is placed at the live check its `MovedTo` chain ends
            // at; it shows there and in that check's verse.
            let mut on_check: Option<CheckKey> = None;
            let placed: Option<(u16, u16)> = match &comment.target {
                CommentTarget::Verse { chapter, verse, .. } => {
                    match attach::place(self, book, (chapter.get(), verse.get())).0 {
                        attach::Place::Run(c, v) => Some((c, v)),
                        attach::Place::Nowhere => None,
                    }
                }
                CommentTarget::Check(check) => {
                    let live = attach::follow_moves(self, check);
                    let at = attach::target_exists(self, &live)
                        .then(|| check_verse(&live))
                        .flatten();
                    if at.is_some() {
                        on_check = Some(live);
                    }
                    at
                }
                CommentTarget::Frame { .. } => None,
            };
            let order = placed
                .or_else(|| match &comment.target {
                    CommentTarget::Verse { chapter, verse, .. } => {
                        Some((chapter.get(), verse.get()))
                    }
                    CommentTarget::Check(check) => check_verse(check),
                    CommentTarget::Frame { .. } => None,
                })
                .unwrap_or((0, 0));
            let mut record = Map::new();
            record.insert("id".into(), Value::from(id.to_hex()));
            record.insert(
                "target".into(),
                canon::comment_target_json(&comment.target).map_or(Value::Null, |j| value(&j)),
            );
            record.insert(
                "placed".into(),
                placed.map_or(Value::Null, |(c, v)| {
                    let mut p = Map::new();
                    if let Some(json) = on_check.as_ref().and_then(canon::check_key_json) {
                        p.insert("check".into(), value(&json));
                    }
                    p.insert("chapter".into(), Value::from(c));
                    p.insert("verse".into(), Value::from(v));
                    Value::Object(p)
                }),
            );
            record.insert("text".into(), Value::from(comment.text.as_str()));
            record.insert(
                "author".into(),
                attach::author(self, self.device_of(meta))
                    .map_or(Value::Null, |a| Value::from(a.as_ref())),
            );
            record.insert("at".into(), Value::from(rfc3339(meta.at)));
            records.push((
                (order.0, order.1, meta.at, meta.id),
                compact(&Value::Object(record)),
            ));
        }
        records.sort();
        let head = format!(
            "{{\"schemaVersion\":1,\"book\":{},\"comments\":[",
            compact(&Value::from(book.code()))
        );
        let records: Vec<String> = records.into_iter().map(|(_, r)| r).collect();
        Some(lines(&head, &records, "]}"))
    }

    fn conflicts_json(&self) -> Option<Vec<u8>> {
        if self.conflicts.is_empty() {
            return None;
        }
        let records: Vec<String> = self
            .conflicts()
            .map(|c| compact(&conflict_record(c)))
            .collect();
        Some(lines(
            "{\"schemaVersion\":1,\"conflicts\":[",
            &records,
            "]}",
        ))
    }

    // ---------------------------------------------------------------------------------
    // Project files
    // ---------------------------------------------------------------------------------

    fn pin_group(&self, group: PinGroup) -> Option<crate::record::PinGroupValue> {
        self.pins.groups.get(&group).cloned()
    }

    fn resources_json(&self) -> Option<Vec<u8>> {
        if self.pin_keys.is_empty() {
            return None;
        }
        let group_json = |group: PinGroup| -> Option<Value> {
            let value = self.pin_group(group)?;
            let mut slots: Vec<(String, Value)> = value
                .0
                .iter()
                .map(|(name, slot)| (name.as_str().to_owned(), slot_json(&slot.0)))
                .collect();
            slots.sort_by_key(|(name, _)| (slot_rank(name), name.clone()));
            Some(Value::Object(slots.into_iter().collect()))
        };
        // The members no pin group holds, by level, in name order after the modelled ones.
        let mut extras: BTreeMap<ResourcesLevel, BTreeMap<String, Value>> = BTreeMap::new();
        for k in self.pin_keys.iter() {
            if let RecordKey::ResourcesExtra(key) = &self.entry(*k).key
                && let Some(Record::ResourcesExtra {
                    value: Some(json), ..
                }) = self.projected_record(*k)
            {
                extras
                    .entry(key.level())
                    .or_default()
                    .insert(key.name().to_owned(), value(&json));
            }
        }
        let mut extra_of = |level: ResourcesLevel, map: &mut Map<String, Value>| {
            for (name, v) in extras.remove(&level).unwrap_or_default() {
                map.entry(name).or_insert(v);
            }
        };
        let mut doc = Map::new();
        doc.insert("schemaVersion".into(), Value::from(2));
        let mut sets = Map::new();
        if let Some(v) = group_json(PinGroup::Primary) {
            sets.insert("primary".into(), v);
        }
        if let Some(v) = group_json(PinGroup::Fallback) {
            sets.insert("fallback".into(), v);
        }
        extra_of(ResourcesLevel::LanguageSets, &mut sets);
        doc.insert("languageSets".into(), Value::Object(sets));
        let mut resources = Map::new();
        if let Some(v) = group_json(PinGroup::OriginalLanguage) {
            resources.insert("originalLanguage".into(), v);
        }
        if let Some(v) = group_json(PinGroup::Lexicon) {
            resources.insert("lexicon".into(), v);
        }
        extra_of(ResourcesLevel::Resources, &mut resources);
        doc.insert("resources".into(), Value::Object(resources));
        if let Some(extra) = self.pin_group(PinGroup::ExtraScripture) {
            // One slot holds the ordered list.
            if let Some((_, slot)) = extra.0.iter().next() {
                doc.insert("extraScripture".into(), value(&slot.0));
            }
        }
        extra_of(ResourcesLevel::Top, &mut doc);
        Some(pretty(&Value::Object(doc)))
    }

    fn settings_json(&self) -> Vec<u8> {
        let mut doc = Map::new();
        doc.insert("schemaVersion".into(), Value::from(1));
        let mut settings: Vec<(String, Value)> = Vec::new();
        for &k in self.setting_keys.iter() {
            if let RecordKey::Setting(name) = &self.entry(k).key
                && let Some(Record::Setting {
                    value: Some(json), ..
                }) = self.projected_record(k)
            {
                settings.push((name.as_str().to_owned(), value(&json)));
            }
        }
        settings.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, v) in settings {
            if name != "schemaVersion" {
                doc.insert(name, v);
            }
        }
        pretty(&Value::Object(doc))
    }

    // ---------------------------------------------------------------------------------
    // Open Bible Stories
    // ---------------------------------------------------------------------------------

    /// `content/NN.md`: the story's template with its title, frames and reference line
    /// spliced in byte for byte. The template's first `# ` line is frame 0, the paragraph
    /// after its n-th image line is frame n, and its last `_…_` line is the reference.
    fn story_markdown(&self, story: StoryNo) -> Option<Vec<u8>> {
        let base = match self.projected_record(self.key_idx(&RecordKey::StoryBase(story))?)? {
            Record::StoryBase { value, .. } => value,
            _ => return None,
        };
        let frame = |n: u16| -> Option<String> {
            let k = self.key_idx(&RecordKey::Frame(story, crate::ids::FrameNo(n)))?;
            match self.projected_record(k)? {
                Record::Frame { value, .. } => Some(value),
                _ => None,
            }
        };
        let reference = self.key_idx(&RecordKey::StoryRef(story)).and_then(|k| {
            match self.projected_record(k) {
                Some(Record::StoryRef { value, .. }) => value,
                _ => None,
            }
        });
        Some(splice_story(&base, frame, reference.as_deref()).into_bytes())
    }

    /// Where every carried file's bytes go (§7.7, §11.4): the provisional head at its path
    /// (or its `.case-` path), every other head of a conflict at its `.conflict-` path.
    pub fn carried_files(&self) -> Vec<CarriedPlacement> {
        let mut out = Vec::new();
        for k in self.carried_keys.iter() {
            let RecordKey::CarriedFile(path) = &self.entry(*k).key else {
                continue;
            };
            let provisional = match &self.entry(*k).projected {
                Projected::Change(c) => Some(*c),
                _ => None,
            };
            for h in self.entry(*k).heads.iter() {
                let Some(Record::CarriedFile {
                    value: CarriedValue::Present { oid, size, md5 },
                    ..
                }) = record_of(self, *h)
                else {
                    continue;
                };
                let at = if Some(*h) == provisional {
                    self.case_moves
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| path.as_str().to_owned())
                } else {
                    attach::moved_path(path.as_str(), &oid, "conflict")
                };
                out.push(CarriedPlacement {
                    path: at,
                    record: path.clone(),
                    oid,
                    size,
                    md5,
                    head: self.meta(*h).id,
                });
            }
        }
        out.sort();
        out
    }
}

/// The numeric chapter and first verse of a check's target.
fn check_verse(check: &CheckKey) -> Option<(u16, u16)> {
    match &check.target {
        CheckTarget::Verse { chapter, verse, .. } => Some((chapter.number()?, verse.span()?.0)),
        CheckTarget::Frame { .. } => None,
    }
}

/// A check's target verse as its key writes it: (chapter, verse).
type Target<'a> = (&'a str, &'a str);
/// A decision's key and the (chapter, verse) its target names.
type Targeted<'a> = (Target<'a>, KeyIdx);

/// The content a decision's target names by the no-text rule of §7.7, as tC4's Check screen
/// finds a check's draft text (`verseTextIndex` and `withSpanMembers`, `src/state.jsx` at
/// d310bf3, and `decisionIsStale`, `src/data/revalidate.ts`): a numeric chapter, and a verse
/// that is a number (the run that holds it) or a span label that a projected run carries
/// exactly. A run tC4 does not read as drafted ([`codec::is_drafted`]: the body its indexer
/// finds, trimmed, is empty or the stub `___`) gives no text. Every other target (`front`,
/// `intro`, a list such as `1,3,8,12`, a span no run carries) has none: it gives no
/// evidence either way.
pub(crate) fn target_content(text: &BookText, check: &CheckKey) -> Option<String> {
    verse_content(text, &check.target)
}

fn verse_content(text: &BookText, target: &CheckTarget) -> Option<String> {
    let CheckTarget::Verse { chapter, verse, .. } = target else {
        return None;
    };
    let chapter = text.chapters.get(&chapter.number()?)?;
    let start = match verse.number() {
        Some(v) => *chapter.run_of.get(&v)?,
        None => {
            let (first, last) = verse.span()?;
            let end = *chapter.ends.get(&first)?;
            (end == last && codec::label(first, end) == verse.as_str()).then_some(first)?
        }
    };
    chapter.drafted_content(start)
}

/// A JSON object written as a preserve-order `serde_json::Map` serializes compactly:
/// members in the order their keys were first set; a key set again keeps its place and
/// takes the new value. Records render through it without building a `Value` tree.
pub(crate) struct Obj<'a> {
    members: Vec<(&'a str, std::ops::Range<usize>)>,
    values: String,
}

impl<'a> Obj<'a> {
    pub(crate) fn new() -> Self {
        Self {
            members: Vec::with_capacity(16),
            values: String::with_capacity(256),
        }
    }

    /// Sets `key` to the JSON text `write` appends.
    fn raw(&mut self, key: &'a str, write: impl FnOnce(&mut String)) {
        let start = self.values.len();
        write(&mut self.values);
        let range = start..self.values.len();
        match self.members.iter_mut().find(|(k, _)| *k == key) {
            Some(member) => member.1 = range,
            None => self.members.push((key, range)),
        }
    }

    fn str(&mut self, key: &'a str, value: &str) {
        self.raw(key, |out| crate::json::write_string(value, out));
    }

    fn bool(&mut self, key: &'a str, value: bool) {
        self.raw(key, |out| {
            out.push_str(if value { "true" } else { "false" })
        });
    }

    fn int(&mut self, key: &'a str, value: impl Into<i128>) {
        let value = value.into();
        self.raw(key, |out| crate::json::write_int(value, out));
    }

    fn json(&mut self, key: &'a str, value: &Json) {
        self.raw(key, |out| value.write_str(out));
    }

    /// Sets every member of `extra`, the stored fields a newer minor added.
    fn extra(&mut self, extra: &'a crate::json::JsonMap) {
        for (k, v) in extra {
            self.json(k, v);
        }
    }

    fn write(&self, out: &mut String) {
        out.push('{');
        for (i, (key, range)) in self.members.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            crate::json::write_string(key, out);
            out.push(':');
            out.push_str(&self.values[range.clone()]);
        }
        out.push('}');
    }

    pub(crate) fn finish(&self) -> String {
        let mut out = String::with_capacity(self.values.len() + 16 * self.members.len());
        self.write(&mut out);
        out
    }
}

/// A canonical object (sorted keys) as compact JSON, as serde_json writes it.
fn write_map(map: &crate::json::JsonMap, out: &mut String) {
    crate::json::write_map(map, out);
}

/// Appends a JSON array of `items`, each written by `write`.
fn write_array<T>(out: &mut String, items: &[T], mut write: impl FnMut(&T, &mut String)) {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write(item, out);
    }
    out.push(']');
}

fn source_word(w: &SourceWord, out: &mut String) {
    let mut m = Obj::new();
    if let Some(s) = &w.strong {
        m.str("strong", s.as_str());
    }
    if let Some(s) = &w.lemma {
        m.str("lemma", s.as_str());
    }
    if let Some(s) = &w.morph {
        m.str("morph", s.as_str());
    }
    m.int("occurrence", w.occurrence);
    m.int("occurrences", w.occurrences);
    m.str("word", w.word.as_str());
    m.extra(&w.extra);
    m.write(out);
}

fn target_word(w: &TargetWord, out: &mut String) {
    let mut m = Obj::new();
    m.int("occurrence", w.occurrence);
    m.int("occurrences", w.occurrences);
    m.str("word", w.word.as_str());
    m.extra(&w.extra);
    m.write(out);
}

fn group(g: &AlignmentGroup, out: &mut String) {
    let mut m = Obj::new();
    m.raw("topWords", |o| write_array(o, &g.top_words, source_word));
    m.raw("bottomWords", |o| {
        write_array(o, &g.bottom_words, target_word)
    });
    m.extra(&g.extra);
    m.write(out);
}

/// Where one record of a book's alignment sidecar goes, and what is written beside it.
#[derive(Debug, Clone, Copy)]
enum AlignmentSlot {
    /// At the label of its run, `(chapter, verse, end)`.
    Placed {
        at: (u16, u16, u16),
        invalid: bool,
        anchor: Option<(u16, u16)>,
        span_end: Option<u16>,
    },
    /// At its own label `(chapter, verse)`, written invalid: it has no run to stand at, or
    /// an earlier record holds its run.
    Fallback {
        at: (u16, u16),
        span_end: Option<u16>,
    },
}

/// A tC4 §5.1 alignment record with `invalid` computed and every stored field kept.
pub(crate) fn alignment_record(a: &AlignmentValue, invalid: bool) -> Obj<'_> {
    let mut m = Obj::new();
    m.raw("alignments", |o| write_array(o, &a.alignments, group));
    m.raw("wordBank", |o| write_array(o, &a.word_bank, target_word));
    m.bool("invalid", invalid);
    m.str("targetVerseMd5", &a.target_verse_md5.to_hex());
    m.str("sourceVersion", a.source_version.as_ref());
    m.bool("done", a.done);
    if let Some(source) = &a.source {
        m.raw("source", |o| {
            let mut s = Obj::new();
            s.str("repoPath", source.repo_path.as_ref());
            s.str("sha", &source.sha.to_hex());
            s.write(o);
        });
    }
    if a.marked_invalid {
        m.bool("markedInvalid", true);
    }
    if a.dismissed {
        m.bool("dismissed", true);
    }
    m.extra(&a.extra);
    m
}

/// The anchor of a record placed at another verse (a renumber moved its text), so a decoder
/// reads it back under its own key.
fn add_anchor(record: &mut Obj<'_>, chapter: u16, verse: u16) {
    record.raw("anchor", |o| {
        let mut a = Obj::new();
        a.int("chapter", chapter);
        a.int("verse", verse);
        a.write(o);
    });
}

/// The span end of a record whose label does not say it.
fn add_span_end(record: &mut Obj<'_>, end: u16) {
    record.int("spanEnd", end);
}

/// tC4's `resolutionRecord` of a decision file: the pin the (tool, collection) resolves to,
/// its label and its language set. A pin without a commit has no `sha` (tC4 writes the
/// record all the same, and no decision matches it).
fn resolution_record(answer: &Answer) -> Value {
    let mut resource = Map::new();
    let (repo_path, sha) = match answer {
        Answer::Pin { pin, .. } => (
            Some(pin.resource.repo_path.to_string()),
            Some(pin.resource.sha.to_hex()),
        ),
        Answer::Unusable { pin, .. } => (pin.repo_path.clone(), None),
    };
    if let Some(repo_path) = repo_path {
        resource.insert("repoPath".into(), Value::from(repo_path));
    }
    if let Some(version) = answer.version() {
        resource.insert("version".into(), Value::from(version));
    }
    if let Some(sha) = sha {
        resource.insert("sha".into(), Value::from(sha));
    }
    resource.insert("languageSet".into(), Value::from(answer.rung().name()));
    Value::Object(resource)
}

/// A tC4 §5.2 decision record with its flags computed (§7.7) by [`decision::assess`].
/// `context` is the decision's context as JSON, written once by the caller, which also sorts
/// the file by it.
pub(crate) fn decision_record<'a>(
    d: &'a DecisionRecord,
    context: &str,
    check: &CheckKey,
    resolved: Option<&crate::record::ResourceRef>,
    verse: Option<&TargetText>,
    newest: WallTime,
) -> Obj<'a> {
    let assessment = decision::assess(d, resolved, verse);
    let invalidated = assessment.invalidation.is_some();
    let mut m = Obj::new();
    m.raw("contextId", |o| o.push_str(context));
    if let Some(category) = &d.category {
        m.str("category", category.as_ref());
    }
    match &d.judgement.selections {
        Some(list) => m.raw("selections", |o| {
            write_array(o, list, |s, o| {
                let mut sel = Obj::new();
                sel.str("text", s.text.as_str());
                sel.int("occurrence", s.occurrence);
                sel.int("occurrences", s.occurrences);
                sel.extra(&s.extra);
                sel.write(o);
            });
        }),
        None => m.bool("selections", false),
    }
    match &d.comment {
        Some(c) => m.str("comments", c.as_str()),
        None => m.bool("comments", false),
    }
    m.bool("reminders", d.bookmarked);
    m.bool("nothingToSelect", d.judgement.nothing_to_select);
    m.bool("verseEdits", assessment.verse_edits);
    m.bool("invalidated", invalidated);
    if let Some(status) = assessment.status {
        m.str("status", status.name());
    }
    if assessment.user_invalid {
        m.bool("userInvalid", true);
    }
    m.str("modifiedTimestamp", &rfc3339(newest));
    // What the journal stores beyond tC4's shape, so a decoder reads the record back.
    if let Some(s) = d.judgement.status {
        m.str("userStatus", s.name());
    }
    m.str("verseTextMd5", &d.judgement.verse_text_md5.to_hex());
    if resolved.is_none_or(|r| *r != d.resource) {
        m.raw("pinnedResource", |o| {
            let mut r = Obj::new();
            r.str("repoPath", d.resource.repo_path.as_ref());
            r.str("sha", &d.resource.sha.to_hex());
            r.write(o);
        });
    }
    if d.imported_invalid {
        m.bool("importedInvalid", true);
    }
    if d.dismissed {
        m.bool("dismissed", true);
    }
    if derive_key(check.tool, check.book(), &d.context).as_ref() != Some(check) {
        match canon::check_key_json(check) {
            Some(key) => m.json("checkKey", &key),
            None => m.raw("checkKey", |o| o.push_str("null")),
        }
    }
    m.extra(&d.extra);
    m
}

/// The check key a tC3 or tC4 context names: its check id, reference and occurrence. A
/// story reference (`{story, frame}`, tC4 §10.5) names a frame; any other names a verse of
/// `book`.
pub(crate) fn derive_key(
    tool: ToolId,
    book: Option<BookId>,
    context: &crate::json::JsonMap,
) -> Option<CheckKey> {
    let part = |j: &Json| -> Option<crate::keys::RefPart> {
        let text = match j {
            Json::Int(n) => n.get().to_string(),
            Json::Str(s) => s.clone(),
            _ => return None,
        };
        crate::keys::RefPart::parse(&text).ok()
    };
    let reference = context.get("reference")?.as_obj()?;
    let occurrence = match context.get("occurrence")? {
        Json::Int(n) => u32::try_from(n.get()).ok()?,
        _ => return None,
    };
    let number = |key: &str| match reference.get(key)? {
        Json::Int(n) => i64::try_from(n.get()).ok(),
        _ => None,
    };
    let target = if reference.contains_key("story") && !reference.contains_key("bookId") {
        CheckTarget::Frame {
            story: StoryNo::new(u8::try_from(number("story")?).ok()?)?,
            frame: crate::ids::FrameNo(u16::try_from(number("frame")?).ok()?),
        }
    } else {
        CheckTarget::Verse {
            book: book?,
            chapter: part(reference.get("chapter")?)?,
            verse: part(reference.get("verse")?)?,
        }
    };
    Some(CheckKey {
        tool,
        check_id: crate::keys::CheckId::parse(context.get("checkId")?.as_str()?).ok()?,
        target,
        occurrence,
    })
}

fn kind_name(kind: &ConflictKind) -> &'static str {
    match kind {
        ConflictKind::VerseText => "verseText",
        ConflictKind::VerseStructure => "verseStructure",
        ConflictKind::Passage => "passage",
        ConflictKind::StructureIssue => "structureIssue",
        ConflictKind::BookHeader => "bookHeader",
        ConflictKind::ChapterFront => "chapterFront",
        ConflictKind::BookPresence => "bookPresence",
        ConflictKind::BookRemovedWhileEdited { .. } => "bookRemovedWhileEdited",
        ConflictKind::LastBookRemoved => "lastBookRemoved",
        ConflictKind::Alignment => "alignment",
        ConflictKind::Check => "check",
        ConflictKind::CheckMoved => "checkMoved",
        ConflictKind::Comment => "comment",
        ConflictKind::Pins => "pins",
        ConflictKind::Setting => "setting",
        ConflictKind::ProjectMeta => "projectMeta",
        ConflictKind::Versification => "versification",
        ConflictKind::StoryBase => "storyBase",
        ConflictKind::Frame => "frame",
        ConflictKind::StoryRef => "storyRef",
        ConflictKind::CarriedFile => "carriedFile",
        ConflictKind::PathCase => "pathCase",
        ConflictKind::Unknown => "unknown",
        ConflictKind::BookLayout => "bookLayout",
        ConflictKind::DeviceProfile => "deviceProfile",
    }
}

fn conflict_record(c: &Conflict) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::from(c.id.to_hex()));
    m.insert(
        "key".into(),
        canon::encode_key(&c.key).map_or(Value::Null, |j| value(&j)),
    );
    if !c.also_keys.is_empty() {
        m.insert(
            "alsoKeys".into(),
            Value::Array(
                c.also_keys
                    .iter()
                    .map(|k| canon::encode_key(k).map_or(Value::Null, |j| value(&j)))
                    .collect(),
            ),
        );
    }
    m.insert("kind".into(), Value::from(kind_name(&c.kind)));
    m.insert("provisional".into(), Value::from(c.provisional.to_hex()));
    m.insert(
        "candidates".into(),
        Value::Array(
            c.candidates
                .iter()
                .map(|candidate| {
                    let mut x = Map::new();
                    x.insert("change".into(), Value::from(candidate.head.to_hex()));
                    x.insert("device".into(), Value::from(candidate.device.to_string()));
                    x.insert(
                        "author".into(),
                        candidate
                            .author
                            .as_ref()
                            .map_or(Value::Null, |a| Value::from(a.as_ref())),
                    );
                    x.insert("at".into(), Value::from(rfc3339(candidate.at)));
                    Value::Object(x)
                })
                .collect(),
        ),
    );
    Value::Object(m)
}

/// tC4's slot order in `resources.json`; slots it does not name come after, by name.
fn slot_rank(name: &str) -> u8 {
    match name {
        "gatewayLanguage" => 0,
        "translationNotes" => 1,
        "translationWordsLinks" => 2,
        "translationWords" => 3,
        "translationAcademy" => 4,
        "translationQuestions" => 5,
        "simplifiedText" => 6,
        "nt" => 7,
        "ot" => 8,
        _ => 9,
    }
}

/// A slot value with tC4's member order: the known members first, then the rest by name.
fn slot_json(j: &Json) -> Value {
    let Json::Obj(map) = j else {
        return value(j);
    };
    const ORDER: [&str; 8] = [
        "languageId",
        "owner",
        "repoPath",
        "version",
        "sha",
        "flavor",
        "books",
        "id",
    ];
    let mut m = Map::new();
    for name in ORDER {
        if let Some(v) = map.get(name) {
            m.insert(name.into(), value(v));
        }
    }
    for (k, v) in map {
        if !ORDER.contains(&k.as_str()) {
            m.insert(k.clone(), value(v));
        }
    }
    Value::Object(m)
}

/// Splices a story's title, frames and reference into its template (see
/// `FoldState::story_markdown`). Lines keep their terminators; a part with no record keeps
/// the template's text.
/// The template's text of frame `n` of a story, as [`splice_story`] cuts the template: frame
/// 0 is its first `# ` line without the `# `, frame n the paragraph after its n-th image
/// line, without the paragraph's last line ending.
pub(crate) fn template_frame(base: &str, n: u16) -> Option<String> {
    let lines: Vec<&str> = base.split_inclusive('\n').collect();
    let body = |line: &str| line.trim_end_matches(['\n', '\r']).to_owned();
    if n == 0 {
        return lines
            .iter()
            .map(|l| body(l))
            .find_map(|b| b.strip_prefix("# ").map(str::to_owned));
    }
    let reference_at = lines.iter().rposition(|l| {
        let b = body(l);
        b.starts_with('_') && b.ends_with('_') && b.len() > 1
    });
    let mut title_done = false;
    let mut images = 0u16;
    let mut i = 0;
    while i < lines.len() {
        let text = body(lines[i]);
        if !title_done && text.starts_with("# ") {
            title_done = true;
            i += 1;
            continue;
        }
        i += 1;
        if Some(i - 1) == reference_at {
            continue;
        }
        if text.starts_with("![") {
            images += 1;
            while i < lines.len() && body(lines[i]).trim().is_empty() {
                i += 1;
            }
            let start = i;
            while i < lines.len()
                && !body(lines[i]).trim().is_empty()
                && Some(i) != reference_at
                && !body(lines[i]).starts_with("![")
            {
                i += 1;
            }
            if images == n {
                let part: String = lines[start..i].concat();
                let end = part.trim_end_matches(['\n', '\r']).len();
                return Some(part[..end].to_owned());
            }
        }
    }
    None
}

pub(crate) fn splice_story(
    base: &str,
    frame: impl Fn(u16) -> Option<String>,
    reference: Option<&str>,
) -> String {
    let lines: Vec<&str> = base.split_inclusive('\n').collect();
    let terminator = |line: &str| -> &'static str {
        if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        }
    };
    let body = |line: &str| line.trim_end_matches(['\n', '\r']).to_owned();
    let reference_at = lines.iter().rposition(|l| {
        let b = body(l);
        b.starts_with('_') && b.ends_with('_') && b.len() > 1
    });
    let mut out = String::with_capacity(base.len());
    let mut title_done = false;
    let mut images = 0u16;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let text = body(line);
        if !title_done && text.starts_with("# ") {
            title_done = true;
            match frame(0) {
                Some(title) => {
                    out.push_str("# ");
                    out.push_str(&title);
                    out.push_str(terminator(line));
                }
                None => out.push_str(line),
            }
            i += 1;
            continue;
        }
        if Some(i) == reference_at {
            match reference {
                Some(r) => {
                    out.push('_');
                    out.push_str(r);
                    out.push('_');
                    out.push_str(terminator(line));
                }
                None => out.push_str(line),
            }
            i += 1;
            continue;
        }
        out.push_str(line);
        i += 1;
        if text.starts_with("![") {
            images += 1;
            // Blank lines, then the frame's paragraph up to the next blank line.
            while i < lines.len() && body(lines[i]).trim().is_empty() {
                out.push_str(lines[i]);
                i += 1;
            }
            let start = i;
            while i < lines.len()
                && !body(lines[i]).trim().is_empty()
                && Some(i) != reference_at
                && !body(lines[i]).starts_with("![")
            {
                i += 1;
            }
            match frame(images) {
                Some(text) => {
                    let end = lines.get(i.saturating_sub(1)).copied().unwrap_or("");
                    out.push_str(&text);
                    let t = if i > start { terminator(end) } else { "\n" };
                    out.push_str(t);
                }
                None => {
                    for l in &lines[start..i] {
                        out.push_str(l);
                    }
                }
            }
        }
    }
    out
}
