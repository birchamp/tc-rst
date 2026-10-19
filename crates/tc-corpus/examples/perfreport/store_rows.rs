//! The store's rows end to end on the whole-Bible corpus: setup, the opens, the editor's
//! actions through the public API of `tc-store`, materialize, checkpoint and close.
//! Every case is a function that runs in a child process of its own and returns its rows.

use crate::common::{
    STORE_THREADS, align_one, alignments_of, cost, decide_one, decisions_of, ms, open,
    project_folder, run_id, save_one, store, verses_across, verses_of,
};
use crate::machine;
use crate::table::Measured;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::command::Command;
use tc_journal::fold::{FoldInput, FoldState};
use tc_journal::format::FormatDoc;
use tc_journal::record::VerseValue;
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, DeviceId, DeviceSeq, Record, RecordKey,
    WallTime,
};
use tc_store::layout::{Folder, GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{CheckpointReason, Leave, OpenOptions, Progress, ProgressSink, StdFs, SyncDir};

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
    std::fs::write(path, bytes).expect("a file");
}

/// Writes the whole-Bible project through the store's own open, seal, consolidation,
/// materialization and first checkpoint, then lets the repack pack the loose objects of that
/// checkpoint, so the cases below start from a project as a team's is after its first year.
pub fn setup(dir: &Path, spec: &Spec, name: &str) {
    let started = Instant::now();
    let corpus = Corpus::generate(spec).expect("the corpus builds");
    eprintln!(
        "generated {} changes in {:?}",
        corpus.journal.stats.changes,
        started.elapsed()
    );
    let store = store(dir);
    let root = dir.join("projects").join(name);
    let f = Folder::new(&root);
    let journal = &corpus.journal;
    let project = serde_json::json!({
        "schemaVersion": 1,
        "projectId": journal.project.to_string(),
        "kind": "bible",
        "created": "2026-10-17T00:00:00.000Z",
    });
    put(
        &f.path("ingredients/tc/project.json"),
        project.to_string().as_bytes(),
    );
    put(
        &f.path("ingredients/tc/format.json"),
        &FormatDoc::current().to_bytes(),
    );
    put(&f.path(".gitignore"), GITIGNORE_BYTES);
    put(&f.path(".gitattributes"), GITATTRIBUTES_BYTES);
    for pack in &journal.packs {
        put(&f.path(&pack.path), &pack.bytes);
    }
    for pending in &journal.pending {
        put(
            &f.path(&format!(".tc/pending/{}", pending.name)),
            &pending.bytes,
        );
    }
    let canonical = std::fs::canonicalize(&root).expect("a canonical path");
    let print = tc_store::device::fingerprint(
        store.app_data().secret(),
        &tc_store::Platform::machine_id(&tc_store::StdPlatform),
        &canonical.to_string_lossy(),
    );
    let device = serde_json::json!({
        "device": journal.devices[0].to_string(),
        "fingerprint": print.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "created": "2026-10-17T00:00:00.000Z",
    });
    put(&f.path(".tc/device.json"), device.to_string().as_bytes());
    let changes = corpus.journal.stats.changes;
    drop(corpus);
    let started = Instant::now();
    let handle = store.open(&root, &OpenOptions::default()).expect("open");
    eprintln!(
        "first open (every file projected and written): {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    handle
        .checkpoint(CheckpointReason::AfterImport)
        .wait()
        .expect("checkpoint");
    eprintln!(
        "checkpoint (seal, consolidation, materialization): {:?}",
        started.elapsed()
    );
    handle.wait_idle();
    drain_repacks(&handle, &root);
    handle.close().expect("close");
    let problems = tc_burrito::validate_dir(&root);
    eprintln!("{changes} changes; burrito problems: {}", problems.len());
    assert!(problems.is_empty(), "{problems:?}");
    drop(store);
    // The repack of the setup's checkpoint is one run per open (at most 96 MiB each); the
    // later opens finish what is left.
    for round in 0..40 {
        let (loose, bytes) = crate::common::loose_objects(&root);
        if loose < tc_store::REPACK_OBJECTS && bytes < tc_store::REPACK_BYTES {
            break;
        }
        eprintln!("repack round {round}: {loose} loose objects, {bytes} bytes");
        let store = crate::common::store_with(dir, Arc::new(tc_store::SystemClock));
        let handle = store.open(&root, &OpenOptions::default()).expect("open");
        drain_repacks(&handle, &root);
        handle.close().expect("close");
    }
    let (loose, bytes) = crate::common::loose_objects(&root);
    eprintln!(
        "setup done: {loose} loose objects ({bytes} bytes), .git {} MiB",
        crate::common::dir_size(&root.join(".git")) >> 20
    );
}

/// Waits until the loose objects stop disappearing: the repack the open or the checkpoint
/// started has ended (a closing project stops one that runs, so setup waits for it).
fn drain_repacks(handle: &tc_store::ProjectHandle, root: &Path) {
    let mut last = crate::common::loose_objects(root);
    let mut still = 0;
    let started = Instant::now();
    while still < 6 && started.elapsed() < Duration::from_secs(600) {
        std::thread::sleep(Duration::from_millis(500));
        let now = crate::common::loose_objects(root);
        if now == last && handle.counters().git_repacks > 0 || now.0 == 0 {
            still += 1;
        } else {
            still = 0;
        }
        last = now;
    }
}

struct FirstProgress {
    first: Arc<Mutex<Option<f64>>>,
    sink: ProgressSink,
}

fn first_progress() -> FirstProgress {
    let first: Arc<Mutex<Option<f64>>> = Arc::default();
    let into = first.clone();
    let sink: ProgressSink = Arc::new(move |p: Progress| {
        let mut slot = into.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(p.at.as_secs_f64() * 1000.0);
        }
    });
    FirstProgress { first, sink }
}

/// One open in a process of its own (`mode`: `cache`, `nocache`, `cold`).
pub fn open_once(dir: &Path, mode: &str) -> Vec<Measured> {
    let root = project_folder(dir);
    let index = root.join(".tc/index.bin");
    match mode {
        "nocache" => {
            let _ = std::fs::remove_file(&index);
        }
        "cold" => {
            let n = crate::common::evict_page_cache(dir);
            if n == 0 {
                println!("cold: the page cache cannot be evicted here");
                return Vec::new();
            }
        }
        _ => {}
    }
    let store = store(dir);
    let progress = first_progress();
    let started = Instant::now();
    let handle = store
        .open(
            &root,
            &OpenOptions {
                progress: progress.sink.clone(),
                ..OpenOptions::default()
            },
        )
        .expect("open");
    let usable = ms(started);
    let peak = machine::status_mib("VmHWM:").unwrap_or(f64::NAN);
    let counters = handle.counters();
    handle.wait_idle();
    let background = ms(started);
    let after = handle.counters();
    let steady = machine::status_mib("VmRSS:").unwrap_or(f64::NAN);
    let note = format!(
        "background done {background:.0} ms, peak {peak:.0} MiB, steady {steady:.0} MiB, files \
         opened {} ({} after the background work), journal files {} ({} cached), bytes read {} \
         ({}), projected {}",
        counters.files_opened,
        after.files_opened,
        counters.journal_files_read,
        counters.journal_files_cached,
        counters.bytes_read,
        after.bytes_read,
        counters.projected,
    );
    let id = match mode {
        "nocache" => "open.nocache",
        "cold" => "open.cold",
        _ => "open.cache",
    };
    let mut rows = vec![Measured::new(id, vec![usable], note)];
    let first = *progress.first.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(first) = first {
        rows.push(Measured::new(
            "open.progress",
            vec![first],
            format!("the {mode} open"),
        ));
    }
    handle.close().expect("close");
    rows
}

struct Dev {
    device: DeviceId,
    seq: u64,
    at: i64,
}

impl Dev {
    fn new() -> Self {
        let mut raw = [0u8; 15];
        raw[..8].copy_from_slice(&(run_id() as u64).to_be_bytes());
        raw[14] = 0xbe;
        Self {
            device: DeviceId::local(raw),
            seq: 0,
            at: 1_900_000_000_000,
        }
    }

    fn write(
        &mut self,
        record: Record,
        parents: Vec<tc_journal::ChangeId>,
        cause: Cause,
    ) -> Change {
        self.seq += 1;
        self.at += 1_000;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&self.seq.to_be_bytes());
        id[8..].copy_from_slice(&run_id().to_be_bytes()[8..]);
        let mut parents = parents;
        parents.sort();
        Change::new(ChangeBody {
            v: tc_journal::FORMAT_MINOR,
            record,
            parents,
            device: self.device,
            seq: DeviceSeq(self.seq),
            at: WallTime(self.at),
            batch: BatchRef::new(BatchId::from_bytes(id), 1).expect("a batch"),
            cause,
        })
        .expect("a change")
    }
}

fn reworded(fold: &FoldState, key: &RecordKey, words: String) -> Option<Record> {
    let Some(Record::Verse {
        book,
        chapter,
        verse,
        value:
            VerseValue::Text {
                marker,
                layout,
                last,
                from,
                ..
            },
    }) = fold.projected(key)
    else {
        return None;
    };
    Some(Record::Verse {
        book,
        chapter,
        verse,
        value: VerseValue::Text {
            marker,
            words,
            layout,
            last,
            from,
        },
    })
}

/// The editor's rows and the three saves, with the whole Bible open.
pub fn editor(dir: &Path) -> Vec<Measured> {
    crate::common::header("editor");
    let store = store(dir);
    let handle = open(&store, dir);
    let mut rows = Vec::new();
    let run = run_id();

    // Open a book: the published snapshot's projection of Psalms.
    let mut times = Vec::new();
    let mut usfm_len = 0;
    for _ in 0..20 {
        let started = Instant::now();
        let read = handle.read_book(BookId::Psalms).wait();
        times.push(ms(started));
        usfm_len = read.usfm.as_ref().map_or(0, String::len);
    }
    rows.push(Measured::new(
        "book.open",
        times,
        format!("Psalms USFM {usfm_len} bytes"),
    ));

    // The first chapter, and a move to any chapter, on a fork that counts its own work.
    let snapshot = handle.snapshot();
    let mut times = Vec::new();
    let mut counts = tc_journal::fold::Counters::default();
    for _ in 0..20 {
        let mut fork = snapshot.fold().clone();
        fork.own_counters();
        let started = Instant::now();
        let keys = fork.chapter_keys(BookId::Psalms, 1);
        let usfm = fork.chapter_usfm(BookId::Psalms, 1);
        times.push(ms(started));
        assert!(usfm.is_some() && !keys.verses.is_empty());
        counts = fork.counters();
    }
    rows.push(Measured::new(
        "book.first_chapter",
        times,
        format!("{counts:?}"),
    ));
    let mut times = Vec::new();
    let mut fork = snapshot.fold().clone();
    fork.own_counters();
    for i in 0..200usize {
        let chapter = ((i * 37) % 150 + 1) as u16;
        let before = fork.counters();
        let started = Instant::now();
        let keys = fork.chapter_keys(BookId::Psalms, chapter);
        let usfm = fork.chapter_usfm(BookId::Psalms, chapter);
        times.push(ms(started));
        assert!(usfm.is_some() && !keys.verses.is_empty());
        counts = fork.counters().since(&before);
    }
    rows.push(Measured::new("chapter.move", times, format!("{counts:?}")));

    // The fold's own step after a save, with a published snapshot alive (the actor publishes
    // a clone after every batch): add one change, publish, project the chapter.
    let verses = verses_across(&handle, BookId::Psalms, 200);
    let mut fold = snapshot.fold().clone();
    fold.own_counters();
    let mut dev = Dev::new();
    let (mut steps, mut counts_ms) = (Vec::new(), Vec::new());
    let mut step_counts = tc_journal::fold::Counters::default();
    let mut published: Vec<FoldState> = Vec::new();
    for (i, key) in verses.iter().enumerate() {
        let Some(record) = reworded(&fold, key, format!("A fold step number {i} of run {run}"))
        else {
            continue;
        };
        let chapter = match key {
            RecordKey::Verse(_, c, _) => c.get(),
            _ => continue,
        };
        let change = dev.write(record, fold.shown_heads(key, None), Cause::Edit);
        let before = fold.counters();
        let started = Instant::now();
        fold.add([FoldInput::from(change)]);
        published.push(fold.clone());
        let usfm = fold.chapter_usfm(BookId::Psalms, chapter);
        steps.push(ms(started));
        assert!(usfm.is_some());
        step_counts = fold.counters().since(&before);
        let started = Instant::now();
        let _ = fold.conflict_counts().len();
        counts_ms.push(ms(started));
        if published.len() > 2 {
            published.remove(0);
        }
    }
    rows.push(Measured::new(
        "save.fold_step",
        steps,
        format!("{step_counts:?}"),
    ));
    rows.push(Measured::new("counts.incremental", counts_ms, ""));

    // The conflict list from scratch.
    let mut times = Vec::new();
    let mut conflicts = 0;
    for _ in 0..20 {
        let started = Instant::now();
        let (list, counts) = fold.conflicts_rebuilt();
        times.push(ms(started));
        conflicts = list.len();
        std::hint::black_box(counts);
    }
    rows.push(Measured::new(
        "conflicts.scratch",
        times,
        format!("{conflicts} conflicts"),
    ));
    drop(published);

    // The three saves until their durable reply.
    let mut verse_ms = Vec::new();
    let before = handle.counters();
    for (i, key) in verses.iter().enumerate() {
        verse_ms.push(save_one(
            &handle,
            key,
            format!("A saved verse number {i} of run {run}"),
        ));
    }
    let delta = handle.counters().since(&before);
    rows.push(
        Measured::new(
            "save.verse",
            verse_ms,
            format!("per save: {}", per(&delta, verses.len() as u64)),
        )
        .with_fsyncs(delta.fsyncs as f64 / verses.len().max(1) as f64),
    );
    let decisions = decisions_of(&handle, BookId::Psalms, 200);
    let mut decision_ms = Vec::new();
    let before = handle.counters();
    for key in &decisions {
        decision_ms.push(decide_one(&handle, key));
    }
    let delta = handle.counters().since(&before);
    rows.push(
        Measured::new(
            "save.decision",
            decision_ms,
            format!("per save: {}", per(&delta, decisions.len() as u64)),
        )
        .with_fsyncs(delta.fsyncs as f64 / decisions.len().max(1) as f64),
    );
    let alignments = alignments_of(&handle, BookId::Psalms, 200);
    let mut align_ms = Vec::new();
    let before = handle.counters();
    for key in &alignments {
        align_ms.push(align_one(&handle, key));
    }
    let delta = handle.counters().since(&before);
    rows.push(
        Measured::new(
            "save.alignment",
            align_ms,
            format!("per save: {}", per(&delta, alignments.len() as u64)),
        )
        .with_fsyncs(delta.fsyncs as f64 / alignments.len().max(1) as f64),
    );
    handle.close().expect("close");
    rows
}

fn per(delta: &tc_store::StoreCounters, n: u64) -> String {
    let n = n.max(1);
    format!(
        "opened {:.2}, created {:.2}, written {} B, fsyncs {:.2}",
        delta.files_opened as f64 / n as f64,
        delta.files_created as f64 / n as f64,
        delta.bytes_written / n,
        delta.fsyncs as f64 / n as f64
    )
}

/// Verses of `book` whose alignment still links words to the original: the first edit of such
/// a verse reflows its alignment (R25), so the batch changes the alignment file as well as
/// the text.
fn linked_verses(handle: &tc_store::ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let chapters = fold.chapters_of(book);
    let mut out = Vec::new();
    let stride = (chapters.len() / n).max(1);
    for chapter in chapters.iter().step_by(stride) {
        for key in fold.chapter_keys(book, *chapter).alignments {
            let Some(Record::Alignment {
                book,
                chapter,
                verse,
                value,
            }) = fold.projected(&key)
            else {
                continue;
            };
            if value.alignments.iter().any(|g| !g.bottom_words.is_empty()) {
                out.push(RecordKey::Verse(book, chapter, verse));
                break;
            }
        }
        if out.len() >= n {
            break;
        }
    }
    out
}

/// "Materialize one book": the time from leaving the book to the writer being done, after the
/// first edit of an aligned verse (text and alignment files), a second edit of it (text
/// only), an alignment save, a decision, and an edit in Jude.
pub fn materialize(dir: &Path) -> Vec<Measured> {
    crate::common::header("materialize");
    let store = store(dir);
    let handle = open(&store, dir);
    let run = run_id();
    let leave = |book: BookId| {
        let before = handle.counters();
        let started = Instant::now();
        handle.leave(Leave::Book(book)).wait().expect("leave");
        (ms(started), handle.counters().since(&before))
    };
    let mut rows = Vec::new();
    let verses = linked_verses(&handle, BookId::Psalms, 20);
    let (mut first, mut second) = (Vec::new(), Vec::new());
    let (mut first_cost, mut second_cost) = (
        tc_store::StoreCounters::default(),
        tc_store::StoreCounters::default(),
    );
    for (i, key) in verses.iter().enumerate() {
        save_one(&handle, key, format!("A first edit {i} of run {run}"));
        let (took, cost) = leave(BookId::Psalms);
        first.push(took);
        first_cost = cost;
        save_one(&handle, key, format!("A second edit {i} of run {run}"));
        let (took, cost) = leave(BookId::Psalms);
        second.push(took);
        second_cost = cost;
    }
    rows.push(
        Measured::new(
            "materialize.book_first_edit",
            first,
            format!(
                "{} aligned verses; the last: {}",
                verses.len(),
                cost(&first_cost)
            ),
        )
        .with_fsyncs(first_cost.fsyncs as f64),
    );
    rows.push(
        Measured::new(
            "materialize.book",
            second,
            format!("the last: {}", cost(&second_cost)),
        )
        .with_fsyncs(second_cost.fsyncs as f64),
    );
    let alignments = alignments_of(&handle, BookId::Psalms, 20);
    let (mut times, mut last) = (Vec::new(), tc_store::StoreCounters::default());
    for key in &alignments {
        align_one(&handle, key);
        let (took, cost) = leave(BookId::Psalms);
        times.push(took);
        last = cost;
    }
    rows.push(
        Measured::new(
            "materialize.book_alignment",
            times,
            format!("the last: {}", cost(&last)),
        )
        .with_fsyncs(last.fsyncs as f64),
    );
    let decisions = decisions_of(&handle, BookId::Psalms, 20);
    let (mut times, mut last) = (Vec::new(), tc_store::StoreCounters::default());
    for key in &decisions {
        decide_one(&handle, key);
        let (took, cost) = leave(BookId::Psalms);
        times.push(took);
        last = cost;
    }
    rows.push(
        Measured::new(
            "materialize.book_decision",
            times,
            format!("the last: {}", cost(&last)),
        )
        .with_fsyncs(last.fsyncs as f64),
    );
    let jude = verses_of(&handle, BookId::Jude, 20);
    let (mut times, mut last) = (Vec::new(), tc_store::StoreCounters::default());
    for (i, key) in jude.iter().enumerate() {
        save_one(&handle, key, format!("A Jude edit {i} of run {run}"));
        let (took, cost) = leave(BookId::Jude);
        times.push(took);
        last = cost;
    }
    rows.push(
        Measured::new(
            "materialize.jude",
            times,
            format!("the last: {}", cost(&last)),
        )
        .with_fsyncs(last.fsyncs as f64),
    );
    handle.wait_idle();
    handle.close().expect("close");
    rows
}

/// A whole-project materialize (a pins change), five times, without saves meanwhile.
pub fn whole(dir: &Path) -> Vec<Measured> {
    crate::common::header("whole");
    let store = store(dir);
    let handle = open(&store, dir);
    let run = run_id();
    let events = crate::r9::watch_progress(&handle);
    let (mut total, mut first) = (Vec::new(), Vec::new());
    let mut last = tc_store::StoreCounters::default();
    let mut n_events = 0;
    for n in 0..5u32 {
        events.clear();
        let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
        let shown = handle.mint(&key, None).expect("a token");
        handle
            .submit(Command::SetPins {
                shown,
                value: crate::r9::pins_value(run, n),
            })
            .wait()
            .expect("pins");
        let before = handle.counters();
        let started = Instant::now();
        handle.leave(Leave::Mode).wait().expect("leave");
        total.push(ms(started));
        handle.wait_idle();
        last = handle.counters().since(&before);
        let seen = events.take();
        n_events = seen.len();
        if let Some(at) = seen.first() {
            first.push(at.duration_since(started).as_secs_f64() * 1000.0);
        }
    }
    handle.close().expect("close");
    vec![
        Measured::new(
            "materialize.whole",
            total,
            format!("{n_events} progress events; {}", cost(&last)),
        )
        .with_fsyncs(last.fsyncs as f64),
        Measured::new(
            "materialize.whole_progress",
            first,
            "from the leave command",
        ),
    ]
}

/// "Checkpoint": after one Psalms save, and after the materialization of that save ran.
pub fn checkpoint(dir: &Path) -> Vec<Measured> {
    crate::common::header("checkpoint");
    let store = store(dir);
    let handle = open(&store, dir);
    let verses = verses_of(&handle, BookId::Psalms, 40);
    let run = run_id();
    // The first checkpoint commits whatever an earlier job changed.
    save_one(
        &handle,
        &verses[0],
        format!("Checkpoint first save of run {run}"),
    );
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .expect("checkpoint");
    handle.wait_idle();
    let repacks_before = handle.counters().git_repacks;
    let (mut dirty, mut settled) = (Vec::new(), Vec::new());
    let (mut last_dirty, mut last_settled) = (
        tc_store::StoreCounters::default(),
        tc_store::StoreCounters::default(),
    );
    for i in 0..20usize {
        save_one(
            &handle,
            &verses[i % verses.len()],
            format!("Checkpoint save {i} of run {run}"),
        );
        let before = handle.counters();
        let started = Instant::now();
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        dirty.push(ms(started));
        last_dirty = handle.counters().since(&before);
        assert!(report.commit.is_some(), "{report:?}");
        handle.wait_idle();
    }
    for i in 0..20usize {
        save_one(
            &handle,
            &verses[(i + 7) % verses.len()],
            format!("Materialized checkpoint save {i} of run {run}"),
        );
        handle
            .leave(Leave::Book(BookId::Psalms))
            .wait()
            .expect("leave");
        let before = handle.counters();
        let started = Instant::now();
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        settled.push(ms(started));
        last_settled = handle.counters().since(&before);
        assert!(report.commit.is_some(), "{report:?}");
        handle.wait_idle();
    }
    let repacks = handle.counters().git_repacks - repacks_before;
    handle.close().expect("close");
    vec![
        Measured::new(
            "checkpoint.dirty",
            dirty,
            format!(
                "the last: {}; repacks meanwhile {repacks}",
                cost(&last_dirty)
            ),
        )
        .with_fsyncs(last_dirty.fsyncs as f64),
        Measured::new(
            "checkpoint.settled",
            settled,
            format!("the last: {}", cost(&last_settled)),
        )
        .with_fsyncs(last_settled.fsyncs as f64),
    ]
}

/// "Close a project" after a Psalms save and a decision, five times.
pub fn close(dir: &Path) -> Vec<Measured> {
    crate::common::header("close");
    let store = store(dir);
    let run = run_id();
    let mut closes = Vec::new();
    let mut last = tc_store::StoreCounters::default();
    let mut before = tc_store::StoreCounters::default();
    for i in 0..5usize {
        let handle = open(&store, dir);
        let verses = verses_of(&handle, BookId::Psalms, 10);
        let decisions = decisions_of(&handle, BookId::Psalms, 10);
        save_one(
            &handle,
            &verses[i % verses.len()],
            format!("Closed after {i} of run {run}"),
        );
        decide_one(&handle, &decisions[i % decisions.len()]);
        before = handle.counters();
        let started = Instant::now();
        let (result, counters) = handle.close_counted();
        closes.push(ms(started));
        result.expect("close");
        last = counters.since(&before);
    }
    let _ = before;
    vec![
        Measured::new(
            "close",
            closes,
            format!("threads {STORE_THREADS}; the last: {}", cost(&last)),
        )
        .with_fsyncs(last.fsyncs as f64),
    ]
}

/// One 1 KiB atomic write with its folder flush, 200 times: this drive's flush, the figure the
/// laptop's own run will divide.
pub fn flush(dir: &Path) -> Vec<Measured> {
    crate::common::header("flush");
    let target = dir.join("flush");
    std::fs::create_dir_all(&target).expect("a folder");
    let bytes = vec![0x5a; 1024];
    let mut times = Vec::new();
    for i in 0..200 {
        let started = Instant::now();
        tc_store::atomic_write(
            &StdFs,
            &target,
            &format!("f{}.bin", i % 20),
            &bytes,
            SyncDir::Now,
        )
        .expect("a write");
        times.push(ms(started));
    }
    vec![Measured::new("flush", times, "1 KiB, two flushes")]
}
