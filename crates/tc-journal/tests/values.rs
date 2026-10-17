//! Values read on demand (§14): a fold that keeps only where each change's bytes lie, and
//! reads them from its source when it merges, lists a conflict or projects, is the fold of
//! the same changes kept in memory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::pack::read_pack;
use tc_journal::pending::read_pending;

/// The files of a corpus, read again by span, as `tc-store` reads packs on disk.
#[derive(Debug)]
struct Files {
    files: Vec<Vec<u8>>,
    /// Files whose bytes changed after they were read: one byte of each value read from
    /// them is flipped, at this share of the value's length.
    changed: Mutex<Vec<(u32, f64)>>,
}

impl ValueSource for Files {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        let file = self.files.get(at.file as usize)?;
        let start = usize::try_from(at.offset).ok()?;
        let mut bytes = file.get(start..start + at.len as usize)?.to_vec();
        let changed = self.changed.lock().ok()?;
        if let Some((_, share)) = changed.iter().find(|(f, _)| *f == at.file) {
            let i = ((bytes.len() as f64 * share) as usize).min(bytes.len().saturating_sub(1));
            if let Some(byte) = bytes.get_mut(i) {
                *byte ^= 1;
            }
        }
        Some(bytes)
    }
}

fn corpus() -> Corpus {
    Corpus::generate(&Spec::psalms(8).with_history(60, 0.1)).expect("the corpus builds")
}

/// The corpus's files read the way an open reads them: in memory, or by span.
fn inputs(corpus: &Corpus, located: bool) -> (Vec<FoldInput>, Files) {
    let mut files = Vec::new();
    let mut inputs = Vec::new();
    let project = corpus.journal.project;
    let reads = corpus
        .journal
        .packs
        .iter()
        .map(|p| {
            let read = read_pack(&p.path, &p.bytes, project);
            (p.bytes.clone(), Some(read.id), read.entries, read.spans)
        })
        .chain(corpus.journal.pending.iter().map(|p| {
            let read = read_pending(&p.bytes, project);
            (p.bytes.clone(), None, read.entries, read.spans)
        }));
    for (bytes, pack, entries, spans) in reads {
        let file = files.len() as u32;
        files.push(bytes);
        for (entry, span) in entries.into_iter().zip(spans) {
            let input = FoldInput::new(entry, pack);
            inputs.push(if located {
                input.prepare_at(ValueAt::new(file, span))
            } else {
                input.prepare()
            });
        }
    }
    let files = Files {
        files,
        changed: Mutex::new(Vec::new()),
    };
    (inputs, files)
}

#[test]
fn a_fold_that_reads_values_on_demand_is_the_fold_in_memory() {
    let corpus = corpus();
    let (resident, _) = inputs(&corpus, false);
    let (located, files) = inputs(&corpus, true);
    let memory = FoldState::fold(ProjectKind::Bible, resident);
    let source: Arc<dyn ValueSource> = Arc::new(files);
    let on_demand = FoldState::fold_from(ProjectKind::Bible, Some(source), located);
    assert!(memory.conflict_count() > 0, "the history has conflicts");
    assert_eq!(on_demand.frontier(), memory.frontier());
    assert_eq!(
        on_demand.conflicts().collect::<Vec<_>>(),
        memory.conflicts().collect::<Vec<_>>()
    );
    assert_eq!(
        on_demand.anomalies().collect::<Vec<_>>(),
        memory.anomalies().collect::<Vec<_>>()
    );
    assert_eq!(memory.counters().values_read, 0);
    let read_while_folding = on_demand.counters().values_read;
    assert!(read_while_folding > 0, "merges and candidates read values");
    for file in memory.derived_files() {
        assert_eq!(
            on_demand.render(&file),
            memory.render(&file),
            "{}",
            file.path()
        );
    }
    assert!(on_demand.counters().values_read > read_while_folding);
}

#[test]
fn bytes_that_changed_on_disk_never_pass_for_the_change() {
    let corpus = corpus();
    let (located, files) = inputs(&corpus, true);
    let files = Arc::new(files);
    let state = FoldState::fold_from(
        ProjectKind::Bible,
        Some(files.clone() as Arc<dyn ValueSource>),
        located,
    );
    let key = state
        .record_keys()
        .into_iter()
        .find(|k| matches!(k, tc_journal::RecordKey::Verse(..)))
        .expect("a verse");
    let head = state.heads(&key)[0];
    let raw = state.raw(&head).expect("the value reads");
    assert_eq!(tc_journal::ChangeId::of(&raw), head);
    let file = (0..files.files.len() as u32)
        .find(|f| {
            let bytes = &files.files[*f as usize];
            bytes.windows(raw.len()).any(|w| w == &raw[..])
        })
        .expect("a file holds it");
    for share in [0.0, 0.37, 0.5, 1.0] {
        *files.changed.lock().unwrap() = vec![(file, share)];
        assert_eq!(
            state.raw(&head),
            None,
            "a byte flipped at {share} of the value"
        );
        assert_eq!(state.body(&head), None);
    }
    files.changed.lock().unwrap().clear();
    assert_eq!(state.raw(&head).as_deref(), Some(&raw[..]));
}

// ---------------------------------------------------------------------------------------
// Values the source cannot read (§2.6, L98)
// ---------------------------------------------------------------------------------------

/// Each change in a file of its own, read again on demand; a blocked file cannot be read
/// (a file locked by another program, a cloud placeholder not yet downloaded).
#[derive(Debug, Default)]
struct Flaky {
    files: Vec<Vec<u8>>,
    blocked: Mutex<BTreeSet<u32>>,
}

impl ValueSource for Flaky {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        if self.blocked.lock().ok()?.contains(&at.file) {
            return None;
        }
        self.files.get(at.file as usize).cloned()
    }
}

/// Folds `changes` with every value read on demand from `source`, the changes in
/// `blocked` unreadable.
fn fold_flaky(
    changes: &[tc_journal::Change],
    blocked: &[tc_journal::ChangeId],
) -> (FoldState, Arc<Flaky>) {
    let mut flaky = Flaky::default();
    let mut inputs = Vec::new();
    for change in changes {
        let file = flaky.files.len() as u32;
        flaky.files.push(change.raw().to_vec());
        if blocked.contains(&change.id()) {
            flaky.blocked.lock().unwrap().insert(file);
        }
        let at = ValueAt {
            offset: 0,
            file,
            len: change.raw().len() as u32,
        };
        inputs.push(FoldInput::from(change.clone()).prepare_at(at));
    }
    let flaky = Arc::new(flaky);
    let source: Arc<dyn ValueSource> = flaky.clone();
    (
        FoldState::fold_from(ProjectKind::Bible, Some(source), inputs),
        flaky,
    )
}

/// Maria's Titus, and Juan's and Ana's edits of verse 1 from it, equal or not.
fn two_edits(
    equal: bool,
) -> (
    Vec<tc_journal::Change>,
    tc_journal::Change,
    tc_journal::Change,
) {
    use support::*;
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[2]);
    let base = on(&book, &verse_key(1, 1)).id();
    let juans = juan.one(verse(1, 1, text("Juan's words", 1)), vec![base]);
    let anas = ana.one(
        verse(
            1,
            1,
            if equal {
                text("Juan's words", 1)
            } else {
                text_layout("words of 1:1", "\n\\p\n", 1)
            },
        ),
        vec![base],
    );
    let mut all = book;
    all.extend([juans.clone(), anas.clone()]);
    (all, juans, anas)
}

/// L98: two different versions that cannot be read are listed as a conflict, never taken
/// as equal and settled; when they can be read, the record is judged again.
#[test]
fn unreadable_versions_are_listed_never_settled() {
    use support::*;
    let (all, juans, anas) = two_edits(false);
    let (mut state, flaky) = fold_flaky(&all, &[juans.id(), anas.id()]);
    let key = verse_key(1, 1);
    let conflicts: Vec<_> = state.conflicts().cloned().collect();
    assert_eq!(conflicts.len(), 1, "{conflicts:#?}");
    let mut both = vec![juans.id(), anas.id()];
    both.sort();
    assert_eq!(conflicts[0].unreadable, both);
    assert!(conflicts[0].candidates.is_empty());
    assert!(matches!(
        state.key_state(&key).map(|k| k.state),
        Some(tc_journal::fold::Resolved::Conflict(_))
    ));
    assert_eq!(state.unreadable_records(), vec![key.clone()]);
    assert!(
        state.shown_heads(&key, None).is_empty(),
        "nothing was shown"
    );
    assert_eq!(
        state.resolution(
            &conflicts[0].id,
            &tc_journal::command::Choice::Candidate { head: juans.id() }
        ),
        Err(tc_journal::fold::ResolveError::Unreadable(key.clone()))
    );
    flaky.blocked.lock().unwrap().clear();
    state.retry_unreadable();
    same_summary(&fold(&all), &state, "after the values can be read");
    assert!(state.unreadable_records().is_empty());
}

/// L98: equal versions that cannot be read are not settled as equal until they can be.
#[test]
fn unreadable_equal_versions_wait_to_be_read() {
    use support::*;
    let (all, juans, anas) = two_edits(true);
    let (mut state, flaky) = fold_flaky(&all, &[anas.id()]);
    assert_eq!(state.conflict_count(), 1);
    assert_eq!(
        state.conflicts().next().unwrap().unreadable,
        vec![anas.id()]
    );
    assert_eq!(
        state.shown_heads(&verse_key(1, 1), Some(juans.id())),
        vec![juans.id()],
        "the unread version is not in the token"
    );
    flaky.blocked.lock().unwrap().clear();
    state.retry_unreadable();
    assert_eq!(state.conflict_count(), 0);
    assert!(matches!(
        state.key_state(&verse_key(1, 1)).map(|k| k.state),
        Some(tc_journal::fold::Resolved::Merged { .. })
    ));
}

/// L98: a base that cannot be read is never taken as absent, so a merge never takes the
/// one readable head over a version nobody saw.
#[test]
fn unreadable_base_never_merges_away_a_version() {
    use support::*;
    let (all, juans, anas) = two_edits(false);
    let base = on(&all, &verse_key(1, 1)).id();
    let (state, _) = fold_flaky(&all, &[base, juans.id()]);
    let key = verse_key(1, 1);
    assert!(
        !matches!(
            state.key_state(&key).map(|k| k.state),
            Some(tc_journal::fold::Resolved::Merged { .. })
        ),
        "{:?}",
        state.key_state(&key)
    );
    let conflict = state.conflicts().next().cloned().expect("listed");
    let mut unread = vec![base, juans.id()];
    unread.sort();
    assert_eq!(conflict.unreadable, unread);
    assert_eq!(
        conflict
            .candidates
            .iter()
            .map(|c| c.head)
            .collect::<Vec<_>>(),
        vec![anas.id()]
    );
    for displayed in [None, Some(anas.id()), Some(juans.id())] {
        assert!(
            !state.shown_heads(&key, displayed).contains(&juans.id()),
            "a token names a version nobody saw"
        );
    }
}

/// L98: a settled record whose one version cannot be read was not shown, so an edit made
/// meanwhile does not replace it.
#[test]
fn unreadable_settled_version_is_not_in_a_token() {
    use support::*;
    let (all, _, _) = two_edits(false);
    let book: Vec<_> = all[..all.len() - 2].to_vec();
    let base = on(&book, &verse_key(1, 1)).id();
    let (state, flaky) = fold_flaky(&book, &[base]);
    assert!(state.shown_heads(&verse_key(1, 1), None).is_empty());
    flaky.blocked.lock().unwrap().clear();
    assert_eq!(state.shown_heads(&verse_key(1, 1), None), vec![base]);
}

/// A fold from scratch indexes its input a chunk at a time as a loader hands it over
/// (§14): a change that arrives before its parent's chunk is linked to it as if they had
/// come together, whatever the order.
#[test]
fn a_fold_indexed_as_it_arrives_is_the_fold_of_the_set() {
    let corpus = Corpus::generate(&Spec::psalms(1)).expect("the corpus builds");
    let lines = corpus.journal.lines();
    assert!(lines.len() > 40_000, "more than one chunk: {}", lines.len());
    let forward = FoldState::fold(
        ProjectKind::Bible,
        lines.iter().cloned().map(|l| FoldInput::from(l).prepare()),
    );
    let reversed = FoldState::fold(
        ProjectKind::Bible,
        lines
            .iter()
            .rev()
            .cloned()
            .map(|l| FoldInput::from(l).prepare()),
    );
    assert_eq!(forward.frontier(), reversed.frontier());
    assert_eq!(
        forward.conflicts().collect::<Vec<_>>(),
        reversed.conflicts().collect::<Vec<_>>()
    );
    assert_eq!(
        forward.anomalies().collect::<Vec<_>>(),
        reversed.anomalies().collect::<Vec<_>>(),
        "no parent is missing because it came in a later chunk"
    );
    for file in forward.derived_files() {
        if file.book() == Some(tc_journal::BookId::Psalms) {
            continue;
        }
        assert_eq!(
            forward.render(&file),
            reversed.render(&file),
            "{}",
            file.path()
        );
    }
    let psalms = tc_journal::fold::DerivedFile::Usfm(tc_journal::BookId::Psalms);
    assert_eq!(forward.render(&psalms), reversed.render(&psalms));
}
