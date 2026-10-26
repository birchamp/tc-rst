//! Operation counts (`docs/PERFORMANCE.md` rule 2): an interactive step costs what it
//! touches. Each step runs on the Psalms corpus and on the corpus ten times larger (more
//! books, more history, more devices), on a Psalms chapter whose records are alike in both,
//! and fails if any of the fold's work counters is larger on the larger corpus.
//!
//! These counters exist because the other reading of the spec has been measured: tC4
//! re-folds its whole history at every save, at 23.81 microseconds an event, linear with no
//! plateau, so a save at whole-Bible scale costs about 6 s
//! (`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md` F3). tC4 does it for a sound reason (its fold
//! is defined as a pure function of the event set), and a fold that is correct the same way
//! here would be just as slow unless a test pins what each step may touch.

// Helpers build fixed, known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::OnceLock;
use support::Dev;
use tc_corpus::{Corpus, Spec};
use tc_journal::command::Choice;
use tc_journal::fold::{Counters, FoldInput, FoldState, Placement, ProjectKind, Resolved};
use tc_journal::record::{CheckValue, VerseValue};
use tc_journal::{BookId, Cause, ChangeId, Record, RecordKey};

const PSALMS: BookId = BookId::Psalms;

fn fold_corpus(spec: &Spec) -> FoldState {
    let corpus = Corpus::generate(spec).expect("the corpus builds");
    let lines = corpus.journal.lines();
    FoldState::fold(
        ProjectKind::Bible,
        lines.into_iter().map(|l| FoldInput::from(l).prepare()),
    )
}

fn psalms() -> &'static FoldState {
    static STATE: OnceLock<FoldState> = OnceLock::new();
    STATE.get_or_init(|| fold_corpus(&Spec::psalms(1)))
}

fn larger() -> &'static FoldState {
    static STATE: OnceLock<FoldState> = OnceLock::new();
    STATE.get_or_init(|| fold_corpus(&Spec::scaled(1, Spec::test_factor())))
}

/// A snapshot of `state` that counts its own work.
fn fork(state: &FoldState) -> FoldState {
    let mut fork = state.clone();
    fork.own_counters();
    fork
}

/// What a step touches in a chapter: its verses and its attached records by key, and
/// whether any of them is in a conflict.
fn chapter_records(state: &FoldState, chapter: u16) -> (Vec<RecordKey>, bool) {
    let keys: Vec<RecordKey> = state
        .record_keys()
        .into_iter()
        .filter(|k| match k {
            RecordKey::Verse(b, c, _) | RecordKey::Alignment(b, c, _) => {
                *b == PSALMS && c.get() == chapter
            }
            RecordKey::ChapterFront(b, c) => *b == PSALMS && c.get() == chapter,
            _ => false,
        })
        .collect();
    let conflicted = keys.iter().any(|k| {
        state
            .key_state(k)
            .is_some_and(|s| matches!(s.state, Resolved::Conflict(_)) || s.heads.len() > 1)
    });
    (keys, conflicted)
}

/// The first Psalms chapter whose verses and alignments are the same records in both
/// corpora, with no conflict and no comment on it: a step there touches the same amount
/// of work in both, so only the project's size differs.
fn common_chapter() -> u16 {
    static CHAPTER: OnceLock<u16> = OnceLock::new();
    *CHAPTER.get_or_init(|| {
        let comments_in = |state: &FoldState, chapter: u16| {
            state.record_keys().iter().any(|k| {
                matches!(k, RecordKey::Comment(_))
                    && matches!(
                        state.placement(k),
                        Placement::Run { chapter: c, .. } if c == chapter
                    )
            })
        };
        (1..=150u16)
            .find(|&c| {
                let (small, small_conflict) = chapter_records(psalms(), c);
                let (big, big_conflict) = chapter_records(larger(), c);
                small == big
                    && !small_conflict
                    && !big_conflict
                    && !comments_in(psalms(), c)
                    && !comments_in(larger(), c)
            })
            .expect("a Psalms chapter alike in both corpora")
    })
}

/// The first verse of the common chapter with a text value.
fn text_verse(state: &FoldState) -> (RecordKey, Record) {
    let chapter = common_chapter();
    chapter_records(state, chapter)
        .0
        .into_iter()
        .filter(|k| matches!(k, RecordKey::Verse(..)))
        .find_map(|k| match state.projected(&k) {
            Some(
                record @ Record::Verse {
                    value: VerseValue::Text { .. },
                    ..
                },
            ) => Some((k, record)),
            _ => None,
        })
        .expect("a verse with text")
}

fn reword(record: &Record, words: &str) -> Record {
    let mut record = record.clone();
    if let Record::Verse {
        value: VerseValue::Text { words: w, .. },
        ..
    } = &mut record
    {
        *w = words.into();
    }
    record
}

/// Adds `changes` to `state` and returns the work the step counted.
fn step(state: &mut FoldState, changes: Vec<tc_journal::Change>) -> Counters {
    let before = state.counters();
    state.add(changes.into_iter().map(FoldInput::from));
    state.counters().since(&before)
}

/// Fails when a count of the larger corpus is above the Psalms corpus's.
fn assert_no_growth(action: &str, small: Counters, big: Counters) {
    let rows = [
        (
            "changes indexed",
            small.changes_indexed,
            big.changes_indexed,
        ),
        ("keys updated", small.keys_updated, big.keys_updated),
        (
            "keys recomputed",
            small.keys_recomputed,
            big.keys_recomputed,
        ),
        (
            "changes visited",
            small.changes_visited,
            big.changes_visited,
        ),
        ("values decoded", small.values_decoded, big.values_decoded),
        ("groups built", small.groups_built, big.groups_built),
        ("books checked", small.books_checked, big.books_checked),
        (
            "chapters cohered",
            small.chapters_cohered,
            big.chapters_cohered,
        ),
        ("verses cohered", small.verses_cohered, big.verses_cohered),
        (
            "attached placed",
            small.attached_placed,
            big.attached_placed,
        ),
        (
            "conflicts built",
            small.conflicts_built,
            big.conflicts_built,
        ),
        (
            "records projected",
            small.records_projected,
            big.records_projected,
        ),
        ("values read", small.values_read, big.values_read),
        (
            "anomalies touched",
            small.anomalies_touched,
            big.anomalies_touched,
        ),
    ];
    eprintln!("{action}, Psalms: {small:?}");
    eprintln!("{action}, {} times larger: {big:?}", Spec::test_factor());
    for (name, s, b) in rows {
        assert!(
            b <= s,
            "{action}: {name} grows with the project: {s} on Psalms, {b} on the corpus {} times larger",
            Spec::test_factor()
        );
    }
}

/// A sanity bound: the step did some work, and no more than a chapter's worth.
fn assert_local(action: &str, counts: Counters, changes: u64) {
    assert_eq!(counts.changes_indexed, changes, "{action}");
    assert!(counts.books_checked <= 1, "{action}: {counts:?}");
    assert!(counts.chapters_cohered <= 1, "{action}: {counts:?}");
    assert!(
        counts.keys_updated + counts.keys_recomputed >= 1,
        "{action}: {counts:?}"
    );
}

#[test]
fn the_larger_corpus_is_the_test_factor_times_larger() {
    let small = psalms();
    let big = larger();
    let factor = Spec::test_factor();
    assert!(
        big.len() >= (factor - 1) * small.len(),
        "TC_SCALE_FACTOR={factor}: {} vs {}",
        big.len(),
        small.len()
    );
    assert!(big.books().len() > small.books().len());
}

#[test]
fn a_verse_save_and_its_chapter_cost_what_they_touch() {
    assert_no_growth("verse save", verse_save(psalms()), verse_save(larger()));
}

/// A verse save on `state`, counted with its chapter's USFM.
fn verse_save(state: &FoldState) -> Counters {
    let mut state = fork(state);
    let (key, record) = text_verse(&state);
    let mut dev = Dev::new(201);
    let parents = state.shown_heads(&key, None);
    let change = dev.batch(
        vec![(reword(&record, "a saved verse"), parents)],
        Cause::Edit,
    );
    let mut counts = step(&mut state, change);
    assert_local("verse save", counts, 1);
    let before = state.counters();
    let usfm = state
        .chapter_usfm(PSALMS, common_chapter())
        .expect("the chapter projects");
    assert!(usfm.contains("a saved verse"));
    let projection = state.counters().since(&before);
    counts.records_projected += projection.records_projected;
    counts.values_decoded += projection.values_decoded;
    counts
}

/// A step never reads the list of anomalies: a project with thousands of them (a lost or
/// damaged pack leaves many missing parents) saves a verse with the same work.
#[test]
fn a_verse_save_costs_nothing_per_anomaly() {
    let mut damaged = psalms().clone();
    let mut dev = Dev::new(209);
    let orphans: Vec<tc_journal::Change> = (0..2_000u32)
        .flat_map(|i| {
            let mut lost = [0u8; 32];
            lost[..4].copy_from_slice(&i.to_be_bytes());
            let record = Record::Verse {
                book: BookId::Genesis,
                chapter: tc_journal::ChapterNo::new(1 + (i / 50) as u16).expect("chapter"),
                verse: tc_journal::VerseNo::new(1 + (i % 50) as u16).expect("verse"),
                value: VerseValue::Text {
                    marker: tc_journal::record::MarkerWs::canonical(),
                    words: format!("orphan {i}"),
                    layout: "\n".into(),
                    last: tc_journal::VerseNo::new(1 + (i % 50) as u16).expect("verse"),
                    from: None,
                },
            };
            dev.batch(
                vec![(record, vec![ChangeId::from_bytes(lost)])],
                Cause::Edit,
            )
        })
        .collect();
    damaged.add(orphans.into_iter().map(FoldInput::from));
    assert!(damaged.anomalies().count() >= 2_000 + psalms().anomalies().count());
    assert_no_growth("verse save", verse_save(psalms()), verse_save(&damaged));
}

#[test]
fn a_decision_costs_what_it_touches() {
    let measure = |state: &FoldState| {
        let mut state = fork(state);
        let chapter = common_chapter().to_string();
        // A decision both corpora hold, on a check of the common chapter if one has one.
        let key = state
            .record_keys()
            .into_iter()
            .filter(|k| matches!(k, RecordKey::Check(c) if c.book() == Some(PSALMS)))
            .filter(|k| larger().key_state(k).is_some() && psalms().key_state(k).is_some())
            .filter(|k| psalms().heads(k).len() == 1 && larger().heads(k).len() == 1)
            .min_by_key(|k| match k {
                RecordKey::Check(c) => !matches!(
                    &c.target,
                    tc_journal::CheckTarget::Verse { chapter: ch, .. } if ch.as_str() == chapter
                ),
                _ => true,
            })
            .expect("a decision in both corpora");
        let Some(Record::Check {
            key: check,
            value: CheckValue::Decision(mut decision),
        }) = state.projected(&key)
        else {
            panic!("{key:?} projects a decision");
        };
        decision.bookmarked = !decision.bookmarked;
        let mut dev = Dev::new(202);
        let parents = state.shown_heads(&key, None);
        let record = Record::Check {
            key: check,
            value: CheckValue::Decision(decision),
        };
        let counts = step(&mut state, dev.batch(vec![(record, parents)], Cause::Edit));
        assert_local("decision", counts, 1);
        counts
    };
    assert_no_growth("decision", measure(psalms()), measure(larger()));
}

#[test]
fn an_alignment_costs_what_it_touches() {
    let measure = |state: &FoldState| {
        let mut state = fork(state);
        let key = chapter_records(&state, common_chapter())
            .0
            .into_iter()
            .find(|k| matches!(k, RecordKey::Alignment(..)))
            .expect("an alignment in the common chapter");
        let Some(Record::Alignment {
            book,
            chapter,
            verse,
            mut value,
        }) = state.projected(&key)
        else {
            panic!("{key:?} projects an alignment");
        };
        value.done = !value.done;
        let mut dev = Dev::new(203);
        let parents = state.shown_heads(&key, None);
        let record = Record::Alignment {
            book,
            chapter,
            verse,
            value,
        };
        let counts = step(&mut state, dev.batch(vec![(record, parents)], Cause::Edit));
        assert_local("alignment", counts, 1);
        counts
    };
    assert_no_growth("alignment", measure(psalms()), measure(larger()));
}

#[test]
fn a_resolution_costs_what_it_touches() {
    let measure = |state: &FoldState| {
        let mut state = fork(state);
        let (key, record) = text_verse(&state);
        let parents = state.shown_heads(&key, None);
        let mut maria = Dev::new(204);
        let mut juan = Dev::new(205);
        let a = maria.batch(
            vec![(reword(&record, "Maria's words"), parents.clone())],
            Cause::Edit,
        );
        let b = juan.batch(
            vec![(reword(&record, "Juan's words"), parents)],
            Cause::Edit,
        );
        let juans: ChangeId = b[0].id();
        state.add(a.into_iter().chain(b).map(FoldInput::from));
        let conflict = state
            .conflicts()
            .find(|c| c.key == key)
            .cloned()
            .expect("the two edits conflict");
        let planned = state
            .resolution(&conflict.id, &Choice::Candidate { head: juans })
            .expect("a candidate resolves the conflict");
        let resolving: Vec<tc_journal::Change> = planned
            .into_iter()
            .flat_map(|p| maria.batch(vec![(p.record, p.parents)], Cause::Resolve))
            .collect();
        let n = resolving.len() as u64;
        let counts = step(&mut state, resolving);
        assert_local("resolution", counts, n);
        assert!(state.conflicts().all(|c| c.key != key), "resolved");
        counts
    };
    assert_no_growth("resolution", measure(psalms()), measure(larger()));
}

#[test]
fn a_book_renders_as_its_files_do() {
    let state = psalms();
    let rendered = state.render_book(PSALMS);
    assert!(rendered.len() >= 4, "USFM, alignments, decisions, comments");
    for (file, bytes) in &rendered {
        assert_eq!(state.render(file).as_ref(), Some(bytes), "{}", file.path());
    }
}

/// Materializing a book lists the book's files from its own records, never from the rest of
/// the project: on the corpus ten times larger, the listing visits no more than the records
/// that belong to Psalms (and the comments, which are placed by their target).
#[test]
fn a_book_lists_its_files_from_its_own_records() {
    let larger_name = format!("{} times larger", Spec::test_factor());
    for (corpus, state) in [("Psalms", psalms()), (larger_name.as_str(), larger())] {
        let state = fork(state);
        let before = state.counters();
        let files = state.render_book(PSALMS);
        let counts = state.counters().since(&before);
        assert!(files.len() >= 4, "{corpus}: {files:?}");
        let book_records = state
            .record_keys()
            .iter()
            .filter(|k| k.book() == Some(PSALMS) || matches!(k, RecordKey::Comment(_)))
            .count() as u64;
        assert!(
            counts.records_listed > 0 && counts.records_listed <= book_records,
            "{corpus}: listing Psalms' files visited {} records; Psalms holds {book_records}",
            counts.records_listed
        );
    }
}
