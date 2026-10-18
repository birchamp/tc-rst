//! Materialization book by book (`docs/STORAGE.md` §5.10, PERFORMANCE.md): what a change
//! marks, what a group touches, how often a buffer is hashed, the progress a long job shows,
//! and the typed notice of a job that cannot write (APP-DESIGN R16).

use crate::common::*;
use crate::corpus::{OnDisk, psalms, psalms_x10};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::{Blocked, Notice};
use tc_journal::record::Scope;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::fault::OpKind;
use tc_store::{
    CommandError, Leave, OpenOptions, ProjectHandle, StoreCounters, StoreEvent, StoreReply, Work,
};

const PSA: BookId = BookId::Psalms;

fn psalm(c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(PSA, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap())
}

fn save_psalm(
    handle: &ProjectHandle,
    c: u16,
    v: u16,
    words: &str,
) -> Result<StoreReply, CommandError> {
    let shown = handle.mint(&psalm(c, v), None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.to_owned(),
        })
        .wait()
}

fn words_of(handle: &ProjectHandle, key: &RecordKey) -> String {
    match handle.snapshot().fold().projected(key) {
        Some(Record::Verse {
            value: tc_journal::record::VerseValue::Text { words, .. },
            ..
        }) => words,
        other => panic!("{other:?}"),
    }
}

/// The book code a path of a book's file names: `TIT` of `ingredients/TIT.usfm`,
/// `ingredients/checking/alignments/TIT.json` or the heads file of either.
fn book_of(path: &Path) -> Option<String> {
    let text = path.to_string_lossy().replace("%2F", "/");
    let name = text.rsplit('/').next()?;
    let stem = name.split('.').next()?;
    (stem.len() == 3
        && stem
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
    .then(|| stem.to_owned())
}

/// What a save of one Psalms verse and `leave` cost on a copy of a corpus project, after the
/// entries were settled.
struct LeaveCost {
    counters: StoreCounters,
    opened: Vec<std::path::PathBuf>,
    written: Vec<std::path::PathBuf>,
    psalms_files: usize,
}

fn psalms_leave(corpus: &OnDisk) -> LeaveCost {
    let fs = corpus.fs.copy();
    fs.advance(Duration::from_secs(5));
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    fs.advance(Duration::from_secs(5));
    save_psalm(&handle, 23, 1, "The LORD is my shepherd, I shall not want").unwrap();
    handle.wait_idle();
    fs.clear_log();
    let before = handle.counters();
    handle.leave(Leave::Book(PSA)).wait().unwrap();
    let counters = handle.counters().since(&before);
    let opened = fs.opened();
    let written: Vec<_> = fs
        .log()
        .into_iter()
        .filter(|op| matches!(op.kind, OpKind::Rename | OpKind::RemoveFile))
        .map(|op| op.path)
        .collect();
    let psalms_files = handle.snapshot().fold().book_files(PSA).len();
    handle.close().unwrap();
    LeaveCost {
        counters,
        opened,
        written,
        psalms_files,
    }
}

/// B5 and the writer: a verse save marks its book, the writer renders and compares that
/// book's files only, and the files it opens and writes are that book's, `metadata.json`,
/// the manifest and the intent: the same files on the corpus ten times larger.
#[test]
fn a_books_materialize_touches_only_that_books_files() {
    let mut costs = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let cost = psalms_leave(corpus);
        let c = &cost.counters;
        assert!(c.projected >= 1, "{c:?}");
        // The book's files, and conflicts.json when the corpus has conflicts on it.
        assert!(
            c.projected <= cost.psalms_files as u64 + 1,
            "{c:?} for {} files",
            cost.psalms_files
        );
        assert!(c.derived_read <= cost.psalms_files as u64 + 1, "{c:?}");
        for path in cost.opened.iter().chain(&cost.written) {
            let text = path.to_string_lossy();
            assert!(
                !text.contains("/ingredients/tc/changes/") && !text.contains("/.tc/pending/"),
                "{text}: no journal file is touched"
            );
            if let Some(book) = book_of(path) {
                assert_eq!(book, "PSA", "{text} belongs to another book");
            }
        }
        let allowed = |p: &std::path::PathBuf| {
            let t = p.to_string_lossy();
            book_of(p).is_some()
                || t.ends_with("/metadata.json")
                || t.contains("/.tc/")
                || t.contains("/ingredients/tc/conflicts.json")
        };
        assert!(
            cost.written.iter().all(allowed),
            "{:?}",
            cost.written
                .iter()
                .filter(|p| !allowed(p))
                .collect::<Vec<_>>()
        );
        costs.push(cost);
    }
    // Ten times the project, the same kind of work: each file written is a rename of the
    // file and one of its heads file, and a group adds metadata.json, the manifest and the
    // intent, whatever the size of the project; and the heads file of metadata.json the
    // first time a session writes it (§3.6).
    for cost in &costs {
        let c = &cost.counters;
        assert_eq!(c.renames, 2 * c.derived_written + 4, "{c:?}");
        assert_eq!(c.files_created, c.renames, "{c:?}");
    }
}

/// B5: adding a book marks that book: one file projected, no other book's file read.
#[test]
fn a_book_change_marks_only_that_books_files() {
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        fs.advance(Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        handle
            .submit(Command::AddBook {
                book: BookId::ThirdJohn,
                scope: Scope::Whole,
                seed: SeedSource {
                    usfm:
                        "\\id 3JN source\n\\usfm 3.0\n\\h 3JN\n\\mt 3JN\n\\c 1\n\\p\n\\v 1 One.\n"
                            .into(),
                },
            })
            .wait()
            .unwrap();
        handle.wait_idle();
        fs.clear_log();
        let before = handle.counters();
        handle.leave(Leave::Book(BookId::ThirdJohn)).wait().unwrap();
        let cost = handle.counters().since(&before);
        assert!(
            (1..=2).contains(&cost.projected),
            "the new book's file, and conflicts.json: {cost:?}"
        );
        for path in fs.opened() {
            if let Some(book) = book_of(&path) {
                assert_eq!(book, "3JN", "{} is another book's", path.display());
            }
        }
        assert!(
            fs.peek(&corpus.folder.join("ingredients/3JN.usfm"))
                .is_some()
        );
        handle.close().unwrap();
        assert_valid_burrito(&fs, &corpus.folder);
    }
}

/// Each buffer is hashed once: a job that finds every file unchanged hashes what it rendered
/// and nothing else, reads no file back and writes none.
#[test]
fn an_unchanged_file_is_not_read_back_and_each_buffer_is_hashed_once() {
    let corpus = psalms();
    let fs = corpus.fs.copy();
    // Settle the entries: two opens, the clock past the time stamp's tick before each.
    for _ in 0..2 {
        fs.advance(Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        handle.close().unwrap();
    }
    fs.advance(Duration::from_secs(5));
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    let original = words_of(&handle, &psalm(23, 1));
    // A save and the save back: the book is marked, and its files come out as they were.
    save_psalm(&handle, 23, 1, "changed for a moment").unwrap();
    save_psalm(&handle, 23, 1, &original).unwrap();
    handle.wait_idle();
    fs.clear_log();
    let before = handle.counters();
    let sizes: std::collections::BTreeMap<std::path::PathBuf, u64> = fs
        .files_under(&corpus.folder.join("ingredients"))
        .into_iter()
        .map(|(p, b)| (p, b.len() as u64))
        .collect();
    handle.leave(Leave::Book(PSA)).wait().unwrap();
    let cost = handle.counters().since(&before);
    let before_sizes: u64 = fs.opened().iter().filter_map(|p| sizes.get(p)).sum();
    // The files the job found unchanged (settled, equal to their entries) were neither read
    // nor written: a file the second save changed (a decision's stamp) is the only one read.
    assert!(cost.projected > cost.derived_read, "{cost:?}");
    assert_eq!(cost.derived_read, cost.derived_written, "{cost:?}");
    // Hashed: each rendered buffer once, each file read for the comparison once, and
    // metadata.json's bytes; never a buffer twice.
    let snapshot = handle.snapshot();
    let rendered: u64 = snapshot
        .fold()
        .render_book(PSA)
        .iter()
        .map(|(_, b)| b.len() as u64)
        .sum::<u64>()
        + snapshot
            .fold()
            .render(&tc_journal::fold::DerivedFile::Conflicts)
            .map_or(0, |b| b.len() as u64);
    let metadata = fs.peek(&corpus.folder.join("metadata.json")).unwrap().len() as u64;
    assert!(cost.bytes_hashed >= rendered, "{cost:?}");
    assert!(
        cost.bytes_hashed <= rendered + before_sizes + 3 * metadata,
        "a buffer was hashed more than once: {} hashed, {rendered} rendered, {before_sizes} read, {cost:?}",
        cost.bytes_hashed
    );
    handle.close().unwrap();
}

fn collect_progress(handle: &ProjectHandle) -> Arc<Mutex<Vec<(tc_store::Progress, Instant)>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    let events = handle.subscribe();
    std::thread::spawn(move || {
        for event in events {
            if let StoreEvent::Progress {
                work: Work::Materialize,
                progress,
            } = event
            {
                into.lock().unwrap().push((progress, Instant::now()));
            }
        }
    });
    seen
}

/// A whole-project materialization is background work under PERFORMANCE.md rule 4: it
/// reports determinate progress from 300 ms on, saves go on meanwhile, and the folder is a
/// valid burrito after each group and at the end.
#[test]
fn a_whole_project_materialize_reports_progress_and_saves_go_on() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    for book in [BookId::Philemon, BookId::SecondJohn] {
        handle
            .submit(Command::AddBook {
                book,
                scope: Scope::Whole,
                seed: SeedSource {
                    usfm: format!(
                        "\\id {} source\n\\usfm 3.0\n\\h {}\n\\mt {}\n\\c 1\n\\p\n\\v 1 One.\n",
                        book.code(),
                        book.code(),
                        book.code()
                    ),
                },
            })
            .wait()
            .unwrap();
    }
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let seen = collect_progress(&handle);
    // A pin change rewrites every book's decisions and `resources.json`: a job of four
    // groups, each read slowly.
    rig.fs.set_read_delay(Some(Duration::from_millis(150)));
    let pins = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
    let shown = handle.mint(&pins, None).unwrap();
    let started = Instant::now();
    handle
        .submit(Command::SetPins {
            shown,
            value: lexicon_pin(),
        })
        .wait()
        .unwrap();
    let left = handle.leave(Leave::Mode);
    // Saves go on while the groups are written.
    let mut saved = 0;
    while !left.is_ready() && saved < 40 {
        let reply = save_verse(
            &handle,
            1,
            1 + (saved % 14) as u16,
            &format!("saved during the job {saved}"),
        )
        .unwrap();
        assert_eq!(reply.stored, tc_store::Stored::Durable);
        saved += 1;
    }
    left.wait().unwrap();
    let ended = Instant::now();
    rig.fs.set_read_delay(None);
    handle.wait_idle();
    let seen = seen.lock().unwrap().clone();
    assert!(!seen.is_empty(), "no progress was reported");
    let first = seen[0].1.duration_since(started);
    assert!(
        first >= Duration::from_millis(280) && first <= Duration::from_millis(700),
        "the first report came after {first:?}"
    );
    for pair in seen.windows(2) {
        assert!(pair[0].0.done <= pair[1].0.done, "counts only grow");
    }
    let last = seen.last().unwrap();
    assert_eq!(last.0.done, last.0.total, "the last report is complete");
    assert!(last.1 <= ended + Duration::from_millis(200));
    assert!(saved > 0, "the job lasted long enough for saves to go on");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

fn lexicon_pin() -> tc_journal::record::PinGroupValue {
    use std::collections::BTreeMap;
    use tc_journal::json::{Json, JsonMap};
    use tc_journal::keys::SlotName;
    use tc_journal::record::{PinGroupValue, SlotValue};
    PinGroupValue(BTreeMap::from([(
        SlotName::parse("ugl").unwrap(),
        SlotValue(Json::Obj(JsonMap::from([
            (
                "repoPath".to_owned(),
                Json::from("git.door43.org/unfoldingWord/en_ugl"),
            ),
            ("sha".to_owned(), Json::from("0".repeat(40))),
            ("version".to_owned(), Json::from("v1")),
        ]))),
    )]))
}

/// A one-book materialization is what an ordinary leave costs: it shows no indicator.
#[test]
fn a_one_book_materialize_shows_no_progress() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let seen = collect_progress(&handle);
    rig.fs.set_read_delay(Some(Duration::from_millis(120)));
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    let started = Instant::now();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    rig.fs.set_read_delay(None);
    assert!(
        started.elapsed() > Duration::from_millis(350),
        "the job was slow enough to show one"
    );
    handle.wait_idle();
    assert!(
        seen.lock().unwrap().is_empty(),
        "{:?}",
        seen.lock().unwrap()
    );
    handle.close().unwrap();
}

fn blocked_notices(handle: &ProjectHandle) -> Vec<(Vec<String>, Blocked)> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::MaterializeBlocked { paths, reason } => Some((
                paths.iter().map(|p| p.as_str().to_owned()).collect(),
                reason,
            )),
            _ => None,
        })
        .collect()
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// R16: a job that cannot write says why, once, in a typed notice; it goes when the path
/// is written.
#[test]
fn a_blocked_materialize_is_a_typed_notice() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let usfm = folder.join("ingredients/TIT.usfm");
    for (reason, hold) in [(Blocked::FileBusy, true), (Blocked::FileUnreadable, false)] {
        let set = |on: bool| {
            if hold {
                rig.fs.set_busy(&usfm, on);
            } else {
                rig.fs.set_unreadable(&usfm, on);
            }
        };
        set(true);
        save_verse(&handle, 1, 2, &format!("blocked {reason:?}")).unwrap();
        handle.leave(Leave::Book(TIT)).wait().unwrap();
        wait_for("a blocked notice", || {
            blocked_notices(&handle)
                .iter()
                .any(|(p, r)| p == &["ingredients/TIT.usfm"] && *r == reason)
        });
        let listed = blocked_notices(&handle);
        assert_eq!(listed.len(), 1, "listed once: {listed:?}");
        set(false);
        wait_for("the notice to go", || blocked_notices(&handle).is_empty());
        wait_for("the file to be written", || {
            rig.fs.peek(&usfm).is_some_and(|b| {
                String::from_utf8_lossy(&b).contains(&format!("blocked {reason:?}"))
            })
        });
    }
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// One file a scanner holds does not stall the other files of its book: the writer waits
/// 35 ms for it, writes the rest, and tries the held one again later.
#[test]
fn a_held_derived_file_does_not_stall_the_book() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let usfm = folder.join("ingredients/TIT.usfm");
    // A program holds TIT.usfm for a while.
    rig.fs.set_busy(&usfm, true);
    save_verse(&handle, 1, 3, "held for a while").unwrap();
    let before = handle.counters();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    let after = handle.counters().since(&before);
    // The held path waited three short tries (35 ms) and no more.
    assert!(after.held_wait_ns <= 3 * 35_000_000, "{after:?}");
    assert!(
        rig.fs.held_waits().iter().sum::<Duration>() <= Duration::from_millis(60),
        "{:?}",
        rig.fs.held_waits()
    );
    // metadata.json and the manifest were written all the same: the folder is valid.
    assert_valid_burrito(&rig.fs, &folder);
    rig.fs.set_busy(&usfm, false);
    wait_for("the held file to be written", || {
        rig.fs
            .peek(&usfm)
            .is_some_and(|b| String::from_utf8_lossy(&b).contains("held for a while"))
    });
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
