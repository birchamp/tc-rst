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
use tc_journal::keys::ToolId;
use tc_journal::pins::Collection;
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
    STATE.get_or_init(|| fold_corpus(&Spec::scaled(1, 10)))
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
    eprintln!("{action}, ten times larger: {big:?}");
    for (name, s, b) in rows {
        assert!(
            b <= s,
            "{action}: {name} grows with the project: {s} on Psalms, {b} on the corpus ten times larger"
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
fn the_larger_corpus_is_ten_times_larger() {
    let small = psalms();
    let big = larger();
    assert!(
        big.len() >= 9 * small.len(),
        "{} vs {}",
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
    for (corpus, state) in [("Psalms", psalms()), ("ten times larger", larger())] {
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

// ---------------------------------------------------------------------------------------
// Unit P: the editor's other interactive actions (docs/PERFORMANCE.md "Editor")
// ---------------------------------------------------------------------------------------

/// What the text of Psalms holds: its header, chapter front and verse records, and the values
/// those records' heads hold (a conflict has several).
fn psalms_text(state: &FoldState) -> (u64, u64) {
    let keys: Vec<RecordKey> = state
        .record_keys()
        .into_iter()
        .filter(|k| match k {
            RecordKey::Verse(b, ..) | RecordKey::ChapterFront(b, _) | RecordKey::BookHeader(b) => {
                *b == PSALMS
            }
            _ => false,
        })
        .collect();
    let values = keys.iter().map(|k| state.heads(k).len() as u64).sum();
    (keys.len() as u64, values)
}

/// Opening a book renders that book's text and nothing else. The two corpora hold Psalms
/// with different histories (a few records have two heads in one and not in the other), so
/// the count is held to what the book itself holds: one projection of each of its header,
/// chapter front and verse records, one value decoded and read for each head, and no
/// listing of the project's other records.
#[test]
fn opening_a_book_costs_that_book() {
    for (corpus, state) in [("Psalms", psalms()), ("ten times larger", larger())] {
        let state = fork(state);
        let (records, values) = psalms_text(&state);
        let before = state.counters();
        let usfm = state.book_usfm(PSALMS).expect("Psalms projects");
        assert!(usfm.contains("\\c 150"));
        let counts = state.counters().since(&before);
        eprintln!("open a book, {corpus}: {counts:?} ({records} records, {values} values)");
        assert!(
            counts.records_projected <= records && counts.records_projected > 2_000,
            "{corpus}: opening Psalms projected {} records; it holds {records}",
            counts.records_projected
        );
        assert!(
            counts.values_decoded <= values && counts.values_read <= values,
            "{corpus}: opening Psalms decoded {} and read {} values; it holds {values}",
            counts.values_decoded,
            counts.values_read
        );
        assert_eq!(counts.changes_indexed, 0, "{corpus}: {counts:?}");
        assert_eq!(counts.keys_recomputed, 0, "{corpus}: {counts:?}");
        assert_eq!(counts.conflicts_built, 0, "{corpus}: {counts:?}");
        assert_eq!(counts.books_checked, 0, "{corpus}: {counts:?}");
    }
}

/// Moving to a chapter reads that chapter's keys and projects its records: the same work on
/// the corpus ten times larger, and exactly the chapter's verse and front records.
#[test]
fn moving_to_a_chapter_costs_that_chapter() {
    let measure = |state: &FoldState| {
        let state = fork(state);
        let chapter = common_chapter();
        let before = state.counters();
        let keys = state.chapter_keys(PSALMS, chapter);
        let usfm = state.chapter_usfm(PSALMS, chapter).expect("it projects");
        assert!(usfm.contains(&format!("\\c {chapter}")));
        let counts = state.counters().since(&before);
        let records = keys.verses.len() + usize::from(keys.front.is_some());
        assert_eq!(
            counts.records_projected, records as u64,
            "a chapter projects its {records} verse and front records: {counts:?}"
        );
        counts
    };
    assert_no_growth("move to a chapter", measure(psalms()), measure(larger()));
}

/// The project-wide conflict counts a save leaves are kept by the step itself: reading them
/// builds no conflict, whatever the project holds. Psalms with two thousand more conflicts (2,000
/// decisions on which two devices disagree) saves a verse and reads the counts with the same work as the plain Psalms corpus and the one ten times larger, and
/// the counts differ before and after only by what the save changed (nothing: the verse was
/// not in a conflict).
#[test]
fn the_conflict_counts_after_a_save_cost_nothing_per_conflict() {
    let many = {
        let mut state = psalms().clone();
        let decisions: Vec<RecordKey> = state
            .record_keys()
            .into_iter()
            .filter(|k| matches!(k, RecordKey::Check(c) if c.book() == Some(PSALMS)))
            .filter(|k| state.heads(k).len() == 1)
            .take(2_000)
            .collect();
        assert_eq!(
            decisions.len(),
            2_000,
            "Psalms has enough decisions to disagree on"
        );
        let (mut maria, mut juan) = (Dev::new(210), Dev::new(211));
        let mut changes = Vec::new();
        for (i, key) in decisions.iter().enumerate() {
            let Some(Record::Check {
                key: check,
                value: CheckValue::Decision(decision),
            }) = state.projected(key)
            else {
                continue;
            };
            let parents = state.shown_heads(key, None);
            for (dev, name) in [(&mut maria, "Maria"), (&mut juan, "Juan")] {
                let mut said = decision.clone();
                said.comment = Some(format!("{name} {i}"));
                let record = Record::Check {
                    key: check.clone(),
                    value: CheckValue::Decision(said),
                };
                changes.extend(dev.batch(vec![(record, parents.clone())], Cause::Edit));
            }
        }
        state.add(changes.into_iter().map(FoldInput::from));
        state
    };
    assert!(
        many.conflict_count() >= psalms().conflict_count() + 1_900,
        "{} conflicts against {}",
        many.conflict_count(),
        psalms().conflict_count()
    );
    let measure = |label: &str, state: &FoldState| {
        let mut state = fork(state);
        let (key, record) = text_verse(&state);
        eprintln!("{label}: saving {key:?}");
        let counts_before = state.conflict_counts().clone();
        let parents = state.shown_heads(&key, None);
        let mut dev = Dev::new(212);
        let change = dev.batch(
            vec![(reword(&record, "a saved verse"), parents)],
            Cause::Edit,
        );
        let before = state.counters();
        state.add(change.into_iter().map(FoldInput::from));
        let counts = state.conflict_counts().clone();
        let work = state.counters().since(&before);
        assert_eq!(work.conflicts_built, 0, "{work:?}");
        assert_eq!(
            counts, counts_before,
            "a save outside a conflict changes no count"
        );
        work
    };
    let (small, big, crowded) = (
        measure("Psalms", psalms()),
        measure("larger", larger()),
        measure("crowded", &many),
    );
    assert_no_growth("conflict counts after a save, ten times larger", small, big);
    assert_eq!(
        crowded, small,
        "two thousand conflicts changed what a save and its counts cost"
    );
}

/// A tool's decisions of a book read that tool's decision records and the verse texts they
/// are assessed against, and no other record of the book. The Check screen opens with them,
/// and the book also holds its verses, alignments, comments and the other tool's decisions:
/// reading all of them to find the Check records made the open cost three times what it
/// needs (`docs/PERFORMANCE.md` rule 2, "Checking").
#[test]
fn a_tools_decisions_read_only_its_own_records() {
    for (name, state) in [("Psalms", psalms()), ("larger", larger())] {
        let fold = fork(state);
        let own = fold.decisions_of(ToolId::TranslationNotes, PSALMS).len();
        let other = fold.decisions_of(ToolId::TranslationWords, PSALMS).len();
        let verses: usize = fold
            .chapters_of(PSALMS)
            .iter()
            .map(|&c| fold.chapter_keys(PSALMS, c).verses.len())
            .sum();
        assert!(own > 0 && other > 0 && verses > 0, "{name}");
        let before = fold.counters();
        let decisions = fold.decisions(ToolId::TranslationNotes, Collection::Book(PSALMS));
        let read = fold.counters().since(&before);
        assert_eq!(decisions.len(), own, "{name}: every decision of the tool");
        // Its decisions and the verses of the book (and a front for a chapter), once each.
        let bound = (own + verses + 150 + 8) as u64;
        assert!(
            read.records_projected <= bound,
            "{name}: {} records projected for {own} decisions and {verses} verses",
            read.records_projected
        );
        assert!(read.values_decoded <= bound, "{name}: {read:?}");
        // The other tool's decisions are not among them.
        assert!(
            read.records_projected < (own + other + verses) as u64,
            "{name}: the other tool's {other} decisions were read as well"
        );
    }
}

/// The resource of each decision comes from what the fold keeps beside the change: the same
/// resources, in the same order, as the decisions themselves carry, with nothing decoded for
/// the decisions that have a single head. An upgrade counts the decisions it would put at risk
/// by them.
#[test]
fn decision_resources_are_read_without_decoding_decisions() {
    for (name, state) in [("Psalms", psalms()), ("larger", larger())] {
        for tool in [ToolId::TranslationNotes, ToolId::TranslationWords] {
            let fold = fork(state);
            let collection = Collection::Book(PSALMS);
            let before = fold.counters();
            let resources = fold.decision_resources(tool, collection);
            let read = fold.counters().since(&before);
            let decisions = fold.decisions(tool, collection);
            assert_eq!(resources.len(), decisions.len(), "{name} {tool:?}");
            for (resource, decision) in resources.iter().zip(&decisions) {
                assert_eq!(**resource, decision.record.resource, "{name} {tool:?}");
            }
            assert!(
                read.values_decoded <= (decisions.len() / 100) as u64,
                "{name} {tool:?}: {} values decoded for {} decisions",
                read.values_decoded,
                decisions.len()
            );
        }
    }
}

/// The history axis alone: the same save on the same records, in a project that has had
/// three more years of work, 4,000 more changes in Psalms and 500 of them on the saved verse
/// itself (a chain of 500 ancestors). The ten-times corpus mixes history with books and
/// devices; a corpus with a third of the history is a different set of records (fewer decisions
/// are made), so it would not isolate the axis.
#[test]
fn a_verse_save_costs_nothing_per_year_of_history() {
    let grown = {
        let mut state = psalms().clone();
        let (key, _) = text_verse(&state);
        let mut dev = Dev::new(213);
        for i in 0..500 {
            let record = state.projected(&key).expect("the verse");
            let parents = state.shown_heads(&key, None);
            let change = dev.batch(
                vec![(reword(&record, &format!("an earlier save {i}")), parents)],
                Cause::Edit,
            );
            state.add(change.into_iter().map(FoldInput::from));
        }
        let others: Vec<RecordKey> = state
            .record_keys()
            .into_iter()
            .filter(|k| matches!(k, RecordKey::Verse(b, c, _) if *b == PSALMS && c.get() != common_chapter()))
            .filter(|k| matches!(state.projected(k), Some(Record::Verse { value: VerseValue::Text { .. }, .. })))
            .take(3_500)
            .collect();
        assert!(others.len() > 2_000);
        for (i, other) in others.iter().chain(others.iter()).enumerate() {
            let record = state.projected(other).expect("a verse");
            let parents = state.shown_heads(other, None);
            let change = dev.batch(
                vec![(reword(&record, &format!("history {i}")), parents)],
                Cause::Edit,
            );
            state.add(change.into_iter().map(FoldInput::from));
        }
        state
    };
    assert!(grown.len() >= psalms().len() + 4_000);
    assert_no_growth(
        "verse save, 4,000 more changes of history",
        verse_save(psalms()),
        verse_save(&grown),
    );
}
