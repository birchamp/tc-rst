//! F11: the derived files, projected lazily per book and per document (§7.5, §7.7).
//!
//! Every byte here is a function of the fold alone: no clock, machine or installed
//! resource (I12). Lists are sorted, so two devices with the same changes write the same
//! bytes (I1).

use super::attach::{self, ShownVerse, runs};
use super::books::rewound_batches;
use super::merge::{ancestors_or_self, record_of};
use super::meta::{ChangeIdx, KeyIdx, Shape, VerseShape};
use super::state::{FoldState, Projected};
use super::{Conflict, ConflictKind, Counter};
use crate::canon;
use crate::ids::{ChangeId, GitOid, RepoPath, StoryNo, WallTime};
use crate::json::Json;
use crate::keys::{CheckKey, CheckTarget, PinGroup, RecordKey, ToolId};
use crate::record::{
    AlignmentGroup, AlignmentValue, CarriedValue, ChapterFront, CheckValue, CommentTarget,
    DecisionRecord, LayoutItem, Record, SourceWord, TargetWord, UserStatus, VerseValue,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
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
}

fn tool_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::TranslationWords => "translationWords",
        ToolId::TranslationNotes => "translationNotes",
        ToolId::TranslationQuestions => "translationQuestions",
    }
}

impl DerivedFile {
    /// The repository path of the file.
    pub fn path(&self) -> String {
        match self {
            Self::Usfm(book) => format!("ingredients/{}.usfm", book.code()),
            Self::Alignments(book) => {
                format!("ingredients/checking/alignments/{}.json", book.code())
            }
            Self::Decisions(tool, book) => {
                format!(
                    "ingredients/checking/{}/{}.json",
                    tool_name(*tool),
                    book.code()
                )
            }
            Self::Comments(book) => format!("ingredients/tc/comments/{}.json", book.code()),
            Self::Resources => "ingredients/checking/resources.json".into(),
            Self::Settings => "ingredients/checking/settings.json".into(),
            Self::Versification => "ingredients/vrs.json".into(),
            Self::Conflicts => "ingredients/tc/conflicts.json".into(),
            Self::Story(story) => format!("ingredients/content/{:02}.md", story.get()),
        }
    }

    /// The `role` metadata.json lists for the file (§3.3).
    pub fn role(&self) -> Option<&'static str> {
        match self {
            Self::Usfm(_) | Self::Versification | Self::Story(_) => None,
            Self::Alignments(_) => Some("x-alignment"),
            Self::Decisions(..) => Some("x-check-decisions"),
            Self::Comments(_) => Some("x-tc-comments"),
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

/// The projected verse texts of a book, read once for a document.
pub(crate) struct BookText {
    /// chapter → verse → what the verse shows.
    pub chapters: BTreeMap<u16, ChapterText>,
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
        self.count(Counter::RecordsProjected, 1);
        match &self.entry(k).projected {
            Projected::Nothing => None,
            Projected::Change(c) => record_of(self, *c),
            Projected::Merged(record) => Some((**record).clone()),
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
                            record_of(self, *c)
                        }
                        ShownVerse::Merged(_, record) => Some((**record).clone()),
                    };
                    if let Some(text) = record.as_ref().and_then(verse_text) {
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
        for (book, entry) in self.books.iter() {
            if !entry.status.projects() {
                continue;
            }
            out.insert(DerivedFile::Usfm(*book));
            for k in &entry.keys {
                let e = self.entry(*k);
                if matches!(e.projected, Projected::Nothing) {
                    continue;
                }
                match &e.key {
                    RecordKey::Alignment(..) => {
                        out.insert(DerivedFile::Alignments(*book));
                    }
                    RecordKey::Check(check) => {
                        let decision = match &e.projected {
                            Projected::Change(c) => {
                                matches!(self.meta(*c).shape, Shape::Decision { .. })
                            }
                            Projected::Merged(r) => matches!(
                                &**r,
                                Record::Check {
                                    value: CheckValue::Decision(_),
                                    ..
                                }
                            ),
                            Projected::Nothing => false,
                        };
                        if decision {
                            out.insert(DerivedFile::Decisions(check.tool, *book));
                        }
                    }
                    RecordKey::Comment(_) => {
                        out.insert(DerivedFile::Comments(*book));
                    }
                    _ => {}
                }
            }
        }
        if !self.pin_keys.is_empty() {
            out.insert(DerivedFile::Resources);
        }
        if self.kind == super::ProjectKind::Bible {
            out.insert(DerivedFile::Settings);
        }
        if let Some(k) = self.key_ids.get(&RecordKey::Versification)
            && !matches!(self.entry(*k).projected, Projected::Nothing)
        {
            out.insert(DerivedFile::Versification);
        }
        if !self.conflicts.is_empty() {
            out.insert(DerivedFile::Conflicts);
        }
        for k in self.story_keys.iter() {
            if let RecordKey::StoryBase(story) = &self.entry(*k).key
                && !matches!(self.entry(*k).projected, Projected::Nothing)
            {
                out.insert(DerivedFile::Story(*story));
            }
        }
        let mut files: Vec<DerivedFile> = out.into_iter().collect();
        files.sort_by_key(DerivedFile::path);
        files
    }

    /// The bytes of a derived file, or `None` when the projection does not hold it.
    pub fn render(&self, file: &DerivedFile) -> Option<Vec<u8>> {
        match file {
            DerivedFile::Usfm(book) => self.book_usfm(*book).map(String::into_bytes),
            DerivedFile::Alignments(book) => self.alignments_json(*book),
            DerivedFile::Decisions(tool, book) => self.decisions_json(*tool, *book),
            DerivedFile::Comments(book) => self.comments_json(*book),
            DerivedFile::Resources => self.resources_json(),
            DerivedFile::Settings => Some(self.settings_json()),
            DerivedFile::Versification => {
                let k = self.key_ids.get(&RecordKey::Versification)?;
                match self.projected_record(*k)? {
                    Record::Versification { value } => Some(value.bytes.into_bytes()),
                    _ => None,
                }
            }
            DerivedFile::Conflicts => self.conflicts_json(),
            DerivedFile::Story(story) => self.story_markdown(*story),
        }
    }

    // ---------------------------------------------------------------------------------
    // Alignments
    // ---------------------------------------------------------------------------------

    fn alignments_json(&self, book: BookId) -> Option<Vec<u8>> {
        if !self.book(book).projects() {
            return None;
        }
        let text = self.book_text(book);
        let ol_sha = self.pins.original_sha(book);
        // (chapter, label) → record; placed records first.
        let mut placed: BTreeMap<(u16, u16, u16), Value> = BTreeMap::new();
        let mut fallback: Vec<((u16, u16), Value)> = Vec::new();
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
        for k in sorted {
            let RecordKey::Alignment(_, c, v) = &self.entry(k).key else {
                continue;
            };
            let (c, v) = (c.get(), v.get());
            let Some(Record::Alignment { value, .. }) = self.projected_record(k) else {
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
                    let md5 = md5_of(&tc_usfm::plain_text_v1(&content));
                    // The alignment covers as many verses as it did where it was made: a
                    // text that moved keeps its alignment valid at its new number.
                    let length = value.span_end.get().checked_sub(v);
                    let label_end = length.map_or(rv, |l| rv.saturating_add(l));
                    let valid = length.is_some()
                        && end == label_end
                        && md5 == value.target_verse_md5
                        && value.source.as_ref().is_none_or(|s| Some(s.sha) == ol_sha)
                        && !value.marked_invalid;
                    taken.insert((rc, rv));
                    let mut record = alignment_record(&value, !valid);
                    if (rc, rv) != (c, v) {
                        add_anchor(&mut record, c, v);
                    }
                    if value.span_end.get() != label_end {
                        add_span_end(&mut record, value.span_end.get());
                    }
                    placed.insert((rc, rv, label_end), record);
                }
                _ => {
                    let mut record = alignment_record(&value, true);
                    if value.span_end.get() != v {
                        add_span_end(&mut record, value.span_end.get());
                    }
                    fallback.push(((c, v), record));
                }
            }
        }
        let mut chapters: BTreeMap<u16, Vec<(u16, String)>> = BTreeMap::new();
        for ((c, v, end), record) in placed {
            let label = codec::label(v, end.max(v));
            chapters.entry(c).or_default().push((
                v,
                format!("{}:{}", compact(&Value::from(label)), compact(&record)),
            ));
        }
        for ((c, v), record) in fallback {
            // A record whose own label another record holds stays in the journal and in
            // the review's Detached list; the sidecar cannot hold two records at one label.
            if !taken.insert((c, v)) {
                continue;
            }
            chapters.entry(c).or_default().push((
                v,
                format!(
                    "{}:{}",
                    compact(&Value::from(v.to_string())),
                    compact(&record)
                ),
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

    fn decisions_json(&self, tool: ToolId, book: BookId) -> Option<Vec<u8>> {
        if !self.book(book).projects() {
            return None;
        }
        let text = self.book_text(book);
        let resolution = self.pins.resolution(tool, book);
        let resolved = resolution.as_ref().map(|r| r.0.clone());
        let mut records: Vec<(String, String)> = Vec::new();
        let keys: Vec<KeyIdx> = self
            .books
            .get(&book)
            .map(|b| b.keys.clone())
            .unwrap_or_default();
        for k in keys {
            let RecordKey::Check(check) = &self.entry(k).key else {
                continue;
            };
            if check.tool != tool {
                continue;
            }
            let Some(Record::Check {
                value: CheckValue::Decision(decision),
                ..
            }) = self.projected_record(k)
            else {
                continue;
            };
            let newest = self
                .entry(k)
                .heads
                .iter()
                .map(|h| self.meta(*h).at)
                .max()
                .unwrap_or(WallTime(0));
            let content = target_text(&text, check);
            let record = decision_record(&decision, check, resolved.as_ref(), &content, newest);
            let context = compact(&value(&Json::Obj(decision.context.clone())));
            records.push((context, compact(&record)));
        }
        records.sort();
        let mut head = Map::new();
        head.insert("schemaVersion".into(), Value::from(1));
        head.insert("tool".into(), Value::from(tool_name(tool)));
        head.insert("book".into(), Value::from(book.code()));
        if let Some((pin, set, slot)) = &resolution {
            let mut resource = Map::new();
            resource.insert("repoPath".into(), Value::from(pin.repo_path.as_ref()));
            if let Some(version) = slot.as_obj().and_then(|m| m.get("version")) {
                resource.insert("version".into(), value(version));
            }
            resource.insert("sha".into(), Value::from(pin.sha.to_hex()));
            resource.insert("languageSet".into(), Value::from(*set));
            head.insert("resource".into(), Value::Object(resource));
        }
        let mut head = compact(&Value::Object(head));
        head.pop();
        head.push_str(",\"decisions\":[");
        let records: Vec<String> = records.into_iter().map(|(_, r)| r).collect();
        Some(lines(&head, &records, "]}"))
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
            let Some(Record::Comment { value: comment, .. }) = self.projected_record(k) else {
                continue;
            };
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
                attach::author(self, meta.device).map_or(Value::Null, |a| Value::from(a.as_ref())),
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
        let mut doc = Map::new();
        doc.insert("schemaVersion".into(), Value::from(2));
        let mut sets = Map::new();
        if let Some(v) = group_json(PinGroup::Primary) {
            sets.insert("primary".into(), v);
        }
        if let Some(v) = group_json(PinGroup::Fallback) {
            sets.insert("fallback".into(), v);
        }
        doc.insert("languageSets".into(), Value::Object(sets));
        let mut resources = Map::new();
        if let Some(v) = group_json(PinGroup::OriginalLanguage) {
            resources.insert("originalLanguage".into(), v);
        }
        if let Some(v) = group_json(PinGroup::Lexicon) {
            resources.insert("lexicon".into(), v);
        }
        doc.insert("resources".into(), Value::Object(resources));
        if let Some(extra) = self.pin_group(PinGroup::ExtraScripture) {
            // One slot holds the ordered list.
            if let Some((_, slot)) = extra.0.iter().next() {
                doc.insert("extraScripture".into(), value(&slot.0));
            }
        }
        Some(pretty(&Value::Object(doc)))
    }

    fn settings_json(&self) -> Vec<u8> {
        let mut doc = Map::new();
        doc.insert("schemaVersion".into(), Value::from(1));
        let mut settings: Vec<(String, Value)> = Vec::new();
        for (key, k) in self.key_ids.iter() {
            if let RecordKey::Setting(name) = key
                && let Some(Record::Setting {
                    value: Some(json), ..
                }) = self.projected_record(*k)
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
        let base = match self.projected_record(*self.key_ids.get(&RecordKey::StoryBase(story))?)? {
            Record::StoryBase { value, .. } => value,
            _ => return None,
        };
        let frame = |n: u16| -> Option<String> {
            let k = self
                .key_ids
                .get(&RecordKey::Frame(story, crate::ids::FrameNo(n)))?;
            match self.projected_record(*k)? {
                Record::Frame { value, .. } => Some(value),
                _ => None,
            }
        };
        let reference = self.key_ids.get(&RecordKey::StoryRef(story)).and_then(|k| {
            match self.projected_record(*k) {
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

pub(crate) fn md5_of(text: &str) -> crate::ids::Md5 {
    crate::ids::Md5::from_bytes(tc_burrito::Md5::of(text.as_bytes()).bytes())
}

/// The numeric chapter and first verse of a check's target.
fn check_verse(check: &CheckKey) -> Option<(u16, u16)> {
    match &check.target {
        CheckTarget::Verse { chapter, verse, .. } => Some((
            chapter.as_str().parse().ok()?,
            verse.as_str().split('-').next()?.parse().ok()?,
        )),
        CheckTarget::Frame { .. } => None,
    }
}

/// The text a decision's target names: the runs covering its verse or span.
fn target_text(text: &BookText, check: &CheckKey) -> String {
    let CheckTarget::Verse { chapter, verse, .. } = &check.target else {
        return String::new();
    };
    let Ok(c) = chapter.as_str().parse::<u16>() else {
        return String::new();
    };
    let mut bounds = verse.as_str().split('-');
    let Some(first) = bounds.next().and_then(|v| v.parse::<u16>().ok()) else {
        return String::new();
    };
    let last = bounds
        .next()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(first)
        .max(first);
    let Some(chapter) = text.chapters.get(&c) else {
        return String::new();
    };
    let mut starts: Vec<u16> = (first..=last)
        .filter_map(|v| chapter.run_of.get(&v).copied())
        .collect();
    starts.dedup();
    starts
        .into_iter()
        .filter_map(|s| chapter.content(s))
        .collect::<Vec<_>>()
        .join("")
}

fn source_word(w: &SourceWord) -> Value {
    let mut m = Map::new();
    if let Some(s) = &w.strong {
        m.insert("strong".into(), Value::from(s.as_str()));
    }
    if let Some(s) = &w.lemma {
        m.insert("lemma".into(), Value::from(s.as_str()));
    }
    if let Some(s) = &w.morph {
        m.insert("morph".into(), Value::from(s.as_str()));
    }
    m.insert("occurrence".into(), Value::from(w.occurrence));
    m.insert("occurrences".into(), Value::from(w.occurrences));
    m.insert("word".into(), Value::from(w.word.as_str()));
    for (k, v) in &w.extra {
        m.insert(k.clone(), value(v));
    }
    Value::Object(m)
}

fn target_word(w: &TargetWord) -> Value {
    let mut m = Map::new();
    m.insert("occurrence".into(), Value::from(w.occurrence));
    m.insert("occurrences".into(), Value::from(w.occurrences));
    m.insert("word".into(), Value::from(w.word.as_str()));
    for (k, v) in &w.extra {
        m.insert(k.clone(), value(v));
    }
    Value::Object(m)
}

fn group(g: &AlignmentGroup) -> Value {
    let mut m = Map::new();
    m.insert(
        "topWords".into(),
        Value::Array(g.top_words.iter().map(source_word).collect()),
    );
    m.insert(
        "bottomWords".into(),
        Value::Array(g.bottom_words.iter().map(target_word).collect()),
    );
    for (k, v) in &g.extra {
        m.insert(k.clone(), value(v));
    }
    Value::Object(m)
}

/// A tC4 §5.1 alignment record with `invalid` computed and every stored field kept.
pub(crate) fn alignment_record(a: &AlignmentValue, invalid: bool) -> Value {
    let mut m = Map::new();
    m.insert(
        "alignments".into(),
        Value::Array(a.alignments.iter().map(group).collect()),
    );
    m.insert(
        "wordBank".into(),
        Value::Array(a.word_bank.iter().map(target_word).collect()),
    );
    m.insert("invalid".into(), Value::from(invalid));
    m.insert(
        "targetVerseMd5".into(),
        Value::from(a.target_verse_md5.to_hex()),
    );
    m.insert(
        "sourceVersion".into(),
        Value::from(a.source_version.as_ref()),
    );
    m.insert("done".into(), Value::from(a.done));
    if let Some(source) = &a.source {
        let mut s = Map::new();
        s.insert("repoPath".into(), Value::from(source.repo_path.as_ref()));
        s.insert("sha".into(), Value::from(source.sha.to_hex()));
        m.insert("source".into(), Value::Object(s));
    }
    if a.marked_invalid {
        m.insert("markedInvalid".into(), Value::from(true));
    }
    if a.dismissed {
        m.insert("dismissed".into(), Value::from(true));
    }
    for (k, v) in &a.extra {
        m.insert(k.clone(), value(v));
    }
    Value::Object(m)
}

/// The anchor of a record placed at another verse (a renumber moved its text), so a decoder
/// reads it back under its own key.
fn add_anchor(record: &mut Value, chapter: u16, verse: u16) {
    if let Value::Object(m) = record {
        let mut a = Map::new();
        a.insert("chapter".into(), Value::from(chapter));
        a.insert("verse".into(), Value::from(verse));
        m.insert("anchor".into(), Value::Object(a));
    }
}

/// The span end of a record whose label does not say it.
fn add_span_end(record: &mut Value, end: u16) {
    if let Value::Object(m) = record {
        m.insert("spanEnd".into(), Value::from(end));
    }
}

fn status_name(s: UserStatus) -> &'static str {
    match s {
        UserStatus::Valid => "valid",
        UserStatus::Invalid => "invalid",
        UserStatus::Todo => "todo",
    }
}

/// A tC4 §5.2 decision record with its flags computed (§7.7).
pub(crate) fn decision_record(
    d: &DecisionRecord,
    check: &CheckKey,
    resolved: Option<&crate::record::ResourceRef>,
    verse_text: &str,
    newest: WallTime,
) -> Value {
    let plain = tc_usfm::plain_text_v1(verse_text);
    let resource_ok = attach::resource_ok(&d.resource, resolved);
    let verse_changed = md5_of(&plain) != d.judgement.verse_text_md5;
    let selections_ok = !verse_changed
        || d.judgement.selections.as_ref().is_none_or(|selections| {
            let counts: Vec<tc_usfm::SelectionCount<'_>> = selections
                .iter()
                .map(|s| tc_usfm::SelectionCount {
                    text: &s.text,
                    occurrences: s.occurrences,
                })
                .collect();
            tc_usfm::selections_v1(verse_text, &counts)
        });
    let invalidated = !resource_ok || !selections_ok || d.imported_invalid;
    let mut m = Map::new();
    m.insert("contextId".into(), value(&Json::Obj(d.context.clone())));
    if let Some(category) = &d.category {
        m.insert("category".into(), Value::from(category.as_ref()));
    }
    m.insert(
        "selections".into(),
        match &d.judgement.selections {
            Some(list) => Value::Array(
                list.iter()
                    .map(|s| {
                        let mut sel = Map::new();
                        sel.insert("text".into(), Value::from(s.text.as_str()));
                        sel.insert("occurrence".into(), Value::from(s.occurrence));
                        sel.insert("occurrences".into(), Value::from(s.occurrences));
                        for (k, v) in &s.extra {
                            sel.insert(k.clone(), value(v));
                        }
                        Value::Object(sel)
                    })
                    .collect(),
            ),
            None => Value::from(false),
        },
    );
    m.insert(
        "comments".into(),
        d.comment
            .as_ref()
            .map_or(Value::from(false), |c| Value::from(c.as_str())),
    );
    m.insert("reminders".into(), Value::from(d.bookmarked));
    m.insert(
        "nothingToSelect".into(),
        Value::from(d.judgement.nothing_to_select),
    );
    m.insert("verseEdits".into(), Value::from(verse_changed));
    m.insert("invalidated".into(), Value::from(invalidated));
    let status = match (invalidated, d.judgement.status) {
        (true, Some(UserStatus::Todo)) => Some("todo"),
        (true, _) => Some("invalid"),
        (false, s) => s.map(status_name),
    };
    if let Some(status) = status {
        m.insert("status".into(), Value::from(status));
    }
    if invalidated && d.judgement.status == Some(UserStatus::Invalid) {
        m.insert("userInvalid".into(), Value::from(true));
    }
    m.insert("modifiedTimestamp".into(), Value::from(rfc3339(newest)));
    // What the journal stores beyond tC4's shape, so a decoder reads the record back.
    if let Some(s) = d.judgement.status {
        m.insert("userStatus".into(), Value::from(status_name(s)));
    }
    m.insert(
        "verseTextMd5".into(),
        Value::from(d.judgement.verse_text_md5.to_hex()),
    );
    if resolved.is_none_or(|r| *r != d.resource) {
        let mut r = Map::new();
        r.insert(
            "repoPath".into(),
            Value::from(d.resource.repo_path.as_ref()),
        );
        r.insert("sha".into(), Value::from(d.resource.sha.to_hex()));
        m.insert("pinnedResource".into(), Value::Object(r));
    }
    if d.imported_invalid {
        m.insert("importedInvalid".into(), Value::from(true));
    }
    if d.dismissed {
        m.insert("dismissed".into(), Value::from(true));
    }
    if derive_key(check.tool, check.book(), &d.context).as_ref() != Some(check) {
        let key = canon::check_key_json(check).map_or(Value::Null, |j| value(&j));
        m.insert("checkKey".into(), key);
    }
    for (k, v) in &d.extra {
        m.insert(k.clone(), value(v));
    }
    Value::Object(m)
}

/// The check key a tC3 context names: its check id, reference and occurrence.
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
    Some(CheckKey {
        tool,
        check_id: crate::keys::CheckId::parse(context.get("checkId")?.as_str()?).ok()?,
        target: CheckTarget::Verse {
            book: book?,
            chapter: part(reference.get("chapter")?)?,
            verse: part(reference.get("verse")?)?,
        },
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
