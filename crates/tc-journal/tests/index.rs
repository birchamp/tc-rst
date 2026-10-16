//! The fold cache, `.tc/index.bin` (`docs/STORAGE.md` §3.6, §6.1 O8, §14): an open that
//! takes a file's prepared changes from the cache gets exactly what reading and parsing the
//! file gives, and a cache that does not stand for the file's bytes is never used.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod support;

use common::{arb_body, fixtures, local, project, verse_change};
use proptest::prelude::*;
use std::sync::Arc;
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::index::{
    FileKey, Fingerprinter, INDEX_VERSION, Index, IndexError, Loaded, Section, fingerprint,
    load_pack, load_pending,
};
use tc_journal::json::{self, Json};
use tc_journal::pack::{PackSpec, seal};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::{Change, ChangeId, PackId, ProjectId, Verified, WallTime, parse_change};

const BUILD: &str = "test build";

/// Whether a file of a project folder is a pack or a pending file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Pack,
    Pending,
}

/// One pack or pending file of a project folder.
#[derive(Debug, Clone)]
struct File {
    kind: FileKind,
    path: String,
    bytes: Vec<u8>,
}

impl File {
    fn key(&self) -> FileKey {
        FileKey::of(&self.path, &self.bytes)
    }

    /// The file read in full at file number `file`, as an open without the cache reads it.
    fn load(&self, project: ProjectId, file: u32) -> Loaded {
        match self.kind {
            FileKind::Pack => load_pack(&self.path, &self.bytes, project, file).1,
            FileKind::Pending => load_pending(&self.path, &self.bytes, project, file).1,
        }
    }

    /// The file's lines, verified, parsed and prepared at file number `file`.
    fn read(&self, project: ProjectId, file: u32) -> Vec<FoldInput> {
        self.load(project, file).inputs
    }

    fn section(&self, project: ProjectId, file: u32) -> Option<Section> {
        self.load(project, file).section
    }
}

/// The files of an open folder, read again by span.
#[derive(Debug)]
struct Files(Vec<Vec<u8>>);

impl ValueSource for Files {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        let file = self.0.get(at.file as usize)?;
        let start = usize::try_from(at.offset).ok()?;
        Some(file.get(start..start + at.len as usize)?.to_vec())
    }
}

/// How each input reads: two inputs with the same text are the same prepared change.
fn texts(inputs: &[FoldInput]) -> Vec<String> {
    inputs.iter().map(|i| format!("{i:?}")).collect()
}

fn pending_of(changes: &[Verified], device: u8) -> File {
    let encoded = encode_pending(
        &PendingHeader {
            device: local(device),
            project: project(),
            purpose: Purpose::Save,
            reconciles: None,
        },
        changes,
    )
    .unwrap();
    File {
        kind: FileKind::Pending,
        path: format!(".tc/pending/{}", encoded.name),
        bytes: encoded.bytes,
    }
}

fn packs_of(changes: &[Verified]) -> Vec<File> {
    let sealed = seal(
        &PackSpec {
            project: project(),
            sealer: local(1),
            sealed: WallTime(1_760_000_000_000),
        },
        changes,
    )
    .unwrap();
    sealed
        .packs
        .into_iter()
        .map(|pack| File {
            kind: FileKind::Pack,
            path: pack.path.to_string(),
            bytes: pack.bytes,
        })
        .collect()
}

/// The pinned change of every record kind (`tests/fixtures/v0-every-kind.ndjson`), and a
/// change of a kind from a newer minor, which the fold holds as an opaque record.
fn every_kind() -> Vec<Verified> {
    let pinned = std::fs::read(fixtures().join("v0-every-kind.ndjson")).unwrap();
    let mut out: Vec<Verified> = pinned
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|line| tc_journal::line::read_line(line).unwrap())
        .collect();
    let base = verse_change(local(2), 7, 3, "x");
    let Json::Obj(mut map) = json::parse(base.raw()).unwrap() else {
        panic!("a change is an object")
    };
    map.insert(
        "record".into(),
        json::parse(br#"{"note":{"id":"n1","value":{"text":"later"}}}"#).unwrap(),
    );
    map.insert("v".into(), Json::from(1u64));
    let text = Json::Obj(map).to_canonical_string();
    let newer = parse_change(ChangeId::of(text.as_bytes()), Arc::from(text.as_bytes()));
    assert!(matches!(newer, Verified::Change(_)), "{newer:?}");
    out.push(newer);
    out
}

fn assert_same_inputs(file: &File, section: &Section, number: u32) {
    let from_cache = section.inputs(number).unwrap();
    assert_eq!(texts(&from_cache), texts(&file.read(project(), number)));
}

#[test]
fn every_kind_from_the_cache_is_what_parsing_gives() {
    let changes = every_kind();
    let mut files = packs_of(&changes);
    files.push(pending_of(&changes, 3));
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            f.section(project(), n as u32)
                .expect("the file can be cached")
        })
        .collect();
    let index = Index::parse(Index::encode(project(), BUILD, &sections), project(), BUILD).unwrap();
    assert_eq!(index.len(), files.len());
    for (n, file) in files.iter().enumerate() {
        let section = index.section(&file.key()).expect("a section for the file");
        let pack = (file.kind == FileKind::Pack).then(|| PackId::of(&file.bytes));
        assert_eq!(section.pack(), pack);
        // The loader numbers files anew at every open.
        assert_same_inputs(file, section, n as u32 + 7);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn any_changes_come_back_from_the_cache_unchanged(
        bodies in prop::collection::vec(arb_body(), 1..12),
        file in any::<u32>(),
    ) {
        let changes: Vec<Verified> = bodies
            .into_iter()
            .map(|b| Verified::Change(Change::new(b).unwrap()))
            .collect();
        for file_ in packs_of(&changes).into_iter().chain([pending_of(&changes, 4)]) {
            let section = file_.section(project(), file).expect("the file can be cached");
            let bytes = Index::encode(project(), BUILD, [&section]);
            let index = Index::parse(bytes, project(), BUILD).unwrap();
            let from_cache = index.section(&file_.key()).unwrap().inputs(file).unwrap();
            prop_assert_eq!(texts(&from_cache), texts(&file_.read(project(), file)));
        }
    }
}

/// The corpus of the operation-count tests, smaller: real Psalms text and checks, and a
/// generated team history with conflicts.
fn corpus_files() -> Vec<File> {
    let corpus =
        Corpus::generate(&Spec::psalms(5).with_history(60, 0.1)).expect("the corpus builds");
    let mut files: Vec<File> = corpus
        .journal
        .packs
        .iter()
        .map(|p| File {
            kind: FileKind::Pack,
            path: p.path.clone(),
            bytes: p.bytes.clone(),
        })
        .collect();
    files.extend(corpus.journal.pending.iter().map(|p| File {
        kind: FileKind::Pending,
        path: format!(".tc/pending/{}", p.name),
        bytes: p.bytes.clone(),
    }));
    assert_eq!(corpus.journal.project, project_of(&files));
    files
}

fn project_of(files: &[File]) -> ProjectId {
    let pack = files.iter().find(|f| f.kind == FileKind::Pack).unwrap();
    let text = std::str::from_utf8(&pack.bytes).unwrap();
    let header = text.split('\t').next().unwrap();
    let Json::Obj(map) = json::parse(header.as_bytes()).unwrap() else {
        panic!("a header is an object")
    };
    ProjectId::parse(map.get("project").and_then(Json::as_str).unwrap()).unwrap()
}

/// An open of `files`: each file's inputs from `index` when it holds the file's bytes,
/// read from the file otherwise. Returns the fold and how many files came from the index.
fn open(files: &[File], index: Option<&Index>) -> (FoldState, usize) {
    let project = project_of(files);
    let mut inputs = Vec::new();
    let mut cached = 0;
    for (n, file) in files.iter().enumerate() {
        let n = n as u32;
        match index
            .and_then(|i| i.section(&file.key()))
            .map(|s| s.inputs(n))
        {
            Some(Ok(from_cache)) => {
                cached += 1;
                inputs.extend(from_cache);
            }
            _ => inputs.extend(file.read(project, n)),
        }
    }
    let source: Arc<dyn ValueSource> =
        Arc::new(Files(files.iter().map(|f| f.bytes.clone()).collect()));
    (
        FoldState::fold_from(ProjectKind::Bible, Some(source), inputs),
        cached,
    )
}

#[test]
fn an_open_from_the_cache_is_the_open_from_the_files() {
    let files = corpus_files();
    let project = project_of(&files);
    let (read, _) = open(&files, None);
    assert!(read.conflict_count() > 0, "the history has conflicts");
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            f.section(project, n as u32)
                .expect("the file can be cached")
        })
        .collect();
    let index = Index::parse(Index::encode(project, BUILD, &sections), project, BUILD).unwrap();
    let (cached, from_cache) = open(&files, Some(&index));
    assert_eq!(from_cache, files.len());
    support::same_fold(&read, &cached, "every file from the cache");

    // Saves since the cache was written, and a seal that replaced some pending files with a
    // pack: the new files are read, the gone ones never asked for, and the open is the
    // same as reading every file.
    let mut later = files.clone();
    let pending: Vec<usize> = (0..later.len())
        .filter(|n| later[*n].kind == FileKind::Pending)
        .collect();
    assert!(pending.len() >= 2, "the corpus has pending files");
    let sealed_lines: Vec<Verified> = pending[..2]
        .iter()
        .flat_map(|n| read_pending(&later[*n].bytes, project).entries)
        .collect();
    let sealed = seal(
        &PackSpec {
            project,
            sealer: local(9),
            sealed: WallTime(1_900_000_000_000),
        },
        &sealed_lines,
    )
    .unwrap();
    for n in pending[..2].iter().rev() {
        later.remove(*n);
    }
    later.extend(sealed.packs.into_iter().map(|p| File {
        kind: FileKind::Pack,
        path: p.path.to_string(),
        bytes: p.bytes,
    }));
    let save = |seq: u64, words: &str| {
        let change = verse_change(local(9), seq, 1, words);
        encode_pending(
            &PendingHeader {
                device: local(9),
                project,
                purpose: Purpose::Save,
                reconciles: None,
            },
            &[Verified::Change(change)],
        )
        .unwrap()
    };
    let saved = save(1, "a save after the cache was written");
    later.push(File {
        kind: FileKind::Pending,
        path: format!(".tc/pending/{}", saved.name),
        bytes: saved.bytes,
    });
    // A file the cache holds, whose bytes another program replaced under the same name
    // (every file in `.tc/pending/` is read whatever its name, §4.2).
    let replaced = later
        .iter()
        .position(|f| f.kind == FileKind::Pending && files.iter().any(|g| g.path == f.path))
        .expect("a pending file the cache holds");
    later[replaced].bytes = save(2, "another save under the same name").bytes;
    let (read_later, _) = open(&later, None);
    let (cached_later, from_cache) = open(&later, Some(&index));
    assert_eq!(
        from_cache,
        later.len() - 3,
        "the new pack, the new save and the replaced file are read"
    );
    support::same_fold(
        &read_later,
        &cached_later,
        "after saves, a seal and a replaced file",
    );
}

#[test]
fn a_section_stands_only_for_the_bytes_it_was_made_from() {
    let file = pending_of(&every_kind(), 3);
    let section = file.section(project(), 0).unwrap();
    let index = Index::parse(
        Index::encode(project(), BUILD, [&section]),
        project(),
        BUILD,
    )
    .unwrap();
    let key = file.key();
    assert!(index.section(&key).is_some());
    let mut other = key.clone();
    other.size += 1;
    assert!(index.section(&other).is_none(), "another size");
    let mut other = key.clone();
    other.fingerprint ^= 1;
    assert!(index.section(&other).is_none(), "other bytes");
    let mut other = key.clone();
    other.path = ".tc/pending/copy.batch".into();
    assert!(index.section(&other).is_none(), "another path");
}

#[test]
fn an_index_of_another_project_version_or_build_is_not_read() {
    let file = pending_of(&every_kind(), 3);
    let section = file.section(project(), 0).unwrap();
    let bytes = Index::encode(project(), BUILD, [&section]);
    assert_eq!(
        Index::parse(bytes.clone(), common::other_project(), BUILD).err(),
        Some(IndexError::Project)
    );
    assert_eq!(
        Index::parse(bytes.clone(), project(), "another build").err(),
        Some(IndexError::Build)
    );
    let mut newer = bytes.clone();
    newer[8..12].copy_from_slice(&(INDEX_VERSION + 1).to_le_bytes());
    assert_eq!(
        Index::parse(newer, project(), BUILD).err(),
        Some(IndexError::Version(INDEX_VERSION + 1))
    );
    let mut minor = bytes.clone();
    minor[12] ^= 1;
    assert!(matches!(
        Index::parse(minor, project(), BUILD).err(),
        Some(IndexError::Minor(_))
    ));
    assert_eq!(
        Index::parse(b"not an index".to_vec(), project(), BUILD).err(),
        Some(IndexError::NotAnIndex)
    );
}

/// Every single-bit flip and every truncation of an index either makes it unreadable,
/// makes its section unused, or leaves what it gives exactly as before: the open then
/// reads the file, so a damaged cache can cost time, never a change.
#[test]
fn a_damaged_index_never_gives_other_changes() {
    let changes: Vec<Verified> = every_kind().into_iter().take(6).collect();
    let file = pending_of(&changes, 3);
    let section = file.section(project(), 0).unwrap();
    let bytes = Index::encode(project(), BUILD, [&section]);
    let expected = texts(&file.read(project(), 0));
    let check = |damaged: Vec<u8>, how: &str| {
        let Ok(index) = Index::parse(damaged, project(), BUILD) else {
            return;
        };
        let Some(section) = index.section(&file.key()) else {
            return;
        };
        if let Ok(inputs) = section.inputs(0) {
            assert_eq!(texts(&inputs), expected, "{how}");
        }
    };
    let mut unused = 0;
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut damaged = bytes.clone();
            damaged[at] ^= 1 << bit;
            check(damaged.clone(), &format!("bit {bit} of byte {at}"));
            let used = Index::parse(damaged, project(), BUILD)
                .ok()
                .and_then(|i| i.section(&file.key()).map(|s| s.inputs(0).is_ok()))
                .unwrap_or(false);
            unused += usize::from(!used);
        }
    }
    assert_eq!(unused, bytes.len() * 8, "no damaged index is used");
    for len in 0..bytes.len() {
        check(bytes[..len].to_vec(), &format!("cut at {len}"));
        assert!(Index::parse(bytes[..len].to_vec(), project(), BUILD).is_err());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_bytes_read_as_an_index_never_panic(
        bytes in prop::collection::vec(any::<u8>(), 0..400),
        splice in any::<bool>(),
    ) {
        let input = if splice {
            // A valid header, so the section framing and decoding are reached.
            let file = pending_of(&every_kind()[..2], 3);
            let section = file.section(project(), 0).unwrap();
            let mut valid = Index::encode(project(), BUILD, [&section]);
            let keep = valid.len().min(40);
            valid.truncate(keep);
            valid.extend_from_slice(&bytes);
            valid
        } else {
            bytes
        };
        if let Ok(index) = Index::parse(input, project(), BUILD) {
            for section in index.sections() {
                let _ = section.inputs(0);
            }
        }
    }
}

#[test]
fn a_file_the_cache_cannot_stand_for_has_no_section() {
    let good = verse_change(local(1), 1, 1, "a");
    let text = std::str::from_utf8(good.raw())
        .unwrap()
        .replacen(',', ", ", 1);
    let quarantined = parse_change(ChangeId::of(text.as_bytes()), Arc::from(text.as_bytes()));
    assert!(matches!(quarantined, Verified::Quarantined(_)));
    let lines = [Verified::Change(good.clone()), quarantined];
    let file = pending_of(&lines, 3);
    let (read, loaded) = load_pending(&file.path, &file.bytes, project(), 0);
    assert_eq!(read.status, PendingStatus::Valid);
    assert_eq!(loaded.inputs.len(), 2);
    assert!(
        loaded.section.is_none(),
        "a quarantined line keeps its bytes in memory"
    );
    for pack in packs_of(&lines) {
        let (read, loaded) = load_pack(&pack.path, &pack.bytes, project(), 0);
        assert!(read.is_valid());
        assert!(loaded.section.is_none(), "a pack with a quarantined line");
    }

    // A damaged file is read, salvaged and reported at every open, never cached; so is a
    // file of another project, or of a newer framing.
    let file = pending_of(&[Verified::Change(good.clone())], 3);
    let pack = packs_of(&[Verified::Change(good)]).remove(0);
    assert!(file.section(project(), 0).is_some());
    assert!(pack.section(project(), 0).is_some());
    for mut damaged in [file.clone(), pack.clone()] {
        damaged.bytes.pop();
        let loaded = damaged.load(project(), 0);
        assert_eq!(loaded.inputs.len(), 1, "the line is salvaged");
        assert!(loaded.section.is_none(), "{}", damaged.path);
    }
    let mut moved = pack.clone();
    moved.path = moved.path.replace("/2025-", "/x2025-");
    assert!(
        moved.section(project(), 0).is_none(),
        "a pack at a path that breaks P1"
    );
    for f in [&file, &pack] {
        assert!(
            f.section(common::other_project(), 0).is_none(),
            "a foreign file"
        );
    }
}

/// The bytes of the cache of the pinned changes of every kind. A change to what a prepared
/// change keeps, or to how the cache writes it, changes them: then raise `INDEX_VERSION`,
/// so no app reads a cache an older build wrote as its own, and pin the new digest.
#[test]
fn the_encoding_is_pinned() {
    let changes = every_kind();
    let mut files = packs_of(&changes);
    files.push(pending_of(&changes, 3));
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| f.section(project(), n as u32).unwrap())
        .collect();
    let bytes = Index::encode(project(), BUILD, &sections);
    let digest = PackId::of(&bytes).to_hex();
    assert_eq!(
        (INDEX_VERSION, digest.as_str()),
        (
            1,
            "2b157fa1f612b0f4131a921a287faf05663cf70d35b26d9b40e169879b85f7a3"
        ),
        "the cache's bytes changed: raise INDEX_VERSION and pin the new digest"
    );
}

#[test]
fn the_same_sections_give_the_same_bytes() {
    let changes = every_kind();
    let mut files = packs_of(&changes);
    files.push(pending_of(&changes, 3));
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| f.section(project(), n as u32).unwrap())
        .collect();
    let forward = Index::encode(project(), BUILD, &sections);
    let backward = Index::encode(project(), BUILD, sections.iter().rev());
    assert_eq!(forward, backward);
    // An index written from the sections of a parsed index is the same index.
    let index = Index::parse(forward.clone(), project(), BUILD).unwrap();
    assert_eq!(Index::encode(project(), BUILD, index.sections()), forward);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// An open fingerprints a file as it reads it, a chunk at a time.
    #[test]
    fn a_file_read_in_chunks_has_the_fingerprint_of_its_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..200),
        cuts in prop::collection::vec(0usize..200, 0..8),
    ) {
        let mut cuts: Vec<usize> = cuts.into_iter().map(|c| c.min(bytes.len())).collect();
        cuts.sort_unstable();
        let mut chunked = Fingerprinter::new();
        let mut from = 0;
        for cut in cuts.into_iter().chain([bytes.len()]) {
            chunked.update(&bytes[from..cut]);
            from = cut;
        }
        prop_assert_eq!(chunked.finish(), fingerprint(&bytes));
        let mut flipped = bytes.clone();
        if let Some(byte) = flipped.first_mut() {
            *byte ^= 1;
            prop_assert_ne!(fingerprint(&flipped), fingerprint(&bytes));
        }
    }
}
