//! The row's `probe_finds_every_key_a_scan_finds` (APP-DESIGN §4.3, A-21): the key reads tc-app
//! makes (R1's range reads through `CountingSnapshot`) find exactly the keys a scan of every
//! record finds, so a read that misses a record fails here instead of hiding work. Five
//! projects: the sample Bible project, the same with two teammates' heads on Titus 1:1, a
//! project with records beyond its versification and in chapters with no verse, Psalms, and
//! the corpus ten times larger.

use std::collections::BTreeSet;
use std::sync::Arc;

use tc_app::backend::ProjectStore;
use tc_app::backend::memory::{MemoryBackend, MemoryProject, teammates};
use tc_app::counters::{AppCounters, CountingSnapshot};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind};
use tc_journal::json::{Json, JsonMap};
use tc_journal::record::{
    AlignmentValue, BookPresence, ChapterFront, CheckValue, CommentTarget, CommentValue,
    DecisionRecord, Judgement, MarkerWs, PresentHow, ResourceRef, Scope, TargetWord, UsfmBytes,
    VerseValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChapterNo, CheckId, CheckKey,
    CheckTarget, CommentId, DeviceId, DeviceSeq, GitOid, Md5, ProjectId, Record, RecordKey,
    RefPart, ToolId, VerseNo, WallTime,
};

use super::corpus::corpora;
use crate::common::{folder, open_progress, sample_bible, tit};

const TITUS: BookId = BookId::Titus;

fn ch(n: u16) -> ChapterNo {
    ChapterNo::new(n).unwrap()
}

fn vn(n: u16) -> VerseNo {
    VerseNo::new(n).unwrap()
}

fn md5(text: &str) -> Md5 {
    Md5::from_bytes(tc_burrito::Md5::of(text.as_bytes()).bytes())
}

/// One writing device, as tc-journal's own test support writes changes.
struct Dev {
    id: DeviceId,
    seq: u64,
    batches: u8,
}

impl Dev {
    fn new() -> Self {
        Self {
            id: DeviceId::local([0x42; 15]),
            seq: 0,
            batches: 0,
        }
    }

    fn batch(&mut self, records: Vec<Record>) -> Vec<Change> {
        self.batches += 1;
        let batch = BatchRef::new(
            BatchId::from_bytes([self.batches; 16]),
            records.len() as u32,
        )
        .unwrap();
        records
            .into_iter()
            .map(|record| {
                self.seq += 1;
                Change::new(ChangeBody {
                    v: 0,
                    record,
                    parents: Vec::new(),
                    device: self.id,
                    seq: DeviceSeq(self.seq),
                    at: WallTime(1_760_000_000_000 + self.seq as i64 * 60_000),
                    batch,
                    cause: Cause::Edit,
                })
                .unwrap()
            })
            .collect()
    }
}

fn verse(c: u16, v: u16, words: &str) -> Record {
    Record::Verse {
        book: TITUS,
        chapter: ch(c),
        verse: vn(v),
        value: VerseValue::Text {
            marker: MarkerWs::canonical(),
            words: words.into(),
            layout: "\n".into(),
            last: vn(v),
            from: None,
        },
    }
}

fn alignment(c: u16, v: u16) -> Record {
    Record::Alignment {
        book: TITUS,
        chapter: ch(c),
        verse: vn(v),
        value: AlignmentValue {
            span_end: vn(v),
            alignments: vec![],
            word_bank: vec![TargetWord {
                word: "word".into(),
                occurrence: 1,
                occurrences: 1,
                extra: JsonMap::new(),
            }],
            target_verse_md5: md5("word"),
            source_version: "dcs::unfoldingWord/el-x-koine_ugnt@v0.34".into(),
            source: None,
            done: true,
            marked_invalid: false,
            dismissed: false,
            extra: JsonMap::new(),
        },
    }
}

fn decision(c: u16, v: u16) -> Record {
    Record::Check {
        key: CheckKey {
            tool: ToolId::TranslationNotes,
            check_id: CheckId::parse("bv01").unwrap(),
            target: CheckTarget::Verse {
                book: TITUS,
                chapter: RefPart::parse(&c.to_string()).unwrap(),
                verse: RefPart::parse(&v.to_string()).unwrap(),
            },
            occurrence: 1,
        },
        value: CheckValue::Decision(DecisionRecord {
            context: JsonMap::from([("checkId".to_owned(), Json::from("bv01"))]),
            category: Some("translate".into()),
            judgement: Judgement {
                selections: None,
                nothing_to_select: false,
                status: None,
                verse_text_md5: md5("x"),
            },
            comment: None,
            bookmarked: true,
            resource: ResourceRef {
                repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
                sha: GitOid::from_bytes([7; 20]),
            },
            imported_invalid: false,
            dismissed: false,
            extra: JsonMap::new(),
        }),
    }
}

fn comment(n: u8, c: u16, v: u16) -> Record {
    Record::Comment {
        id: CommentId::from_bytes([n; 16]),
        value: CommentValue {
            target: CommentTarget::Verse {
                book: TITUS,
                chapter: ch(c),
                verse: vn(v),
            },
            text: "a note".into(),
        },
    }
}

/// Titus with verses 3:16 and 3:17 and chapter 4 (Titus has three chapters, 15 verses in
/// chapter 3), an alignment, a decision and a comment there, and an alignment and a comment
/// in chapters 5 and 6, which hold no verse.
fn beyond_versification() -> MemoryProject {
    let mut dev = Dev::new();
    let mut records = vec![
        Record::BookPresence {
            book: TITUS,
            value: BookPresence::Present {
                scope: Scope::Whole,
                how: PresentHow::Added,
            },
        },
        Record::BookHeader {
            book: TITUS,
            value: UsfmBytes("\\id TIT\n".into()),
        },
    ];
    for c in [3u16, 4] {
        records.push(Record::ChapterFront {
            book: TITUS,
            chapter: ch(c),
            value: ChapterFront::Present(UsfmBytes(format!("\\c {c}\n\\p\n"))),
        });
    }
    records.extend([
        verse(3, 16, "beyond the frame"),
        verse(3, 17, "beyond it too"),
        verse(4, 1, "a fourth chapter"),
        verse(4, 2, "with verses"),
        verse(4, 3, "and notes"),
    ]);
    let mut changes = dev.batch(records);
    changes.extend(dev.batch(vec![
        alignment(4, 1),
        decision(4, 2),
        comment(1, 4, 3),
        alignment(5, 1),
        comment(2, 6, 1),
    ]));
    MemoryProject::from_changes(
        ProjectId::from_bytes([0xbe; 16]),
        ProjectKind::Bible,
        changes,
    )
}

fn with_teammates() -> MemoryProject {
    let mut project = sample_bible();
    let changes = teammates::edit_verse(&project.fold, &tit(1, 1), &["one", "two"]).unwrap();
    project.fold.add(changes.into_iter().map(FoldInput::from));
    assert_eq!(project.fold.heads(&tit(1, 1)).len(), 2);
    project
}

/// The comments whose history names `book`, as tc-journal's own differential test filters
/// them.
fn scanned_comments(fold: &FoldState, all: &[RecordKey], book: BookId) -> Vec<RecordKey> {
    all.iter()
        .filter(|k| {
            matches!(k, RecordKey::Comment(_))
                && fold.history(k).iter().any(|id| {
                    matches!(
                        fold.body(id).map(|b| b.record),
                        Some(Record::Comment { value, .. }) if match &value.target {
                            CommentTarget::Verse { book: b, .. } => *b == book,
                            CommentTarget::Check(c) => c.book() == Some(book),
                            CommentTarget::Frame { .. } => false,
                        }
                    )
                })
        })
        .cloned()
        .collect()
}

/// Compares every key read of `project` with a scan of all its records.
fn probes_find_what_a_scan_finds(name: &str, project: &MemoryProject) {
    let backend = MemoryBackend::new();
    backend.add(folder("probed"), project.clone()).unwrap();
    let handle = backend.open(&folder("probed"), open_progress()).unwrap();
    let snapshot = handle.snapshot();
    let read = CountingSnapshot::new(snapshot.clone(), Arc::new(AppCounters::default()));
    let fold = snapshot.fold();
    let all = fold.record_keys();
    let books: BTreeSet<BookId> = all
        .iter()
        .filter_map(RecordKey::book)
        .chain(read.projected_books())
        .collect();
    assert!(!books.is_empty(), "{name}");
    let of_book = |want: fn(&RecordKey, BookId) -> bool, book: BookId| -> Vec<RecordKey> {
        all.iter().filter(|k| want(k, book)).cloned().collect()
    };
    for book in books {
        let mut verses = Vec::new();
        let mut fronts = Vec::new();
        let mut alignments = BTreeSet::new();
        for chapter in read.chapters_of(book) {
            let keys = read.chapter_keys(book, chapter);
            verses.extend(keys.verses);
            fronts.extend(keys.front);
            alignments.extend(keys.alignments);
        }
        verses.sort();
        fronts.sort();
        assert_eq!(
            verses,
            of_book(|k, b| matches!(k, RecordKey::Verse(x, ..) if *x == b), book),
            "{name} {book:?} verses"
        );
        assert_eq!(
            fronts,
            of_book(
                |k, b| matches!(k, RecordKey::ChapterFront(x, _) if *x == b),
                book
            ),
            "{name} {book:?} chapter fronts"
        );
        let book_alignments = read.alignments_of(book);
        assert_eq!(
            book_alignments,
            of_book(
                |k, b| matches!(k, RecordKey::Alignment(x, ..) if *x == b),
                book
            ),
            "{name} {book:?} alignments"
        );
        // A chapter with no verse is in no `chapters_of` list, so a loop over them may miss
        // an alignment anchored there: book-wide reads use `alignments_of`.
        assert!(
            alignments.iter().all(|k| book_alignments.contains(k)),
            "{name} {book:?}"
        );
        for tool in ToolId::ALL {
            let scan: Vec<RecordKey> = all
                .iter()
                .filter(|k| {
                    matches!(k, RecordKey::Check(c) if c.tool == tool
                        && matches!(&c.target, CheckTarget::Verse { book: b, .. } if *b == book))
                })
                .cloned()
                .collect();
            assert_eq!(
                read.decisions_of(tool, book),
                scan,
                "{name} {book:?} {tool}"
            );
        }
        assert_eq!(
            read.comments_of(book),
            scanned_comments(fold, &all, book),
            "{name} {book:?} comments"
        );
    }
    let detached: Vec<RecordKey> = fold.detached().cloned().collect();
    assert_eq!(read.detached(), detached, "{name} detached");
}

#[test]
fn probe_finds_every_key_a_scan_finds() {
    probes_find_what_a_scan_finds("the sample Bible project", &sample_bible());
    probes_find_what_a_scan_finds("two teammates on Titus 1:1", &with_teammates());

    let beyond = beyond_versification();
    // The project holds what it is for: records past the frame and in empty chapters.
    assert_eq!(beyond.fold.chapters_of(TITUS), [3, 4]);
    assert_eq!(beyond.fold.alignments_of(TITUS).len(), 2);
    assert_eq!(beyond.fold.comments_of(TITUS).len(), 2);
    assert_eq!(
        beyond
            .fold
            .decisions_of(ToolId::TranslationNotes, TITUS)
            .len(),
        1
    );
    probes_find_what_a_scan_finds("records beyond the versification", &beyond);

    let corpora = corpora();
    probes_find_what_a_scan_finds("Psalms", &corpora.psalms);
    probes_find_what_a_scan_finds("ten times Psalms", &corpora.larger);
}
