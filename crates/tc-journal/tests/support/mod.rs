//! A small team for fold tests: devices that write changes from their own views.

// Helpers build fixed, known-valid values and may fail loudly.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use tc_journal::fold::{FoldInput, FoldState, ProjectKind};
use tc_journal::json::{Json, JsonMap};
use tc_journal::record::{
    AlignmentValue, BookPresence, ChapterFront, ChapterVerse, CheckValue, CommentTarget,
    CommentValue, DecisionRecord, Judgement, MarkerWs, PinGroupValue, PresentHow, ResourceRef,
    Scope, SlotValue, UserStatus, UsfmBytes, VerseValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChangeId, ChapterNo, CheckId, CheckKey,
    CheckTarget, CommentId, DeviceId, DeviceSeq, GitOid, Md5, PinGroup, Record, RecordKey, RefPart,
    SlotName, ToolId, VerseNo, WallTime,
};

pub const BOOK: BookId = BookId::Titus;

pub fn ch(n: u16) -> ChapterNo {
    ChapterNo::new(n).expect("chapter")
}

pub fn vn(n: u16) -> VerseNo {
    VerseNo::new(n).expect("verse")
}

pub fn verse_key(c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(BOOK, ch(c), vn(v))
}

/// One writing device with its own clock.
#[derive(Debug, Clone)]
pub struct Dev {
    pub id: DeviceId,
    pub seq: u64,
    pub at: i64,
    batches: u64,
}

impl Dev {
    pub fn new(n: u8) -> Self {
        let mut random = [0u8; 15];
        random[0] = n;
        random[14] = 0x5a;
        Self {
            id: DeviceId::local(random),
            seq: 0,
            at: 1_760_000_000_000 + i64::from(n) * 1_000,
            batches: 0,
        }
    }

    /// One batch of changes, each with its parents.
    pub fn batch(&mut self, items: Vec<(Record, Vec<ChangeId>)>, cause: Cause) -> Vec<Change> {
        self.batches += 1;
        let mut id = [0u8; 16];
        id[..15].copy_from_slice(&self.id.as_bytes()[1..]);
        id[15] = (self.batches % 251) as u8;
        id[0] ^= (self.batches / 251) as u8;
        let batch = BatchRef::new(BatchId::from_bytes(id), items.len() as u32).expect("batch");
        items
            .into_iter()
            .map(|(record, mut parents)| {
                parents.sort();
                parents.dedup();
                self.seq += 1;
                self.at += 60_000;
                Change::new(ChangeBody {
                    v: 0,
                    record,
                    parents,
                    device: self.id,
                    seq: DeviceSeq(self.seq),
                    at: WallTime(self.at),
                    batch,
                    cause: cause.clone(),
                })
                .expect("a valid change")
            })
            .collect()
    }

    pub fn one(&mut self, record: Record, parents: Vec<ChangeId>) -> Change {
        self.batch(vec![(record, parents)], Cause::Edit)
            .pop()
            .expect("one change")
    }

    pub fn with(&mut self, record: Record, parents: Vec<ChangeId>, cause: Cause) -> Change {
        self.batch(vec![(record, parents)], cause)
            .pop()
            .expect("one change")
    }
}

pub fn text(words: &str, last: u16) -> VerseValue {
    VerseValue::Text {
        marker: MarkerWs::canonical(),
        words: words.into(),
        layout: "\n".into(),
        last: vn(last),
        from: None,
    }
}

pub fn text_layout(words: &str, layout: &str, last: u16) -> VerseValue {
    VerseValue::Text {
        marker: MarkerWs::canonical(),
        words: words.into(),
        layout: layout.into(),
        last: vn(last),
        from: None,
    }
}

pub fn text_from(words: &str, last: u16, from: (u16, u16)) -> VerseValue {
    VerseValue::Text {
        marker: MarkerWs::canonical(),
        words: words.into(),
        layout: "\n".into(),
        last: vn(last),
        from: Some(ChapterVerse {
            chapter: ch(from.0),
            verse: vn(from.1),
        }),
    }
}

pub fn covered(by: u16) -> VerseValue {
    VerseValue::Covered { by: vn(by) }
}

pub fn absent(moved_to: Option<(u16, u16)>) -> VerseValue {
    VerseValue::Absent {
        moved_to: moved_to.map(|(c, v)| ChapterVerse {
            chapter: ch(c),
            verse: vn(v),
        }),
    }
}

pub fn verse(c: u16, v: u16, value: VerseValue) -> Record {
    Record::Verse {
        book: BOOK,
        chapter: ch(c),
        verse: vn(v),
        value,
    }
}

pub fn present(how: PresentHow) -> Record {
    Record::BookPresence {
        book: BOOK,
        value: BookPresence::Present {
            scope: Scope::Whole,
            how,
        },
    }
}

pub fn removed(seen: &[ChangeId]) -> Record {
    let mut seen: Vec<_> = seen.iter().map(ChangeId::short).collect();
    seen.sort();
    seen.dedup();
    Record::BookPresence {
        book: BOOK,
        value: BookPresence::Removed { seen },
    }
}

pub fn header() -> Record {
    Record::BookHeader {
        book: BOOK,
        value: UsfmBytes("\\id TIT\n".into()),
    }
}

pub fn front(c: u16) -> Record {
    Record::ChapterFront {
        book: BOOK,
        chapter: ch(c),
        value: ChapterFront::Present(UsfmBytes(format!("\\c {c}\n\\p\n"))),
    }
}

/// A seeded book: presence, header, one front per chapter, stub verses (`___`).
pub fn seed_book(dev: &mut Dev, chapters: &[u16]) -> Vec<Change> {
    let mut items = vec![(present(PresentHow::Added), vec![]), (header(), vec![])];
    for (i, &count) in chapters.iter().enumerate() {
        let c = i as u16 + 1;
        items.push((front(c), vec![]));
        for v in 1..=count {
            items.push((verse(c, v, text("___", v)), vec![]));
        }
    }
    dev.batch(items, Cause::Seed)
}

/// A book drafted from the start, with real words: presence, header, fronts and verses.
pub fn imported_book(dev: &mut Dev, chapters: &[u16]) -> Vec<Change> {
    let mut items = vec![(present(PresentHow::Added), vec![]), (header(), vec![])];
    for (i, &count) in chapters.iter().enumerate() {
        let c = i as u16 + 1;
        items.push((front(c), vec![]));
        for v in 1..=count {
            items.push((verse(c, v, text(&format!("words of {c}:{v}"), v)), vec![]));
        }
    }
    dev.batch(
        items,
        Cause::Import(tc_journal::ImportSource::Usfm {
            file_name: "TIT.usfm".into(),
        }),
    )
}

pub fn md5(text: &str) -> Md5 {
    Md5::from_bytes(tc_burrito::Md5::of(text.as_bytes()).bytes())
}

pub fn oid(n: u8) -> GitOid {
    GitOid::from_bytes([n; 20])
}

pub fn tn_pin() -> ResourceRef {
    ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
        sha: oid(7),
    }
}

pub fn check_key(id: &str, c: u16, v: u16) -> CheckKey {
    CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse(id).expect("id"),
        target: CheckTarget::Verse {
            book: BOOK,
            chapter: RefPart::parse(&c.to_string()).expect("chapter"),
            verse: RefPart::parse(&v.to_string()).expect("verse"),
        },
        occurrence: 1,
    }
}

pub fn decision(
    status: Option<UserStatus>,
    bookmarked: bool,
    comment: Option<&str>,
) -> DecisionRecord {
    DecisionRecord {
        context: JsonMap::from([("checkId".to_owned(), Json::from("abcd"))]),
        category: Some("translate".into()),
        judgement: Judgement {
            selections: None,
            nothing_to_select: false,
            status,
            verse_text_md5: md5("x"),
        },
        comment: comment.map(str::to_owned),
        bookmarked,
        resource: tn_pin(),
        imported_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    }
}

pub fn check(key: &CheckKey, value: DecisionRecord) -> Record {
    Record::Check {
        key: key.clone(),
        value: CheckValue::Decision(value),
    }
}

pub fn moved(key: &CheckKey, to: &CheckKey) -> Record {
    Record::Check {
        key: key.clone(),
        value: CheckValue::MovedTo { to: to.clone() },
    }
}

pub fn alignment(c: u16, v: u16, span_end: u16, words: &str) -> Record {
    Record::Alignment {
        book: BOOK,
        chapter: ch(c),
        verse: vn(v),
        value: AlignmentValue {
            span_end: vn(span_end),
            alignments: vec![],
            word_bank: vec![tc_journal::record::TargetWord {
                word: words.into(),
                occurrence: 1,
                occurrences: 1,
                extra: JsonMap::new(),
            }],
            target_verse_md5: md5(words),
            source_version: "dcs::unfoldingWord/el-x-koine_ugnt@v0.34".into(),
            source: None,
            done: true,
            marked_invalid: false,
            dismissed: false,
            extra: JsonMap::new(),
        },
    }
}

pub fn comment(n: u8, target: CommentTarget, text: &str) -> Record {
    Record::Comment {
        id: CommentId::from_bytes([n; 16]),
        value: CommentValue {
            target,
            text: text.into(),
        },
    }
}

pub fn verse_target(c: u16, v: u16) -> CommentTarget {
    CommentTarget::Verse {
        book: BOOK,
        chapter: ch(c),
        verse: vn(v),
    }
}

pub fn pins(group: PinGroup, slots: &[(&str, Json)]) -> Record {
    let mut value = PinGroupValue::default();
    for (name, json) in slots {
        value.0.insert(
            SlotName::parse(name).expect("slot"),
            SlotValue(json.clone()),
        );
    }
    Record::Pins { group, value }
}

pub fn obj(fields: &[(&str, Json)]) -> Json {
    Json::Obj(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

/// The ids of changes.
pub fn ids(changes: &[Change]) -> Vec<ChangeId> {
    changes.iter().map(Change::id).collect()
}

/// The change of `changes` on `key`.
pub fn on(changes: &[Change], key: &RecordKey) -> Change {
    changes
        .iter()
        .find(|c| c.body().record.key() == *key)
        .cloned()
        .expect("a change on the key")
}

pub fn fold(changes: &[Change]) -> FoldState {
    FoldState::fold(
        ProjectKind::Bible,
        changes.iter().cloned().map(FoldInput::from),
    )
}

/// Everything about a fold that must be equal on two devices with the same changes.
pub fn summary(state: &FoldState) -> (String, Vec<String>, Vec<String>, Vec<String>) {
    (
        state.frontier().to_hex(),
        state.conflicts().map(|c| format!("{c:?}")).collect(),
        state.anomalies().map(|a| format!("{a:?}")).collect(),
        state.detached().map(|k| format!("{k:?}")).collect(),
    )
}

/// Words of a verse value.
pub fn words_of(record: &Record) -> Option<String> {
    match record {
        Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        } => Some(words.clone()),
        _ => None,
    }
}

pub fn projected_words(state: &FoldState, c: u16, v: u16) -> Option<String> {
    state.projected(&verse_key(c, v)).and_then(|r| words_of(&r))
}

pub type Views = BTreeMap<u8, Vec<Change>>;
