//! Values read on demand (§14): a fold that keeps only where each change's bytes lie, and
//! reads them from its source when it merges, lists a conflict or projects, is the fold of
//! the same changes kept in memory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::pack::read_pack;
use tc_journal::pending::read_pending;

/// The files of a corpus, read again by span, as `tc-store` reads packs on disk.
#[derive(Debug)]
struct Files {
    files: Vec<Vec<u8>>,
    /// Files whose bytes changed after they were read.
    changed: Mutex<Vec<u32>>,
}

impl ValueSource for Files {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        let file = self.files.get(at.file as usize)?;
        let start = usize::try_from(at.offset).ok()?;
        let mut bytes = file.get(start..start + at.len as usize)?.to_vec();
        if self.changed.lock().ok()?.contains(&at.file)
            && let Some(first) = bytes.first_mut()
        {
            *first ^= 1;
        }
        Some(bytes)
    }
}

fn corpus() -> Corpus {
    Corpus::generate(&Spec::psalms(5).with_history(60, 0.1)).expect("the corpus builds")
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
    files.changed.lock().unwrap().push(file);
    assert_eq!(state.raw(&head), None);
    assert_eq!(state.body(&head), None);
}
