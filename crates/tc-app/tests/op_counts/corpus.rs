//! The scale corpus of PERFORMANCE.md: tc-corpus Psalms and the corpus ten times larger, each
//! generated and folded once per test binary, as tc-journal's own count tests fold them, and
//! each written once to tc-store's in-memory file system as tc-store's own count tests write
//! it (C1a, §8.1: the rows run on the store too).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tc_app::backend::memory::MemoryProject;
use tc_corpus::history::{Journal, PackFile};
use tc_corpus::{Corpus, Spec};
use tc_journal::change::Verified;
use tc_journal::fold::{ChapterKeys, FoldInput, FoldState, ProjectKind, Resolved};
use tc_journal::format::FormatDoc;
use tc_journal::{BookId, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::layout::{GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{FakePlatform, OpenOptions};

use crate::store_world::{APP, PROJECTS, assert_valid_burrito, backend_over};

/// The corpus project's folder name, on both backends.
pub const FOLDER: &str = "psalms";

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
    project_of(&Corpus::generate(spec).expect("the corpus builds").journal)
}

fn project_of(journal: &Journal) -> MemoryProject {
    let fold = FoldState::fold(
        ProjectKind::Bible,
        journal
            .lines()
            .into_iter()
            .map(|line| FoldInput::from(line).prepare()),
    );
    let mut project = MemoryProject::empty(journal.project, ProjectKind::Bible);
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
        let larger = project(&Spec::scaled(1, Spec::test_factor()));
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

/// The corpora on disk: each a file system holding the corpus project at
/// `/projects/psalms`, opened once (which materializes every file), sealed and consolidated by
/// a checkpoint, and closed. Each scale takes a copy.
pub struct StoreCorpora {
    pub psalms: FaultFs,
    pub larger: FaultFs,
}

/// tc-store's `put_corpus` (`crates/tc-store/tests/store/corpus.rs`): the corpus's packs and
/// pending files, the fixed files, and `.tc/device.json`, so the corpus's device 0 is the
/// folder's device.
fn put_corpus(store: &tc_store::Store, fs: &FaultFs, folder: &Path, journal: &Journal) {
    let project = serde_json::json!({
        "schemaVersion": 1,
        "projectId": journal.project.to_string(),
        "kind": "bible",
        "created": "2026-10-17T00:00:00.000Z",
    });
    let put = |rel: &str, bytes: &[u8]| {
        let path = rel
            .split('/')
            .fold(folder.to_owned(), |p, part| p.join(part));
        fs.put(&path, bytes, None);
    };
    put(
        "ingredients/tc/project.json",
        project.to_string().as_bytes(),
    );
    put(
        "ingredients/tc/format.json",
        &FormatDoc::current().to_bytes(),
    );
    put(".gitignore", GITIGNORE_BYTES);
    put(".gitattributes", GITATTRIBUTES_BYTES);
    for pack in &journal.packs {
        put(&pack.path, &pack.bytes);
    }
    for pending in &journal.pending {
        put(&format!(".tc/pending/{}", pending.name), &pending.bytes);
    }
    let print = tc_store::device::fingerprint(
        store.app_data().secret(),
        &FakePlatform::default().machine,
        &folder.to_string_lossy(),
    );
    let device = serde_json::json!({
        "device": journal.devices[0].to_string(),
        "fingerprint": print.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "created": "2026-10-17T00:00:00.000Z",
    });
    put(".tc/device.json", device.to_string().as_bytes());
}

fn on_disk(spec: &Spec) -> FaultFs {
    on_disk_of(&Corpus::generate(spec).expect("the corpus builds").journal)
}

fn on_disk_of(journal: &Journal) -> FaultFs {
    let fs = FaultFs::new();
    let (backend, _backend_watch) = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let folder = Path::new(PROJECTS).join(FOLDER);
    put_corpus(backend.store(), &fs, &folder, journal);
    let handle = backend
        .store()
        .open(&folder, &OpenOptions::default())
        .unwrap();
    assert!(matches!(handle.facts().mode, tc_store::Mode::Writable));
    handle
        .checkpoint(tc_store::CheckpointReason::AfterImport)
        .wait()
        .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
    fs.crash(false)
}

pub fn store_corpora() -> &'static StoreCorpora {
    static CORPORA: OnceLock<StoreCorpora> = OnceLock::new();
    CORPORA.get_or_init(|| StoreCorpora {
        psalms: on_disk(&Spec::psalms(1)),
        larger: on_disk(&Spec::scaled(1, Spec::test_factor())),
    })
}

/// The larger corpus's Psalms alone, in memory and on disk: its journal without the other
/// books' records and the comments on them. More devices wrote the larger corpus, so its
/// Psalms holds a few more or fewer merged and conflicting records than the Psalms corpus;
/// an action on Psalms costs the same on this as on the larger corpus, or it costs the
/// project's size.
pub struct LargerPsalms {
    pub memory: MemoryProject,
    pub disk: FaultFs,
}

pub fn larger_psalms() -> &'static LargerPsalms {
    static ALONE: OnceLock<LargerPsalms> = OnceLock::new();
    ALONE.get_or_init(|| {
        let larger = Corpus::generate(&Spec::scaled(1, Spec::test_factor()))
            .expect("the corpus builds")
            .journal;
        let journal = psalms_alone(&larger, &corpora().larger);
        let memory = project_of(&journal);
        assert_eq!(memory.fold.projected_books(), vec![PSALMS]);
        let keys = corpora().larger.fold.book_keys(PSALMS);
        assert_eq!(memory.fold.book_keys(PSALMS), keys);
        for key in &keys {
            assert_eq!(
                memory.fold.key_state(key),
                corpora().larger.fold.key_state(key),
                "Psalms alone holds the larger corpus's {key:?}"
            );
        }
        LargerPsalms {
            memory,
            disk: on_disk_of(&journal),
        }
    })
}

/// `journal` with Psalms' records, the comments `folded` places in Psalms and the records
/// of no book, each pack and pending file kept as it was with the rest left out.
fn psalms_alone(journal: &Journal, folded: &MemoryProject) -> Journal {
    let comments: std::collections::BTreeSet<RecordKey> =
        folded.fold.comments_of(PSALMS).into_iter().collect();
    let keep = |line: &Verified| match line.as_change().map(|c| c.body().record.key()) {
        Some(key @ RecordKey::Comment(_)) => comments.contains(&key),
        Some(key) => key.book().is_none_or(|book| book == PSALMS),
        None => true,
    };
    let mut packs = Vec::new();
    for pack in &journal.packs {
        let read = tc_journal::pack::read_pack(&pack.path, &pack.bytes, journal.project);
        let header = read.header.expect("a corpus pack has its header");
        let lines: Vec<Verified> = read.entries.into_iter().filter(keep).collect();
        if lines.is_empty() {
            continue;
        }
        let spec = tc_journal::pack::PackSpec {
            project: journal.project,
            sealer: header.sealer,
            sealed: header.sealed,
        };
        let sealed = tc_journal::pack::seal(&spec, &lines).unwrap();
        assert!(sealed.set_aside.is_empty());
        packs.extend(sealed.packs.into_iter().map(|p| PackFile {
            path: p.path.to_string(),
            id: p.id,
            changes: p.changes.len(),
            bytes: p.bytes,
        }));
    }
    let pending = journal
        .pending
        .iter()
        .filter_map(|file| {
            let read = tc_journal::pending::read_pending(&file.bytes, journal.project);
            let lines: Vec<Verified> = read.entries.into_iter().filter(keep).collect();
            (!lines.is_empty()).then(|| {
                let header = read.header.expect("a corpus pending file has its header");
                tc_journal::pending::encode_pending(&header, &lines).unwrap()
            })
        })
        .collect();
    Journal {
        project: journal.project,
        devices: journal.devices.clone(),
        packs,
        pending,
        stats: Default::default(),
    }
}

#[allow(dead_code)]
#[path = "../../examples/bundle_store/build.rs"]
mod bundle_build;

/// A resource store with the bundle's English notes, words links and words articles and the
/// Hebrew Bible, the releases the corpus pins, built once per test run.
pub fn psalms_resources() -> &'static Path {
    static STORE: OnceLock<PathBuf> = OnceLock::new();
    STORE.get_or_init(|| {
        let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en");
        let wanted = ["en_tn", "en_twl", "en_tw", "hbo_uhb"];
        bundle_build::build_once_per_run(
            &PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
            "b5a-psalms-resources",
            &from,
            &|pin| wanted.contains(&pin.repo.name().as_str()),
            4,
        )
        .unwrap_or_else(|e| panic!("the corpus's resources could not be installed: {e}"))
    })
}
