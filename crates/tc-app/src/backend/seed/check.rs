//! The check of a seed (STORAGE §13.2, tC4 R-8.8.2): what the fold projects must give the
//! folder back. Every rule runs and every mismatch is collected.
//!
//! "Field for field" is read as tC4 reads a record: an absent flag reads as false, a
//! decision's absent `status` reads as tC4 derives it, an alignment's `invalid` reads with
//! the I-3 hash rule, a timestamp as its instant, and record order is not compared. The
//! journal's own fields beside tC4's (`markedInvalid`, `userStatus`, …) are compared only
//! where the source has them. Derived fields of `metadata.json` are not compared: the
//! builder makes them (STORAGE §3.4), as tC4's server does.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::fold::{Anomaly, DerivedFile, decode_alignments, decode_decisions};
use tc_journal::pins::Collection;
use tc_journal::record::VerseValue;
use tc_journal::{
    BookId, ChapterNo, CheckTarget, FORMAT_MAJOR, FORMAT_MINOR, FrameNo, Record, RecordKey,
    StoryNo, ToolId, VerseNo,
};

use super::records::pins_records;
use super::strict::{canonical, first_difference, parse_utc_millis, read_json_strict};
use super::{
    Built, CheckRule, DeclaredDifference, METADATA, Mismatch, Role, Tc4Folder, describe_key,
    path_of_key,
};

/// What the check found.
#[derive(Debug, Default)]
pub(super) struct Checked {
    pub mismatches: Vec<Mismatch>,
    pub declared: Vec<DeclaredDifference>,
    pub added_files: Vec<String>,
}

pub(super) fn check(folder: &Tc4Folder, built: &Built) -> Checked {
    let mut checker = Checker {
        folder,
        built,
        out: Checked::default(),
    };
    checker.fold_invariants();
    checker.file_set();
    for (path, role) in &built.roles {
        let bytes = folder.files.get(path).map_or(&[][..], Vec::as_slice);
        match *role {
            Role::Usfm(book) => {
                checker.bytes(path, bytes, DerivedFile::Usfm(book), CheckRule::UsfmBytes)
            }
            Role::Versification => {
                checker.bytes(path, bytes, DerivedFile::Versification, CheckRule::VrsBytes);
            }
            Role::Story(story) => {
                checker.bytes(
                    path,
                    bytes,
                    DerivedFile::Story(story),
                    CheckRule::StoryBytes,
                );
                checker.frames(path, bytes, story);
            }
            Role::Carried => checker.carried(path, bytes),
            Role::Alignments(book) => checker.alignments(path, bytes, book),
            Role::Decisions(tool, book) => checker.decisions(path, bytes, tool, Some(book)),
            Role::StoryDecisions(tool) => checker.decisions(path, bytes, tool, None),
            Role::Resources => checker.resources(path, bytes),
            Role::Settings => checker.settings(path, bytes),
        }
    }
    checker.metadata();
    checker.out
}

struct Checker<'a> {
    folder: &'a Tc4Folder,
    built: &'a Built,
    out: Checked,
}

/// Compact JSON of a member, or `absent`.
fn shown(value: Option<&Value>) -> String {
    value.map_or_else(|| "absent".into(), Value::to_string)
}

/// The start of a file's text for a report: the whole of a short file.
fn excerpt(bytes: &[u8]) -> String {
    const MOST: usize = 400;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MOST {
        return format!("{text:?}");
    }
    let mut end = MOST;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}… ({} bytes)", &text[..end], bytes.len())
}

/// The first line where two texts differ, as `line N: "…"` for each side.
fn first_line_difference(source: &[u8], projected: &[u8]) -> (String, String) {
    let a = String::from_utf8_lossy(source);
    let b = String::from_utf8_lossy(projected);
    let mut left = a.split_inclusive('\n');
    let mut right = b.split_inclusive('\n');
    let mut line = 1;
    loop {
        match (left.next(), right.next()) {
            (Some(x), Some(y)) if x == y => line += 1,
            (x, y) => {
                let side = |s: Option<&str>| match s {
                    Some(s) => format!("line {line}: {s:?}"),
                    None => format!("line {line}: the end of the file"),
                };
                return (side(x), side(y));
            }
        }
    }
}

/// tC4's reading of a flag: absent is false.
fn flag(record: &Map<String, Value>, name: &str) -> Value {
    record.get(name).cloned().unwrap_or(Value::Bool(false))
}

fn is_true(record: &Map<String, Value>, name: &str) -> bool {
    record.get(name) == Some(&Value::Bool(true))
}

/// tC4's reading of a decision's `selections` (`normalizeDecision`, `src/data/httpStore.ts`
/// at d310bf3): an empty list is `false`. An absent one stays absent: tC4's `isDecided`
/// (`src/data/derive.ts:738`) reads it as a selection, unlike `false`.
fn selections(record: &Map<String, Value>) -> Option<Value> {
    match record.get("selections") {
        Some(Value::Array(items)) if items.is_empty() => Some(Value::Bool(false)),
        other => other.cloned(),
    }
}

/// tC4's reading of a decision's status: `normalizeDecision` reads `valid` on an invalidated
/// record as `invalid`, and BURRITO-SPEC §5.2 derives an absent one: invalidated is
/// `invalid`, done (`selections !== false || nothingToSelect === true`) is `valid`, else
/// `todo`.
fn status(record: &Map<String, Value>) -> Value {
    let invalidated = is_true(record, "invalidated");
    match record.get("status") {
        Some(Value::String(s)) if invalidated && s == "valid" => Value::from("invalid"),
        Some(status) => status.clone(),
        None if invalidated => Value::from("invalid"),
        None => {
            let done = selections(record) != Some(Value::Bool(false))
                || is_true(record, "nothingToSelect");
            Value::from(if done { "valid" } else { "todo" })
        }
    }
}

/// The fields of tC4's alignment record the check reads by its own rule.
const ALIGNMENT_READ: &[&str] = &[
    "alignments",
    "wordBank",
    "invalid",
    "targetVerseMd5",
    "sourceVersion",
    "done",
    "source",
];
/// The journal's fields beside tC4's in an alignment record.
const ALIGNMENT_JOURNAL: &[&str] = &["markedInvalid", "dismissed", "anchor", "spanEnd"];
/// The fields of tC4's decision record the check reads by its own rule.
const DECISION_READ: &[&str] = &[
    "contextId",
    "category",
    "selections",
    "comments",
    "reminders",
    "nothingToSelect",
    "invalidated",
    "userInvalid",
    "status",
    "modifiedTimestamp",
    "verseEdits",
];
/// The journal's fields beside tC4's in a decision record.
const DECISION_JOURNAL: &[&str] = &[
    "userStatus",
    "verseTextMd5",
    "pinnedResource",
    "importedInvalid",
    "dismissed",
    "checkKey",
];

// On the whole impl, because the rules between them read all of the fold: its conflicts, its
// derived files, each file rendered, and what metadata.json is built from.
#[expect(
    clippy::disallowed_methods,
    reason = "the seed check reads the whole fold once, when a fixture loads; no interactive path runs it"
)]
impl Checker<'_> {
    fn push(
        &mut self,
        path: &str,
        record: Option<String>,
        field: Option<&str>,
        source: String,
        projected: String,
        rule: CheckRule,
    ) {
        self.out.mismatches.push(Mismatch {
            path: path.to_owned(),
            record,
            field: field.map(str::to_owned),
            source,
            projected,
            rule,
        });
    }

    /// Rule 1: one version of every record, no anomaly, and each seeded change is the value
    /// the fold shows.
    fn fold_invariants(&mut self) {
        let fold = &self.built.fold;
        let conflicts: Vec<_> = fold
            .conflicts()
            .map(|c| (c.key.clone(), c.kind, c.candidates.len()))
            .collect();
        for (key, kind, versions) in conflicts {
            self.push(
                &path_of_key(&key),
                Some(describe_key(&key)),
                None,
                "one version".into(),
                format!("a {kind:?} conflict of {versions} versions"),
                CheckRule::FoldAnomaly,
            );
        }
        let anomalies: Vec<Anomaly> = fold.anomalies().cloned().collect();
        for anomaly in anomalies {
            match &anomaly {
                Anomaly::InvalidForBurrito { key, reason, .. } => {
                    // What the record gives metadata.json: a free field, a book's scope, or
                    // a pin group's relationships.
                    let (record, field) = match key {
                        RecordKey::ProjectMeta(field) => (None, field.path()),
                        RecordKey::BookPresence(_) => (Some(describe_key(key)), "scope".into()),
                        _ => (Some(describe_key(key)), "relationships".into()),
                    };
                    let source = self
                        .built
                        .meta_fields
                        .iter()
                        .find(|(path, _)| *path == field)
                        .map_or_else(
                            || format!("the value of {}", describe_key(key)),
                            |(_, v)| v.to_string(),
                        );
                    self.push(
                        &path_of_key(key),
                        record,
                        Some(&field),
                        source,
                        format!("left out of metadata.json: {reason}"),
                        CheckRule::FoldAnomaly,
                    );
                }
                other => self.push(
                    "",
                    None,
                    None,
                    "a project the fold reads without defect".into(),
                    format!("{other:?}"),
                    CheckRule::FoldAnomaly,
                ),
            }
        }
        for batch in &self.built.batches {
            for change in &batch.changes {
                let record = &change.body().record;
                let key = record.key();
                let projected = fold.projected(&key);
                let heads = fold.heads(&key);
                if projected.as_ref() != Some(record) || heads != [change.id()] {
                    self.push(
                        &path_of_key(&key),
                        Some(describe_key(&key)),
                        None,
                        format!("the seeded change {}", change.id()),
                        format!(
                            "{} from {} heads",
                            if projected.is_some() {
                                "another value"
                            } else {
                                "no value"
                            },
                            heads.len()
                        ),
                        CheckRule::NotProvisional,
                    );
                }
            }
        }
    }

    /// Rule 2: every derived file of the folder is one the fold derives, so a materialize
    /// writes it again; a file only `render` gives would be dropped.
    fn file_set(&mut self) {
        let derived: BTreeSet<DerivedFile> = self.built.fold.derived_files().into_iter().collect();
        let roles: Vec<(String, Role)> = self
            .built
            .roles
            .iter()
            .map(|(p, r)| (p.clone(), *r))
            .collect();
        for (path, role) in &roles {
            if let Some(file) = role.derived()
                && !derived.contains(&file)
            {
                let bytes = self.folder.files.get(path).map_or(&[][..], Vec::as_slice);
                self.push(
                    path,
                    None,
                    None,
                    excerpt(bytes),
                    "not among the files the fold derives, so it is not written".into(),
                    CheckRule::FileNotDerived,
                );
            }
        }
        for file in derived {
            let path = file.path();
            if !self.built.roles.contains_key(&path) {
                self.out.added_files.push(path);
            }
        }
    }

    /// Rule 3: a file the projection must give back byte for byte.
    fn bytes(&mut self, path: &str, source: &[u8], file: DerivedFile, rule: CheckRule) {
        match self.built.fold.render(&file) {
            Some(projected) if projected == source => {}
            Some(projected) => {
                let (s, p) = first_line_difference(source, &projected);
                self.push(path, None, None, s, p, rule);
            }
            None => self.push(path, None, None, excerpt(source), "absent".into(), rule),
        }
    }

    /// Rule 3, carried files: placed at their path with the source's blob id, size and
    /// md5, and the bytes kept under that id.
    fn carried(&mut self, path: &str, bytes: &[u8]) {
        let placement = self
            .built
            .fold
            .carried_files()
            .into_iter()
            .find(|p| p.path == path && p.record.as_str() == path);
        let Ok((Record::CarriedFile { value, .. }, oid)) = super::carried_record(path, bytes)
        else {
            return;
        };
        let expected = match value {
            tc_journal::record::CarriedValue::Present { size, md5, .. } => (oid, size, md5),
            tc_journal::record::CarriedValue::Removed => return,
        };
        let source = format!(
            "blob {} of {} bytes, md5 {}",
            expected.0, expected.1, expected.2
        );
        match placement {
            Some(p) if (p.oid, p.size, p.md5) == expected => {
                if self.built.carried.get(&p.oid).map(Vec::as_slice) != Some(bytes) {
                    self.push(
                        path,
                        None,
                        None,
                        source,
                        "no bytes kept under that blob".into(),
                        CheckRule::CarriedFile,
                    );
                }
            }
            Some(p) => self.push(
                path,
                None,
                None,
                source,
                format!("blob {} of {} bytes, md5 {}", p.oid, p.size, p.md5),
                CheckRule::CarriedFile,
            ),
            None => self.push(
                path,
                None,
                None,
                source,
                "absent".into(),
                CheckRule::CarriedFile,
            ),
        }
    }

    /// Rule 4: each frame of a story reads, through the fold, as tC4 reads it from the file
    /// (`parseStory`): what the story screens and the frame checks see.
    fn frames(&mut self, path: &str, bytes: &[u8], story: StoryNo) {
        let Ok(file) = tc_obs::StoryFile::parse(String::from_utf8_lossy(bytes).into_owned()) else {
            self.push(
                path,
                None,
                None,
                excerpt(bytes),
                "a story translationCore 4 cannot read".into(),
                CheckRule::FrameText,
            );
            return;
        };
        let texts = self.built.fold.check_texts(Collection::Stories);
        let count = u16::try_from(file.frame_count()).unwrap_or(u16::MAX);
        let mut found = Vec::new();
        for n in 0..=count.saturating_add(1) {
            let tc4 = file.text(tc_obs::FrameNo::new(n)).map(str::to_owned);
            let fold = texts
                .text(&CheckTarget::Frame {
                    story,
                    frame: FrameNo::new(n),
                })
                .map(|t| t.content().to_owned());
            if tc4 != fold {
                let rule = match (&tc4, &fold) {
                    (None, Some(_)) => CheckRule::RecordAdded,
                    (Some(_), None) => CheckRule::RecordMissing,
                    _ => CheckRule::FrameText,
                };
                found.push((n, tc4, fold, rule));
            }
        }
        drop(texts);
        for (n, tc4, fold, rule) in found {
            let show = |t: Option<String>| t.map_or_else(|| "absent".into(), |t| format!("{t:?}"));
            self.push(
                path,
                Some(format!("story {} frame {n}", story.get())),
                None,
                show(tc4),
                show(fold),
                rule,
            );
        }
    }

    /// The source and projected documents of a sidecar, read strictly.
    fn documents(
        &mut self,
        path: &str,
        bytes: &[u8],
        file: &DerivedFile,
    ) -> Option<(Value, Value)> {
        let source = read_json_strict(bytes).ok()?;
        let Some(projected) = self.built.fold.render(file) else {
            self.push(
                path,
                None,
                None,
                excerpt(bytes),
                "absent".into(),
                CheckRule::RoundTrip,
            );
            return None;
        };
        match read_json_strict(&projected) {
            Ok(doc) => Some((source, doc)),
            Err(e) => {
                self.push(
                    path,
                    None,
                    None,
                    "a JSON document".into(),
                    format!("unreadable JSON: {e:?}"),
                    CheckRule::RoundTrip,
                );
                None
            }
        }
    }

    /// Document members the projection writes exactly: equal on both sides; every other
    /// member but `allowed` is a mismatch on either side.
    fn document_fields(
        &mut self,
        path: &str,
        source: &Map<String, Value>,
        projected: &Map<String, Value>,
        equal: &[&str],
        allowed: &[&str],
    ) {
        for name in equal {
            if source.get(*name) != projected.get(*name) {
                self.push(
                    path,
                    None,
                    Some(name),
                    shown(source.get(*name)),
                    shown(projected.get(*name)),
                    CheckRule::DocumentField,
                );
            }
        }
        let mut others: BTreeSet<&String> = source.keys().chain(projected.keys()).collect();
        others.retain(|k| !equal.contains(&k.as_str()) && !allowed.contains(&k.as_str()));
        for name in others {
            self.push(
                path,
                None,
                Some(name),
                shown(source.get(name)),
                shown(projected.get(name)),
                CheckRule::DocumentField,
            );
        }
    }

    /// Records only one side has.
    fn missing_and_added<K: Ord + Clone>(
        &mut self,
        path: &str,
        source: &Records<'_, K>,
        projected: &Records<'_, K>,
    ) {
        for (key, (name, record)) in source {
            if !projected.contains_key(key) {
                self.push(
                    path,
                    Some(name.clone()),
                    None,
                    Value::Object((*record).clone()).to_string(),
                    "absent".into(),
                    CheckRule::RecordMissing,
                );
            }
        }
        for (key, (name, record)) in projected {
            if !source.contains_key(key) {
                self.push(
                    path,
                    Some(name.clone()),
                    None,
                    "absent".into(),
                    Value::Object((*record).clone()).to_string(),
                    CheckRule::RecordAdded,
                );
            }
        }
    }

    /// Rule 5: an alignments sidecar.
    fn alignments(&mut self, path: &str, bytes: &[u8], book: BookId) {
        let file = DerivedFile::Alignments(book);
        let Some((source, projected)) = self.documents(path, bytes, &file) else {
            return;
        };
        let empty = Map::new();
        let src = source.as_object().unwrap_or(&empty);
        let proj = projected.as_object().unwrap_or(&empty);
        self.document_fields(path, src, proj, &["schemaVersion", "book"], &["chapters"]);
        let (src_records, proj_records) =
            (alignment_records(book, src), alignment_records(book, proj));
        if src.get("chapters").is_some_and(|c| !c.is_object()) {
            self.push(
                path,
                None,
                Some("chapters"),
                shown(src.get("chapters")),
                shown(proj.get("chapters")),
                CheckRule::DocumentField,
            );
        }
        self.missing_and_added(path, &src_records, &proj_records);
        for (key, (name, s)) in &src_records {
            let Some((_, p)) = proj_records.get(key) else {
                continue;
            };
            let now = self.verse_md5(book, &key.0, &key.1);
            for (field, source, projected) in alignment_differences(s, p, now.as_deref()) {
                self.push(
                    path,
                    Some(name.clone()),
                    Some(&field),
                    source,
                    projected,
                    CheckRule::RecordField,
                );
            }
        }
        if let Some(rendered) = self.built.fold.render(&file) {
            let decoded = decode_alignments(book, &rendered).map_err(|e| e.to_string());
            self.round_trip(path, decoded);
        }
    }

    /// The I-3 hash of the text an alignment at `chapter`:`label` stands for now: the verse
    /// value the label's first verse holds, as the projection reads a run's content.
    fn verse_md5(&self, book: BookId, chapter: &str, label: &str) -> Option<String> {
        let chapter = ChapterNo::new(chapter.parse().ok()?)?;
        let first = label.split('-').next()?;
        let verse = VerseNo::new(first.parse().ok()?)?;
        match self
            .built
            .fold
            .projected(&RecordKey::Verse(book, chapter, verse))?
        {
            Record::Verse {
                value: VerseValue::Text { words, layout, .. },
                ..
            } => Some(verse_text_md5_v1(&format!("{words}{layout}")).to_hex()),
            _ => None,
        }
    }

    /// Rule 6: a decisions sidecar.
    fn decisions(&mut self, path: &str, bytes: &[u8], tool: ToolId, book: Option<BookId>) {
        let file = match book {
            Some(book) => DerivedFile::Decisions(tool, book),
            None => DerivedFile::StoryDecisions(tool),
        };
        let Some((source, projected)) = self.documents(path, bytes, &file) else {
            return;
        };
        let empty = Map::new();
        let src = source.as_object().unwrap_or(&empty);
        let proj = projected.as_object().unwrap_or(&empty);
        if src.contains_key("summary") {
            self.out.declared.push(DeclaredDifference::SummaryDropped {
                path: path.to_owned(),
            });
        }
        self.document_fields(
            path,
            src,
            proj,
            &["schemaVersion", "tool", "book", "resource"],
            &["decisions", "summary"],
        );
        let (src_records, proj_records) =
            (decision_records(tool, src), decision_records(tool, proj));
        self.missing_and_added(path, &src_records, &proj_records);
        for (key, (name, s)) in &src_records {
            let Some((_, p)) = proj_records.get(key) else {
                continue;
            };
            let (differences, declared) = decision_differences(path, name, s, p);
            self.out.declared.extend(declared);
            for (field, source, projected) in differences {
                self.push(
                    path,
                    Some(name.clone()),
                    Some(&field),
                    source,
                    projected,
                    CheckRule::RecordField,
                );
            }
        }
        if let Some(rendered) = self.built.fold.render(&file) {
            let decoded = decode_decisions(tool, book, &rendered).map_err(|e| e.to_string());
            self.round_trip(path, decoded);
        }
    }

    /// Decoding the projected sidecar gives every seeded record back, and no other.
    fn round_trip(&mut self, path: &str, decoded: Result<Vec<Record>, String>) {
        let decoded = match decoded {
            Ok(records) => records,
            Err(problem) => {
                self.push(
                    path,
                    None,
                    None,
                    "the seeded records".into(),
                    format!("a file that does not decode: {problem}"),
                    CheckRule::RoundTrip,
                );
                return;
            }
        };
        let by_key = |records: &[Record]| -> BTreeMap<RecordKey, Record> {
            records.iter().map(|r| (r.key(), r.clone())).collect()
        };
        let seeded = by_key(
            self.built
                .sidecar_records
                .get(path)
                .map_or(&[][..], Vec::as_slice),
        );
        let back = by_key(&decoded);
        let keys: BTreeSet<&RecordKey> = seeded.keys().chain(back.keys()).collect();
        for key in keys {
            let (a, b) = (seeded.get(key), back.get(key));
            if a != b {
                let say =
                    |r: Option<&Record>| r.map_or_else(|| "absent".into(), |r| format!("{r:?}"));
                self.push(
                    path,
                    Some(describe_key(key)),
                    None,
                    say(a),
                    say(b),
                    CheckRule::RoundTrip,
                );
            }
        }
    }

    /// Rule 7: `resources.json` gives back the file, or its migration, but for an empty
    /// member the projection always writes.
    fn resources(&mut self, path: &str, bytes: &[u8]) {
        // The builder refused a file it cannot read or migrate.
        let Some(expected) = read_json_strict(bytes)
            .ok()
            .and_then(|doc| pins_records(path, &doc).ok())
            .map(|pins| pins.expected)
        else {
            return;
        };
        let Some(rendered) = self.built.fold.render(&DerivedFile::Resources) else {
            return;
        };
        let Ok(mut projected) = read_json_strict(&rendered) else {
            self.push(
                path,
                None,
                None,
                "a JSON document".into(),
                "unreadable JSON".into(),
                CheckRule::RoundTrip,
            );
            return;
        };
        if let (Value::Object(p), Value::Object(e)) = (&mut projected, &expected) {
            for member in ["languageSets", "resources"] {
                let empty_added = !e.contains_key(member)
                    && p.get(member)
                        .and_then(Value::as_object)
                        .is_some_and(Map::is_empty);
                if empty_added {
                    p.remove(member);
                    self.out
                        .declared
                        .push(DeclaredDifference::EmptyMemberAdded {
                            path: path.to_owned(),
                            member: member.to_owned(),
                        });
                }
            }
        }
        self.json_document(path, &expected, &projected);
    }

    /// Rule 8: `settings.json` gives back the file.
    fn settings(&mut self, path: &str, bytes: &[u8]) {
        let Ok(source) = read_json_strict(bytes) else {
            return;
        };
        let Some(rendered) = self.built.fold.render(&DerivedFile::Settings) else {
            return;
        };
        match read_json_strict(&rendered) {
            Ok(projected) => self.json_document(path, &source, &projected),
            Err(_) => self.push(
                path,
                None,
                None,
                excerpt(bytes),
                "unreadable JSON".into(),
                CheckRule::RoundTrip,
            ),
        }
    }

    /// Two documents equal as JSON, member order aside.
    fn json_document(&mut self, path: &str, expected: &Value, projected: &Value) {
        if let Some(at) = first_difference(expected, projected) {
            let get = |doc: &Value| -> Option<Value> {
                let mut node = doc;
                for segment in at.split('.').filter(|s| !s.is_empty()) {
                    node = match node {
                        Value::Object(map) => map.get(segment)?,
                        Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
                        _ => return None,
                    };
                }
                Some(node.clone())
            };
            self.push(
                path,
                None,
                Some(if at.is_empty() { "(document)" } else { &at }),
                shown(get(expected).as_ref()),
                shown(get(projected).as_ref()),
                CheckRule::DocumentField,
            );
        }
    }

    /// Rule 9: every free field of `metadata.json` is what `metadata.json` is built from.
    fn metadata(&mut self) {
        let source = self
            .built
            .fold
            .metadata_source((FORMAT_MAJOR.into(), FORMAT_MINOR.into()));
        let fields: Vec<(String, Value)> = self.built.meta_fields.clone();
        for (path, value) in fields {
            let projected = source.fields.get(&path);
            if projected != Some(&value) {
                self.push(
                    METADATA,
                    None,
                    Some(&path),
                    value.to_string(),
                    shown(projected),
                    CheckRule::MetadataField,
                );
            }
        }
    }
}

/// A sidecar's records by their identity, each with its name.
type Records<'a, K> = BTreeMap<K, (String, &'a Map<String, Value>)>;

fn alignment_records(book: BookId, doc: &Map<String, Value>) -> Records<'_, (String, String)> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(chapters)) = doc.get("chapters") {
        for (chapter, verses) in chapters {
            let Value::Object(verses) = verses else {
                continue;
            };
            for (label, record) in verses {
                if let Value::Object(record) = record {
                    out.insert(
                        (chapter.clone(), label.clone()),
                        (format!("{} {chapter}:{label}", book.code()), record),
                    );
                }
            }
        }
    }
    out
}

/// A decisions sidecar's records by their canonical contextId (tC4's identity of a
/// record), with their names.
fn decision_records(tool: ToolId, doc: &Map<String, Value>) -> Records<'_, String> {
    let mut out = BTreeMap::new();
    if let Some(Value::Array(items)) = doc.get("decisions") {
        for item in items {
            if let Value::Object(record) = item {
                let context = record.get("contextId").unwrap_or(&Value::Null);
                out.insert(canonical(context), (decision_name(tool, record), record));
            }
        }
    }
    out
}

/// A decision in words: `translationNotes TIT 1:4 gr8c #1`, from its contextId.
fn decision_name(tool: ToolId, record: &Map<String, Value>) -> String {
    let context = record.get("contextId");
    let part = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "?".into(),
    };
    let reference = context.and_then(|c| c.get("reference"));
    let place = match reference {
        Some(r) if r.get("story").is_some() && r.get("bookId").is_none() => {
            format!("OBS {}:{}", part(r.get("story")), part(r.get("frame")))
        }
        Some(r) => format!(
            "{} {}:{}",
            part(r.get("bookId")).to_uppercase(),
            part(r.get("chapter")),
            part(r.get("verse"))
        ),
        None => "?".into(),
    };
    format!(
        "{} {place} {} #{}",
        tool.name(),
        part(context.and_then(|c| c.get("checkId"))),
        part(context.and_then(|c| c.get("occurrence")))
    )
}

/// The fields of an alignment record that read differently, as (field, source,
/// projected). `now` is the I-3 hash of the verse's current text, `None` when the record
/// stands at no verse text.
pub(crate) fn alignment_differences(
    source: &Map<String, Value>,
    projected: &Map<String, Value>,
    now: Option<&str>,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut differ = |field: &str, a: Option<&Value>, b: Option<&Value>| {
        if a != b {
            out.push((field.to_owned(), shown(a), shown(b)));
        }
    };
    for field in [
        "alignments",
        "wordBank",
        "targetVerseMd5",
        "sourceVersion",
        "source",
    ] {
        differ(field, source.get(field), projected.get(field));
    }
    differ(
        "done",
        Some(&flag(source, "done")),
        Some(&flag(projected, "done")),
    );
    // I-3: a record whose hash is not the verse's is invalid whatever its flag says.
    let invalid = |record: &Map<String, Value>| match now {
        Some(now) => Value::Bool(
            is_true(record, "invalid")
                || record.get("targetVerseMd5").and_then(Value::as_str) != Some(now),
        ),
        None => flag(record, "invalid"),
    };
    differ("invalid", Some(&invalid(source)), Some(&invalid(projected)));
    for field in ALIGNMENT_JOURNAL {
        if source.contains_key(*field) {
            differ(field, source.get(*field), projected.get(*field));
        }
    }
    let others: BTreeSet<&String> = source
        .keys()
        .chain(projected.keys())
        .filter(|k| {
            !ALIGNMENT_READ.contains(&k.as_str()) && !ALIGNMENT_JOURNAL.contains(&k.as_str())
        })
        .collect();
    for field in others {
        differ(field, source.get(field), projected.get(field));
    }
    out
}

/// The fields of a decision record that read differently, as (field, source, projected),
/// and the declared differences the record has.
pub(crate) fn decision_differences(
    path: &str,
    name: &str,
    source: &Map<String, Value>,
    projected: &Map<String, Value>,
) -> (Vec<(String, String, String)>, Vec<DeclaredDifference>) {
    let mut out = Vec::new();
    let mut declared = Vec::new();
    let mut differ = |field: &str, a: Option<&Value>, b: Option<&Value>| {
        if a != b {
            out.push((field.to_owned(), shown(a), shown(b)));
        }
    };
    for field in ["contextId", "category"] {
        differ(field, source.get(field), projected.get(field));
    }
    differ(
        "selections",
        selections(source).as_ref(),
        selections(projected).as_ref(),
    );
    for field in [
        "comments",
        "reminders",
        "nothingToSelect",
        "invalidated",
        "userInvalid",
    ] {
        differ(
            field,
            Some(&flag(source, field)),
            Some(&flag(projected, field)),
        );
    }
    differ("status", Some(&status(source)), Some(&status(projected)));
    let other = projected.get("modifiedTimestamp");
    if let Some(stamp) = source.get("modifiedTimestamp") {
        let instant = |v: Option<&Value>| v.and_then(Value::as_str).and_then(parse_utc_millis);
        match (instant(Some(stamp)), instant(other)) {
            (Some(a), Some(b)) if a == b => {
                if Some(stamp) != other {
                    declared.push(DeclaredDifference::TimestampRewritten {
                        path: path.to_owned(),
                        check: name.to_owned(),
                        source: shown(Some(stamp)),
                        projected: shown(other),
                    });
                }
            }
            _ => differ("modifiedTimestamp", Some(stamp), other),
        }
    } else if other.is_some() {
        declared.push(DeclaredDifference::TimestampAdded {
            path: path.to_owned(),
            check: name.to_owned(),
            projected: shown(other),
        });
    }
    let (edits_source, edits_projected) =
        (flag(source, "verseEdits"), flag(projected, "verseEdits"));
    if edits_source != edits_projected {
        declared.push(DeclaredDifference::VerseEditsRecomputed {
            path: path.to_owned(),
            check: name.to_owned(),
            source: edits_source == Value::Bool(true),
            projected: edits_projected == Value::Bool(true),
        });
    }
    for field in DECISION_JOURNAL {
        if source.contains_key(*field) {
            differ(field, source.get(*field), projected.get(*field));
        }
    }
    let others: BTreeSet<&String> = source
        .keys()
        .chain(projected.keys())
        .filter(|k| !DECISION_READ.contains(&k.as_str()) && !DECISION_JOURNAL.contains(&k.as_str()))
        .collect();
    for field in others {
        differ(field, source.get(field), projected.get(field));
    }
    (out, declared)
}
