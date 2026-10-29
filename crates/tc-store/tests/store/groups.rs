//! Materialization book by book (`docs/STORAGE.md` §5.10, PERFORMANCE.md): what a change
//! marks, what a group touches, how often a buffer is hashed, the progress a long job shows,
//! and the typed notice of a job that cannot write (APP-DESIGN R16).

use crate::common::*;
use crate::corpus::{psalms, psalms_larger};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::{Blocked, Notice};
use tc_journal::record::Scope;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::fault::OpKind;
use tc_store::{CommandError, Leave, OpenOptions, ProjectHandle, StoreEvent, StoreReply, Work};

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
    let text = slashed(path).replace("%2F", "/");
    let name = text.rsplit('/').next()?;
    let stem = name.split('.').next()?;
    (stem.len() == 3
        && stem
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
    .then(|| stem.to_owned())
}

/// B5: adding a book marks that book: one file projected, no other book's file read.
#[test]
fn a_book_change_marks_only_that_books_files() {
    // Only the larger corpus has other books' files to leave alone.
    for corpus in [psalms_larger()] {
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

/// B5 and the writer: a verse save marks its book, and the files the writer opens and writes
/// are that book's, `metadata.json`, the manifest and the intent, on the corpus ten times
/// larger too. Unchanged ingredient projections are neither read nor replaced. A changed
/// ingredient gets M1 and final compare-and-swap reads; metadata gets the same two safety reads
/// before the group's replacement, even when its old size and mtime still match the ledger.
#[test]
fn an_unchanged_file_is_not_read_back_and_each_buffer_is_hashed_once() {
    let mut counts = Vec::new();
    for corpus in [psalms(), psalms_larger()] {
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
        // The text returns to its original bytes, while a decision's stamp changes.
        save_psalm(&handle, 23, 1, "changed for a moment").unwrap();
        save_psalm(&handle, 23, 1, &original).unwrap();
        handle.wait_idle();
        let preimages = fs.files_under(&corpus.folder.join("ingredients"));
        let metadata_path = corpus.folder.join("metadata.json");
        let old_metadata_size = fs.peek(&metadata_path).unwrap().len() as u64;
        fs.clear_log();
        let before = handle.counters();
        handle.leave(Leave::Book(PSA)).wait().unwrap();
        handle.wait_idle();
        let cost = handle.counters().since(&before);
        let opened = fs.opened();
        let snapshot = handle.snapshot();
        let rendered: std::collections::BTreeMap<_, _> = snapshot
            .fold()
            .render_book(PSA)
            .into_iter()
            .map(|(file, bytes)| (corpus.folder.join(file.path()), bytes))
            .collect();
        let changed: std::collections::BTreeSet<_> = rendered
            .iter()
            .filter(|(path, bytes)| preimages.get(*path) != Some(*bytes))
            .map(|(path, _)| path.clone())
            .collect();
        assert!(!changed.is_empty(), "the decision stamp must change");
        assert!(
            changed.len() < rendered.len(),
            "some projections must be unchanged"
        );
        let is_derived = |path: &std::path::PathBuf| {
            path.strip_prefix(&corpus.folder)
                .ok()
                .map(slashed)
                .as_deref()
                .and_then(tc_journal::fold::DerivedFile::from_path)
                .is_some()
        };
        let ingredient_reads: std::collections::BTreeSet<_> = opened
            .iter()
            .filter(|path| is_derived(path))
            .cloned()
            .collect();
        let written: Vec<_> = fs
            .log()
            .into_iter()
            .filter(|op| matches!(op.kind, OpKind::Rename | OpKind::RemoveFile))
            .map(|op| op.path)
            .collect();
        for path in opened.iter().chain(&written) {
            let text = slashed(path);
            assert!(
                !text.contains("/ingredients/tc/changes/") && !text.contains("/.tc/pending/"),
                "{text}: no journal file is touched"
            );
            if let Some(book) = book_of(path) {
                assert_eq!(book, "PSA", "{text} belongs to another book");
            }
        }
        let allowed = |p: &std::path::PathBuf| {
            let t = slashed(p);
            book_of(p).is_some()
                || t.ends_with("/metadata.json")
                || t.contains("/.tc/")
                || t.contains("/ingredients/tc/conflicts.json")
        };
        assert!(
            written.iter().all(allowed),
            "{:?}",
            written.iter().filter(|p| !allowed(p)).collect::<Vec<_>>()
        );
        // Each file written is a rename of the file and one of its heads file, and a group
        // adds metadata.json, the manifest and the intent, whatever the size of the project;
        // and the heads file of metadata.json the first time a session writes it (§3.6).
        assert_eq!(cost.renames, 2 * cost.derived_written + 4, "{cost:?}");
        assert_eq!(cost.files_created, cost.renames, "{cost:?}");
        let ingredient_writes: std::collections::BTreeSet<_> = fs
            .log()
            .into_iter()
            .filter(|op| op.kind == OpKind::Rename && is_derived(&op.path))
            .map(|op| op.path)
            .collect();
        // Expected changes come from the byte comparison, so reading and gratuitously
        // rewriting every unchanged file cannot satisfy this oracle.
        assert_eq!(
            ingredient_reads, changed,
            "an unchanged ingredient was read"
        );
        assert_eq!(
            ingredient_writes, changed,
            "an unchanged ingredient was replaced"
        );
        for path in &changed {
            assert_eq!(
                opened.iter().filter(|p| *p == path).count(),
                2,
                "{}: M1 and the final compare-and-swap, once each",
                path.display()
            );
        }
        let metadata_reads = opened.iter().filter(|p| **p == metadata_path).count() as u64;
        assert_eq!(
            metadata_reads, 2,
            "metadata must be read before planning and replacement"
        );
        assert_eq!(cost.derived_read, changed.len() as u64 + 1, "{cost:?}");
        assert_eq!(cost.derived_written, changed.len() as u64, "{cost:?}");
        assert_eq!(cost.projected, rendered.len() as u64, "{cost:?}");
        // Each rendered buffer is hashed once. Each ingredient read is hashed once, and
        // metadata adds its two old-byte reads and its one newly rendered buffer.
        let rendered_bytes: u64 = rendered.values().map(|bytes| bytes.len() as u64).sum();
        let ingredient_read_bytes: u64 = opened
            .iter()
            .filter(|path| is_derived(path))
            .map(|path| preimages[path].len() as u64)
            .sum();
        let metadata_read_bytes = metadata_reads * old_metadata_size;
        let new_metadata_size = fs.peek(&metadata_path).unwrap().len() as u64;
        assert_eq!(
            cost.bytes_hashed,
            rendered_bytes + ingredient_read_bytes + metadata_read_bytes + new_metadata_size,
            "each rendered buffer and each comparison read must be hashed once: {cost:?}"
        );
        counts.push([
            cost.projected,
            cost.derived_read,
            cost.derived_written,
            metadata_reads,
            cost.files_opened,
            cost.renames,
        ]);
        handle.close().unwrap();
    }
    assert_eq!(
        counts[0],
        counts[1],
        "{} times the project must not add operations",
        tc_corpus::Spec::test_factor()
    );
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
    rig.fs.set_read_delay(Some(Duration::from_millis(25)));
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
    rig.fs.set_read_delay(Some(Duration::from_millis(10)));
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

// ---------------------------------------------------------------------------------------
// §5.10, §6.2: a group that stopped after its rename
// ---------------------------------------------------------------------------------------

fn outside_edit_notices(handle: &ProjectHandle) -> Vec<(u32, bool)> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::OutsideEditRecorded {
                path,
                changes,
                unknown_base,
                ..
            } if path.as_str() == "ingredients/TIT.usfm" => Some((changes, unknown_base)),
            _ => None,
        })
        .collect()
}

fn verse_text_conflicts(handle: &ProjectHandle) -> usize {
    handle
        .snapshot()
        .fold()
        .conflicts()
        .filter(|c| {
            matches!(
                c.kind,
                tc_journal::fold::ConflictKind::VerseText
                    | tc_journal::fold::ConflictKind::VerseStructure
            )
        })
        .count()
}

/// A project with 1:1 saved twice, the second save not yet written to `TIT.usfm`, and the
/// number of the write point (counted from the next `leave`) right after `TIT.usfm` was
/// renamed into place.
fn fail_after_the_rename() -> (Rig, std::path::PathBuf, ProjectHandle, u64) {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    save_verse(&handle, 1, 1, "first save").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    // The same steps on a copy tell which write point that is.
    let probe = rig.fs.copy();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    let probed = probe_rig
        .store
        .open(&folder, &OpenOptions::default())
        .unwrap();
    probed.wait_idle();
    save_verse(&probed, 1, 1, "second save").unwrap();
    probe.clear_log();
    let start = probe.ops();
    probed.leave(Leave::Book(TIT)).wait().unwrap();
    let log = probe.log();
    let usfm = folder.join("ingredients/TIT.usfm");
    let renamed = log
        .iter()
        .position(|op| op.kind == OpKind::Rename && op.path == usfm)
        .expect("TIT.usfm is renamed into place");
    let k = log[renamed].index + 1 - start;
    drop(probed);
    drop(probe_rig);
    save_verse(&handle, 1, 1, "second save").unwrap();
    (rig, folder, handle, k)
}

/// The bytes of a group that stopped after its rename are the app's own (§6.2): the next group
/// does not take them for an outside edit, and a later save of the same verse is no conflict
/// with the one before it.
#[test]
fn a_group_that_fails_after_a_rename_does_not_mistake_its_bytes_for_an_outside_edit() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    let failed = handle.leave(Leave::Book(TIT)).wait();
    assert!(failed.is_err(), "the group stopped: {failed:?}");
    assert!(
        blocked_notices(&handle)
            .iter()
            .any(|(_, reason)| matches!(reason, Blocked::Failed { .. })),
        "the person is told derived files could not be written"
    );
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap())
            .contains("second save"),
        "the rename happened"
    );
    save_verse(&handle, 1, 1, "third save").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    assert_eq!(
        outside_edit_notices(&handle),
        Vec::<(u32, bool)>::new(),
        "the app's own bytes were taken for an outside edit"
    );
    assert_eq!(
        verse_text_conflicts(&handle),
        0,
        "a conflict with its own save"
    );
    assert_eq!(words_of(&handle, &verse(1, 1)), "third save");
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap())
            .contains("third save")
    );
    assert!(
        blocked_notices(&handle).is_empty(),
        "the notice goes when a job has written what was left: {:?}",
        blocked_notices(&handle)
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same stop, but a newer save is made and the window gets focus before the group is
/// tried again: a look at the file finds the app's own bytes, which are none an outside
/// program wrote.
#[test]
fn a_focus_after_a_group_stopped_does_not_take_its_bytes_for_an_outside_edit() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    save_verse(&handle, 1, 1, "third save").unwrap();
    let looked = handle.focus().wait().unwrap();
    assert!(looked.found.is_empty(), "{looked:?}");
    handle.wait_idle();
    assert_eq!(outside_edit_notices(&handle), Vec::<(u32, bool)>::new());
    assert_eq!(verse_text_conflicts(&handle), 0);
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    assert_eq!(words_of(&handle, &verse(1, 1)), "third save");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The writer's own retry may find the file already equal to the projection: the ledger
/// names those bytes, whose heads it wrote before the rename (§5.10 M3), so a later outside
/// edit of the verse the file shows has the right base (§11.1 X3), also after a restart, which
/// finds only what the folder holds.
#[test]
fn a_retry_that_finds_the_file_converged_writes_its_heads_file() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    // The retry timer converges the file without a new save: the manifest then names the bytes
    // that are in the file, and the heads file in the folder is the one they stand for.
    let usfm = folder.join("ingredients/TIT.usfm");
    let on_disk = tc_journal::Md5::of(&rig.fs.peek(&usfm).unwrap()).to_hex();
    wait_for("the retry to settle the file", || {
        rig.fs
            .peek(&folder.join(".tc/materialized.json"))
            .is_some_and(|m| String::from_utf8_lossy(&m).contains(&on_disk))
    });
    handle.close().unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    mend_outside(&rig, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    // The edit waits for the answer; *Import* compares it with that base.
    assert_eq!(
        counts(&import_waiting(&handle)),
        vec![(1, 0)],
        "recorded with the base the file shows"
    );
    assert_eq!(verse_text_conflicts(&handle), 0);
    assert_eq!(
        words_of(&handle, &verse(1, 1)),
        "second save, mended outside"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The external copies of `bytes` the project kept, as paths relative to the folder.
fn external_copies_of(rig: &Rig, folder: &Path, bytes: &[u8]) -> Vec<String> {
    rig.fs
        .files_under(&folder.join(".tc/external"))
        .into_iter()
        .filter(|(_, held)| held == bytes)
        .map(|(path, _)| path.display().to_string())
        .collect()
}

/// §5.10, §6.2 rule 9: the group that stopped after its rename knows the file now holds the
/// second save. The bytes of the save before it are no longer the app's own, so a program that
/// puts them back (a revert) has made an edit: kept whole in `.tc/external/` and held for the
/// person's answer, exactly as when no group had stopped.
#[test]
fn an_outside_revert_after_a_stopped_group_is_kept_and_asked() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    let before = rig.fs.peek(&usfm).unwrap();
    assert!(String::from_utf8_lossy(&before).contains("first save"));
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("second save"),
        "the rename happened"
    );
    rig.fs.put_unseen(&usfm, &before, None);
    let looked = handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_eq!(
        looked.found,
        vec!["ingredients/TIT.usfm".to_owned()],
        "the old bytes were taken for the app's own"
    );
    // Kept whole and held: the book waits for the answer, and nothing of it is taken in.
    assert_eq!(handle.waiting_books().len(), 1);
    assert_eq!(outside_edit_notices(&handle), Vec::<(u32, bool)>::new());
    assert_eq!(verse_text_conflicts(&handle), 0);
    assert_eq!(external_copies_of(&rig, &folder, &before).len(), 1);
    assert_eq!(rig.fs.peek(&usfm), Some(before.clone()), "not written over");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same revert met by the writer's own retry instead of a look at the folder: the retry
/// reads the old bytes at M1 and must not write the second save over them.
#[test]
fn a_retry_after_a_stopped_group_does_not_write_over_an_outside_revert() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    let before = rig.fs.peek(&usfm).unwrap();
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    rig.fs.put_unseen(&usfm, &before, None);
    // A save marks the book dirty and the next group starts at M0 and so at M1.
    save_verse(&handle, 1, 2, "a save after the revert").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    // Kept whole and held: the book waits for the answer, and nothing of it is taken in.
    assert_eq!(handle.waiting_books().len(), 1);
    assert_eq!(outside_edit_notices(&handle), Vec::<(u32, bool)>::new());
    assert_eq!(verse_text_conflicts(&handle), 0);
    assert_eq!(external_copies_of(&rig, &folder, &before).len(), 1);
    assert_eq!(rig.fs.peek(&usfm), Some(before.clone()), "not written over");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same for `metadata.json`, which a group renames last: when the group stops right after
/// its rename (it changed with a project detail saved in the same group), the bytes before are
/// no longer the app's own either.
#[test]
fn an_outside_revert_of_metadata_json_after_a_stopped_group_is_an_edit() {
    let (rig, folder, handle, _k) = fail_after_the_rename();
    let name = tc_journal::RecordKey::ProjectMeta(tc_journal::MetaField::Name);
    handle
        .submit(Command::SetMeta {
            shown: handle.mint(&name, None).unwrap(),
            value: Some(tc_journal::Json::Obj(tc_journal::JsonMap::from([(
                "en".to_owned(),
                tc_journal::Json::from("Titus, second name"),
            )]))),
        })
        .wait()
        .unwrap();
    let meta = folder.join("metadata.json");
    let before = rig.fs.peek(&meta).unwrap();
    rig.fs
        .fail_after_rename(&meta, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    let after = rig.fs.peek(&meta).unwrap();
    assert_ne!(before, after, "metadata.json was renamed into place");
    assert!(String::from_utf8_lossy(&after).contains("Titus, second name"));
    rig.fs.put_unseen(&meta, &before, None);
    // The look finds the old bytes.
    let _ = handle.focus().wait();
    let told = handle
        .notices()
        .into_iter()
        .filter(|n| matches!(n.notice, Notice::MetadataEditedOutside { .. }))
        .count();
    assert_eq!(told, 1, "the old bytes were taken for the app's own");
    assert_eq!(external_copies_of(&rig, &folder, &before).len(), 1);
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §5.10 M3: the heads of new bytes are written before the ledger names them and before their
/// rename. While the heads file of `metadata.json` cannot be written, `metadata.json` keeps the
/// bytes the ledger names; the retry once it can writes both.
#[test]
fn metadata_json_is_not_renamed_before_its_heads_are_written() {
    let (rig, folder, handle, _k) = fail_after_the_rename();
    let name = tc_journal::RecordKey::ProjectMeta(tc_journal::MetaField::Name);
    handle
        .submit(Command::SetMeta {
            shown: handle.mint(&name, None).unwrap(),
            value: Some(tc_journal::Json::Obj(tc_journal::JsonMap::from([(
                "en".to_owned(),
                tc_journal::Json::from("Titus, second name"),
            )]))),
        })
        .wait()
        .unwrap();
    let heads = folder.join(tc_store::layout::heads_file("metadata.json"));
    let meta = folder.join("metadata.json");
    let before = rig.fs.peek(&heads);
    let meta_before = rig.fs.peek(&meta);
    rig.fs.set_busy(&heads, true);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    assert_eq!(rig.fs.peek(&heads), before, "the write was refused");
    assert_eq!(
        rig.fs.peek(&meta),
        meta_before,
        "no rename without its heads"
    );
    rig.fs.set_busy(&heads, false);
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    assert_ne!(
        rig.fs.peek(&heads),
        before,
        "the heads file that goes with the new project details"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A group that skipped a file another program holds lists the bytes it wrote in the manifest:
/// the entry of `metadata.json` has the md5 of the file that is in the folder, so no look at
/// the folder takes it for an edit.
#[test]
fn the_manifest_names_the_metadata_json_a_group_that_skipped_a_file_wrote() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.set_busy(&usfm, true);
    save_verse(&handle, 1, 3, "held for a while").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    let manifest = rig.fs.peek(&folder.join(".tc/materialized.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let listed = manifest["paths"]["metadata.json"]["placed"]["md5"]
        .as_str()
        .unwrap();
    let real = tc_journal::Md5::of(&rig.fs.peek(&folder.join("metadata.json")).unwrap()).to_hex();
    assert_eq!(listed, real);
    let looked = handle.focus().wait().unwrap();
    assert!(looked.found.is_empty(), "{looked:?}");
    rig.fs.set_busy(&usfm, false);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// What a person does to the file a stopped group left: a program that has it open saves
/// the verse again.
fn mend_outside(rig: &Rig, folder: &Path) {
    let usfm = folder.join("ingredients/TIT.usfm");
    let edited = String::from_utf8(rig.fs.peek(&usfm).unwrap())
        .unwrap()
        .replace("second save", "second save, mended outside");
    rig.fs.put_unseen(&usfm, edited.as_bytes(), None);
}

/// §5.10, §11.1 D7: a group that stopped after its rename left the heads file describing the
/// bytes before it. The person's edit of the bytes it left, made before anything converged the
/// file, started from the app's save: the writer compares it with the heads the stopped group
/// stood for, so it continues that save and makes no conflict with it.
#[test]
fn an_outside_edit_of_the_bytes_a_stopped_group_left_continues_them() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.fail_op(k, std::io::ErrorKind::StorageFull);
    assert!(handle.leave(Leave::Book(TIT)).wait().is_err());
    mend_outside(&rig, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    // The edit waits for the answer; *Import* compares it with that base.
    assert_eq!(
        counts(&import_waiting(&handle)),
        vec![(1, 0)],
        "recorded with the base the file showed"
    );
    assert_eq!(
        verse_text_conflicts(&handle),
        0,
        "the edit conflicts with the save it started from"
    );
    assert_eq!(
        words_of(&handle, &verse(1, 1)),
        "second save, mended outside"
    );
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap())
            .contains("second save, mended outside")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §5.10: the process is killed between the rename of `TIT.usfm` and the write of its heads
/// file. The open after the restart finds the file equal to its projection while the heads
/// file still describes the bytes before, and writes the heads file again: an outside edit made
/// much later, in a session that knows nothing of the stop, continues the app's save.
#[test]
fn a_crash_after_a_rename_leaves_no_stale_heads_file_for_a_later_edit() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.kill_before(k, false);
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(rig.fs.is_dead(), "killed at the heads file");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap())
            .contains("second save"),
        "the rename happened"
    );
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    rig.fs.advance(Duration::from_secs(10));
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    mend_outside(&rig, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    // The edit waits for the answer; *Import* compares it with that base.
    assert_eq!(
        counts(&import_waiting(&handle)),
        vec![(1, 0)],
        "recorded with the base the file shows"
    );
    assert_eq!(verse_text_conflicts(&handle), 0);
    assert_eq!(
        words_of(&handle, &verse(1, 1)),
        "second save, mended outside"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §5.10 M3: the heads file of `TIT.usfm` cannot be written until after the close (a program
/// holds it), so the file is not written over meanwhile (no rename without its heads); the next
/// open writes both once it can, and an outside edit made in a later session continues the
/// app's save.
#[test]
fn a_close_while_the_heads_file_is_held_leaves_no_stale_heads_file_for_a_later_edit() {
    let (rig, folder, handle, _k) = fail_after_the_rename();
    let heads = folder.join(tc_store::layout::heads_file("ingredients/TIT.usfm"));
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.set_busy(&heads, true);
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(
        !String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("second save"),
        "renamed before its heads were written"
    );
    let _ = handle.close();
    rig.fs.set_busy(&heads, false);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    rig.fs.advance(Duration::from_secs(10));
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    mend_outside(&rig, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    // The edit waits for the answer; *Import* compares it with that base.
    assert_eq!(
        counts(&import_waiting(&handle)),
        vec![(1, 0)],
        "recorded with the base the file shows"
    );
    assert_eq!(verse_text_conflicts(&handle), 0);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same crash, and the heads file is held when the open after the restart would write it
/// again: the manifest keeps that it lags (the entry itself now names the bytes and the scope
/// of the file), so a later open writes it once it can, and a later outside edit continues the
/// app's save.
#[test]
fn a_heads_file_the_open_could_not_write_is_written_by_a_later_open() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    rig.fs.kill_before(k, false);
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(rig.fs.is_dead(), "killed at the heads file");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let heads = folder.join(tc_store::layout::heads_file("ingredients/TIT.usfm"));
    rig.fs.set_busy(&heads, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    rig.fs.advance(Duration::from_secs(10));
    let _ = handle.close();
    rig.fs.set_busy(&heads, false);
    // The entry settles at this open; the heads file is written at the first that can.
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    rig.fs.advance(Duration::from_secs(10));
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    mend_outside(&rig, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_eq!(counts(&import_waiting(&handle)), vec![(1, 0)]);
    assert_eq!(verse_text_conflicts(&handle), 0);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §5.10 M8: a group settles from the intent only the paths it wrote, found equal or removed.
/// The process is killed right after the rename of `TIT.usfm`, and the open cannot read the file
/// (a program holds it), so the open's first job leaves it; the person saves the verse again.
/// That job must keep the intent of `TIT.usfm`: the retry that reads the file later finds the
/// second save there while the manifest still names the first, and only the intent says it is
/// the app's own. Taken for an outside edit, it would be listed as a conflict with the third
/// save, between the person's own two edits.
#[test]
fn a_group_keeps_the_intent_of_a_path_it_could_not_read() {
    let (rig, folder, handle, k) = fail_after_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.kill_before(k, false);
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(rig.fs.is_dead(), "killed at the heads file");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    assert!(String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("second save"));
    rig.fs.set_unreadable(&usfm, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    // The open's first job meets the file it cannot read, and ends.
    handle.wait_idle();
    save_verse(&handle, 1, 1, "third save").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    rig.fs.set_unreadable(&usfm, false);
    wait_for("the retry to write the third save", || {
        String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("third save")
    });
    handle.wait_idle();
    assert_eq!(outside_edit_notices(&handle), Vec::<(u32, bool)>::new());
    assert_eq!(
        verse_text_conflicts(&handle),
        0,
        "a conflict with its own save"
    );
    assert_eq!(words_of(&handle, &verse(1, 1)), "third save");
    assert_eq!(
        rig.fs.files_under(&folder.join(".tc/external")).len(),
        0,
        "the app's own bytes were examined as an outside edit"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A project in an interrupted group whose book file another program holds: `TIT.usfm` was
/// renamed to the second save before the kill, `metadata.json` still lists the first, and the
/// file cannot be read, so nothing can tell which bytes are there (I7). The handle has saved a
/// third version of 1:1 and left Titus once.
fn held_after_a_kill_past_the_rename() -> (Rig, std::path::PathBuf, ProjectHandle) {
    let (rig, folder, handle, k) = fail_after_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.kill_before(k, false);
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(rig.fs.is_dead(), "killed at the heads file");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    assert!(
        !burrito_problems(&rig.fs, &folder).is_empty(),
        "the kill left metadata.json behind the rename (§6.4 M4)"
    );
    // The kill and the program that holds the file left the folder invalid; the tests below
    // check the folder themselves.
    damaged_on_purpose(&rig);
    rig.fs.set_unreadable(&usfm, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    save_verse(&handle, 1, 1, "third save").unwrap();
    let _ = handle.leave(Leave::Book(TIT)).wait();
    handle.wait_idle();
    (rig, folder, handle)
}

/// BURRITO-HOLD: a kill after the group wrote `metadata.json` (§6.4 M5) leaves a valid folder
/// whose ledger still says `Writing`; a checkpoint that cannot read the book file then must
/// not list the bytes before the write in place of the ones on disk.
#[test]
fn a_checkpoint_that_cannot_read_a_book_file_keeps_a_valid_folder_valid() {
    let (rig, folder, handle, _) = fail_after_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.kill_after_rename(&folder.join("metadata.json"));
    let _ = handle.leave(Leave::Book(TIT)).wait();
    assert!(rig.fs.is_dead(), "killed after metadata.json");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    assert_valid_burrito(&rig.fs, &folder);
    rig.fs.set_unreadable(&usfm, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    save_verse(&handle, 1, 1, "third save").unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait()
        .unwrap();
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);
    rig.fs.set_unreadable(&usfm, false);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// BURRITO-HOLD (1): a commit made while a book file's bytes are undecided is a valid Burrito
/// that holds the project's newest text, not the folder's (§9.4).
#[test]
fn a_checkpoint_while_a_book_file_is_undecided_commits_a_valid_burrito() {
    let (rig, folder, handle) = held_after_a_kill_past_the_rename();
    let report = handle
        .checkpoint(tc_store::CheckpointReason::BeforeExport)
        .wait();
    assert!(report.unwrap().commit.is_some(), "the checkpoint commits");
    let repo = repo_of(&rig.fs, &folder);
    let head = commits_of(&repo)[0];
    assert_eq!(commit_problems(&repo, head), Vec::<String>::new());
    let tit = commit_files(&repo, head)["ingredients/TIT.usfm"].clone();
    assert!(String::from_utf8_lossy(&tit).contains("third save"));
    rig.fs
        .set_unreadable(&folder.join("ingredients/TIT.usfm"), false);
    handle.close().unwrap();
}

/// BURRITO-HOLD (2): the first checkpoint after the file can be read again makes the folder
/// a valid Burrito.
#[test]
fn a_checkpoint_after_the_held_book_file_reads_again_heals_the_folder() {
    let (rig, folder, handle) = held_after_a_kill_past_the_rename();
    rig.fs
        .set_unreadable(&folder.join("ingredients/TIT.usfm"), false);
    let report = handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait();
    assert!(report.unwrap().commit.is_some(), "the checkpoint commits");
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);
    assert_eq!(words_of(&handle, &verse(1, 1)), "third save");
    handle.close().unwrap();
}

/// BURRITO-HOLD (2): so does the first open after it.
#[test]
fn an_open_after_the_held_book_file_reads_again_heals_the_folder() {
    let (rig, folder, handle) = held_after_a_kill_past_the_rename();
    let usfm = folder.join("ingredients/TIT.usfm");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    rig.fs.set_unreadable(&usfm, false);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);
    assert!(String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("third save"));
    handle.close().unwrap();
}
