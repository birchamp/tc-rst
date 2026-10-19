//! The scale corpus of PERFORMANCE.md: tc-corpus Psalms and the corpus ten times larger, each
//! generated and folded once per test binary, as tc-journal's own count tests fold them.

use std::sync::OnceLock;

use tc_app::backend::memory::MemoryProject;
use tc_corpus::{Corpus, Spec};
use tc_journal::BookId;
use tc_journal::fold::{ChapterKeys, FoldInput, FoldState, ProjectKind, Resolved};

pub const PSALMS: BookId = BookId::Psalms;

pub struct Corpora {
    pub psalms: MemoryProject,
    pub larger: MemoryProject,
    /// A Psalms chapter whose records are the same in both corpora, with no comment and no
    /// conflict: an action on it touches the same work in both, so only the project's size
    /// differs.
    pub chapter: u16,
}

fn project(spec: &Spec) -> MemoryProject {
    let corpus = Corpus::generate(spec).expect("the corpus builds");
    let fold = FoldState::fold(
        ProjectKind::Bible,
        corpus
            .journal
            .lines()
            .into_iter()
            .map(|line| FoldInput::from(line).prepare()),
    );
    let mut project = MemoryProject::empty(corpus.journal.project, ProjectKind::Bible);
    project.fold = fold;
    project
}

/// Found with R1's reads alone: the chapter's keys, and each key's state.
fn quiet(fold: &FoldState, keys: &ChapterKeys) -> bool {
    keys.comments.is_empty()
        && keys
            .front
            .iter()
            .chain(&keys.verses)
            .chain(&keys.alignments)
            .chain(&keys.checks)
            .all(|key| {
                fold.key_state(key).is_none_or(|state| {
                    state.heads.len() <= 1 && !matches!(state.state, Resolved::Conflict(_))
                })
            })
}

pub fn corpora() -> &'static Corpora {
    static CORPORA: OnceLock<Corpora> = OnceLock::new();
    CORPORA.get_or_init(|| {
        let psalms = project(&Spec::psalms(1));
        let larger = project(&Spec::scaled(1, 10));
        let chapter = psalms
            .fold
            .chapters_of(PSALMS)
            .into_iter()
            .find(|&c| {
                let small = psalms.fold.chapter_keys(PSALMS, c);
                let big = larger.fold.chapter_keys(PSALMS, c);
                small == big
                    && !small.verses.is_empty()
                    && quiet(&psalms.fold, &small)
                    && quiet(&larger.fold, &big)
            })
            .expect("a Psalms chapter alike in both corpora");
        Corpora {
            psalms,
            larger,
            chapter,
        }
    })
}
