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
use tc_journal::consolidate::{consolidate, redundancy};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::index::{
    Facts, FileKey, Fingerprinter, INDEX_VERSION, Index, IndexError, Loaded, PROBE, PROBE_PROJECT,
    Section, fingerprint, load_pack, load_pending, stamp,
};
use tc_journal::json::{self, Json};
use tc_journal::pack::{PackFacts, PackPath, PackSpec, YearMonth, encode_pack, seal};
use tc_journal::pending::Reconciles;
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::record::VerseValue;
use tc_journal::{
    Cause, Change, ChangeId, Md5, PackId, ProjectId, Record, RepoPath, Verified, WallTime,
    parse_change,
};

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
            FileKind::Pack => load_pack(&self.path, &self.bytes, project, file),
            FileKind::Pending => load_pending(&self.path, &self.bytes, project, file),
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

/// The pinned change of every record kind (`tests/fixtures/v0-every-kind.ndjson`), a change
/// of a kind from a newer minor, which the fold holds as an opaque record, one with a cause
/// from a newer minor, and values the fixture does not hold.
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
    let newer_text = Json::Obj(map.clone()).to_canonical_string();
    let newer = parse_change(
        ChangeId::of(newer_text.as_bytes()),
        Arc::from(newer_text.as_bytes()),
    );
    assert!(matches!(newer, Verified::Change(_)), "{newer:?}");
    out.push(newer);
    // A cause from a newer minor, which the fold keeps as `Cause::Newer`.
    map.insert("cause".into(), json::parse(br#"{"later":{}}"#).unwrap());
    let newer_cause_text = Json::Obj(map).to_canonical_string();
    let newer_cause = parse_change(
        ChangeId::of(newer_cause_text.as_bytes()),
        Arc::from(newer_cause_text.as_bytes()),
    );
    let Verified::Change(change) = &newer_cause else {
        panic!("{newer_cause:?}")
    };
    assert!(matches!(
        change.body().cause,
        tc_journal::Cause::Newer { .. }
    ));
    out.push(newer_cause);
    // Values the generated fixture happens not to hold.
    use support::{Dev, absent, check_key, comment, present, text, text_from, verse};
    use tc_journal::record::PresentHow;
    let mut dev = Dev::new(8);
    let values = vec![
        verse(1, 1, text("words", 1)),
        verse(1, 3, text_from("moved here", 3, (2, 1))),
        verse(2, 1, absent(None)),
        present(PresentHow::Added),
        present(PresentHow::StartedEmpty),
        comment(
            9,
            tc_journal::record::CommentTarget::Check(check_key("abc1", 1, 1)),
            "on a check",
        ),
    ];
    for record in values {
        out.push(Verified::Change(dev.one(record, vec![])));
    }
    out
}

/// The section gives what reading the file gives: the same facts and the same prepared
/// changes.
fn assert_same_inputs(file: &File, section: &Section, number: u32) {
    let from_cache = section.load(number).unwrap();
    let read = file.load(project(), number);
    assert_eq!(from_cache.facts, read.facts);
    assert_eq!(texts(&from_cache.inputs), texts(&read.inputs));
}

/// A cache written piece by piece is the cache written whole, so the store can write the file
/// of a whole Bible from its sections without holding it (PERFORMANCE.md memory rows).
#[test]
fn an_index_written_in_pieces_is_the_index_written_whole() {
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
    let whole = Index::encode(project(), &sections);
    let mut pieces = Vec::new();
    let mut calls = 0;
    Index::encode_to::<std::convert::Infallible>(project(), &sections, &mut |bytes| {
        calls += 1;
        pieces.extend_from_slice(bytes);
        Ok(())
    })
    .unwrap();
    assert_eq!(pieces, whole);
    assert!(calls > sections.len(), "each section goes out as it is");
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
    let index = Index::parse(Index::encode(project(), &sections), project()).unwrap();
    assert_eq!(index.len(), files.len());
    for (n, file) in files.iter().enumerate() {
        let section = index.section(&file.key()).expect("a section for the file");
        let pack = (file.kind == FileKind::Pack).then(|| common::pack_id_of(&file.bytes));
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
            let bytes = Index::encode(project(), [&section]);
            let index = Index::parse(bytes, project()).unwrap();
            let from_cache = index.section(&file_.key()).unwrap().load(file).unwrap();
            let read = file_.load(project(), file);
            prop_assert_eq!(&from_cache.facts, &read.facts);
            prop_assert_eq!(texts(&from_cache.inputs), texts(&read.inputs));
        }
    }
}

/// The corpus of the operation-count tests, smaller: real Psalms text and checks, and a
/// generated team history with conflicts.
fn corpus_files() -> Vec<File> {
    let corpus =
        Corpus::generate(&Spec::psalms(8).with_history(60, 0.1)).expect("the corpus builds");
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
            .map(|s| s.load(n))
        {
            Some(Ok(from_cache)) => {
                cached += 1;
                inputs.extend(from_cache.inputs);
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
    let index = Index::parse(Index::encode(project, &sections), project).unwrap();
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
    let index = Index::parse(Index::encode(project(), [&section]), project()).unwrap();
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
fn an_index_of_another_project_or_version_is_not_read() {
    let file = pending_of(&every_kind(), 3);
    let section = file.section(project(), 0).unwrap();
    let bytes = Index::encode(project(), [&section]);
    assert_eq!(
        Index::parse(bytes.clone(), common::other_project()).err(),
        Some(IndexError::Project)
    );
    let mut newer = bytes.clone();
    newer[8..12].copy_from_slice(&(INDEX_VERSION + 1).to_le_bytes());
    assert_eq!(
        Index::parse(newer, project()).err(),
        Some(IndexError::Version(INDEX_VERSION + 1))
    );
    let mut minor = bytes.clone();
    minor[12] ^= 1;
    assert!(matches!(
        Index::parse(minor, project()).err(),
        Some(IndexError::Minor(_))
    ));
    assert_eq!(
        Index::parse(b"not an index".to_vec(), project()).err(),
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
    let bytes = Index::encode(project(), [&section]);
    let expected = texts(&file.read(project(), 0));
    let check = |damaged: Vec<u8>, how: &str| {
        let Ok(index) = Index::parse(damaged, project()) else {
            return;
        };
        let Some(section) = index.section(&file.key()) else {
            return;
        };
        if let Ok(loaded) = section.load(0) {
            assert_eq!(texts(&loaded.inputs), expected, "{how}");
        }
    };
    let mut unused = 0;
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut damaged = bytes.clone();
            damaged[at] ^= 1 << bit;
            check(damaged.clone(), &format!("bit {bit} of byte {at}"));
            let used = Index::parse(damaged, project())
                .ok()
                .and_then(|i| i.section(&file.key()).map(|s| s.load(0).is_ok()))
                .unwrap_or(false);
            unused += usize::from(!used);
        }
    }
    assert_eq!(unused, bytes.len() * 8, "no damaged index is used");
    for len in 0..bytes.len() {
        check(bytes[..len].to_vec(), &format!("cut at {len}"));
        assert!(Index::parse(bytes[..len].to_vec(), project()).is_err());
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
            let mut valid = Index::encode(project(), [&section]);
            let keep = valid.len().min(40);
            valid.truncate(keep);
            valid.extend_from_slice(&bytes);
            valid
        } else {
            bytes
        };
        if let Ok(index) = Index::parse(input, project()) {
            for section in index.sections() {
                let _ = section.load(0);
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
    let loaded = load_pending(&file.path, &file.bytes, project(), 0);
    let Facts::Pending(facts) = &loaded.facts else {
        panic!("{:?}", loaded.facts)
    };
    assert_eq!(facts.status, PendingStatus::Valid);
    assert_eq!(loaded.inputs.len(), 2);
    assert!(
        loaded.section.is_none(),
        "a quarantined line keeps its bytes in memory"
    );
    for pack in packs_of(&lines) {
        let loaded = load_pack(&pack.path, &pack.bytes, project(), 0);
        assert!(matches!(&loaded.facts, Facts::Pack(facts) if facts.is_valid()));
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

/// The pack facts of a loaded file.
fn pack_facts(loaded: &Loaded) -> PackFacts {
    match &loaded.facts {
        Facts::Pack(facts) => facts.clone(),
        Facts::Pending(_) => panic!("a pack"),
    }
}

/// The open's rules read a file's change ids from a read and from the cache alike: the
/// redundancy rule (§4.7) keeps a pack whose replacing pack lacks one of its changes (O7
/// would otherwise remove the only copy of that change from the working tree), and O7 sees
/// every change of a pending file. A loader that moved the changes out of the facts it
/// returned gave the redundancy rule an empty pack, which every pack that lists it holds.
#[test]
fn the_open_s_rules_read_the_same_ids_from_a_read_and_from_the_cache() {
    let changes: Vec<Verified> = (1..=4)
        .map(|seq| Verified::Change(verse_change(local(1), seq, seq as u16, "words")))
        .collect();
    let spec = |at: i64| PackSpec {
        project: project(),
        sealer: local(1),
        sealed: WallTime(1_760_000_000_000 + at),
    };
    let pack_file = |pack: tc_journal::pack::EncodedPack| File {
        kind: FileKind::Pack,
        path: pack.path.to_string(),
        bytes: pack.bytes,
    };
    let replaced = seal(&spec(0), &changes).unwrap().packs.remove(0);
    let name = replaced.path.name;
    let replaced = pack_file(replaced);
    let lacking = pack_file(encode_pack(&spec(1), &changes[..3], &[name]).unwrap());
    let holding = pack_file(encode_pack(&spec(2), &changes, &[name]).unwrap());
    let mut ids: Vec<ChangeId> = changes.iter().map(Verified::id).collect();
    ids.sort();
    for (replacing, redundant) in [(lacking, false), (holding, true)] {
        let files = [replaced.clone(), replacing];
        let loaded: Vec<Loaded> = (0..2).map(|n| files[n].load(project(), n as u32)).collect();
        let index = Index::parse(
            Index::encode(project(), loaded.iter().filter_map(|l| l.section.as_ref())),
            project(),
        )
        .unwrap();
        let from_reads: Vec<PackFacts> = loaded.iter().map(pack_facts).collect();
        let from_cache: Vec<PackFacts> = (0..2)
            .map(|n| {
                let section = index.section(&files[n].key()).unwrap();
                pack_facts(&section.load(n as u32).unwrap())
            })
            .collect();
        assert_eq!(from_reads[0].ids, ids);
        assert_eq!(from_reads, from_cache);
        for facts in [&from_reads, &from_cache] {
            let result = redundancy(facts);
            if redundant {
                assert_eq!(result.redundant, vec![(0, 1)]);
                assert!(result.kept.is_empty());
            } else {
                assert!(result.redundant.is_empty(), "{result:?}");
                assert_eq!(result.kept.len(), 1);
                assert_eq!(result.kept[0].missing, vec![changes[3].id()]);
            }
        }
    }

    let pending = pending_of(&changes, 3);
    let loaded = pending.load(project(), 0);
    let section = loaded.section.clone().unwrap();
    for facts in [loaded.facts, section.load(0).unwrap().facts] {
        let Facts::Pending(facts) = facts else {
            panic!("a pending file")
        };
        assert_eq!(facts.ids, ids);
        assert_eq!(facts.header.unwrap().device, local(3));
    }
}

/// The batch of an outside edit names the file it came from in its header (`reconciles`, §4.2),
/// and `metadata.json` is such a file though no ingredient may be: the pending file the app
/// wrote verifies when it is read again, and so does its section of the cache. A header that
/// read the path as an ingredient's made the app's own file damaged, and a seal could not take it.
#[test]
fn a_pending_file_that_reconciles_metadata_json_verifies_and_caches_like_any_other() {
    let changes = [Verified::Change(verse_change(local(1), 1, 1, "words"))];
    for path in ["metadata.json", "ingredients/TIT.usfm"] {
        let reconciles = Some(Reconciles {
            path: RepoPath::parse_source(path).unwrap(),
            seen_md5: Md5::of(b"the bytes found"),
        });
        let encoded = encode_pending(
            &PendingHeader {
                device: local(1),
                project: project(),
                purpose: Purpose::Intake,
                reconciles: reconciles.clone(),
            },
            &changes,
        )
        .unwrap();
        let read = read_pending(&encoded.bytes, project());
        assert_eq!(read.status, PendingStatus::Valid, "{path}");
        assert_eq!(read.header.unwrap().reconciles, reconciles, "{path}");
        let file = File {
            kind: FileKind::Pending,
            path: format!(".tc/pending/{}", encoded.name),
            bytes: encoded.bytes,
        };
        let loaded = file.load(project(), 0);
        let section = loaded.section.clone().expect("the file can be cached");
        let index = Index::parse(Index::encode(project(), [&section]), project()).unwrap();
        assert_same_inputs(&file, index.section(&file.key()).unwrap(), 0);
        let Facts::Pending(facts) = loaded.facts else {
            panic!("a pending file")
        };
        assert_eq!(facts.status, PendingStatus::Valid, "{path}");
    }
}

/// The changes of the probe (`tc_journal::index::PROBE`): the pinned change of every record
/// kind and the values [`every_kind`] adds, each change again under the cause `Seed` (or
/// `Edit`, for a seed), and a verse of every value class, seeded and edited. The fold's
/// summary tells these classes apart (a seed stub is a `Seed` change on a verse with no
/// words, `___`, a span's covered verse or an absent one, a header, a chapter front or a
/// layout, §7.3), so a build that summarizes any class otherwise makes other sections.
fn probe_changes() -> Vec<Verified> {
    use support::{Dev, absent, covered, text, text_from, verse};
    let mut out = every_kind();
    for line in every_kind() {
        let Verified::Change(change) = &line else {
            panic!("{line:?}")
        };
        let Json::Obj(mut map) = json::parse(change.raw()).unwrap() else {
            panic!("a change is an object")
        };
        let other = if matches!(change.body().cause, Cause::Seed) {
            r#"{"edit":{}}"#
        } else {
            r#"{"seed":{}}"#
        };
        map.insert("cause".into(), json::parse(other.as_bytes()).unwrap());
        let text = Json::Obj(map).to_canonical_string();
        let again = parse_change(ChangeId::of(text.as_bytes()), Arc::from(text.as_bytes()));
        assert!(matches!(again, Verified::Change(_)), "{again:?}");
        out.push(again);
    }
    let mut dev = Dev::new(9);
    for cause in [Cause::Seed, Cause::Edit] {
        for (v, value) in [
            (1, text("", 1)),
            (2, text("___", 2)),
            (3, text("___ words", 3)),
            (4, text("words", 5)),
            (6, text_from("moved", 6, (2, 3))),
            (7, covered(4)),
            (8, absent(None)),
            (9, absent(Some((3, 1)))),
        ] {
            let record = verse(5, v, value);
            out.push(Verified::Change(dev.with(record, vec![], cause.clone())));
        }
    }
    out
}

/// The probe as the library holds it, read.
fn probe() -> Vec<Verified> {
    let read = read_pending(PROBE, ProjectId::from_bytes(PROBE_PROJECT));
    assert_eq!(read.status, PendingStatus::Valid);
    read.entries
}

/// The probe's bytes are what [`probe_changes`] gives. `TC_WRITE_PROBE=1` writes them; the
/// stamp then changes, so every cache an earlier build wrote is read again once.
#[test]
fn the_probe_is_what_the_generator_gives() {
    let encoded = encode_pending(
        &PendingHeader {
            device: local(3),
            project: ProjectId::from_bytes(PROBE_PROJECT),
            purpose: Purpose::Save,
            reconciles: None,
        },
        &probe_changes(),
    )
    .unwrap();
    if std::env::var_os("TC_WRITE_PROBE").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/index_probe.batch");
        std::fs::write(path, &encoded.bytes).unwrap();
        return;
    }
    assert!(
        PROBE == encoded.bytes.as_slice(),
        "the probe differs from the generator's: run with TC_WRITE_PROBE=1"
    );
}

/// The class of a change that the summary's rules tell apart: its record's kind, whether its
/// cause is `Seed`, and for a verse its value's class.
fn class_of(line: &Verified) -> (String, bool, &'static str) {
    let Verified::Change(change) = line else {
        panic!("{line:?}")
    };
    let body = change.body();
    let kind: String = format!("{:?}", body.record)
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    let value = match &body.record {
        Record::Verse { value, .. } => match value {
            VerseValue::Text { words, .. } if words.is_empty() => "no words",
            VerseValue::Text { words, .. } if words == "___" => "___",
            VerseValue::Text { .. } => "words",
            VerseValue::Covered { .. } => "covered",
            VerseValue::Absent { .. } => "absent",
        },
        _ => "",
    };
    (kind, matches!(body.cause, Cause::Seed), value)
}

/// The probe holds every class the summary tells apart: every record kind seeded and not,
/// and every verse value class seeded and not. A build whose stub rule drops a class (a
/// verse with no words, a layout) then makes other sections of the probe.
#[test]
fn the_probe_covers_every_class() {
    let classes: std::collections::BTreeSet<(String, bool, &str)> =
        probe().iter().map(class_of).collect();
    let mut missing = Vec::new();
    for kind in [
        "BookPresence",
        "BookHeader",
        "ChapterFront",
        "Verse",
        "BookLayout",
        "Alignment",
        "Check",
        "Comment",
        "Pins",
        "ResourcesExtra",
        "Setting",
        "ProjectMeta",
        "Versification",
        "DeviceProfile",
        "StoryBase",
        "Frame",
        "StoryRef",
        "CarriedFile",
        "BatchRelease",
        "Unknown",
    ] {
        let values: &[&str] = if kind == "Verse" {
            &["no words", "___", "words", "covered", "absent"]
        } else {
            &[""]
        };
        for seed in [true, false] {
            for value in values {
                if !classes.contains(&(kind.to_string(), seed, *value)) {
                    missing.push((kind, seed, *value));
                }
            }
        }
    }
    assert!(missing.is_empty(), "not in the probe: {missing:?}");
}

/// The probe's changes give every field of the summary a prepared change keeps every value
/// its type has (each cause, each shape and its parts, seed stubs and changes that are not,
/// no parent, one and several, times before and after 1970, single and multi-change batches,
/// every kind of key), so a change to how any of them is kept or written changes its sections.
#[test]
fn the_probe_covers_the_summary() {
    let file = pending_of(&probe(), 3);
    let text = format!("{:?}", file.read(project(), 0));
    let mut missing = Vec::new();
    for needle in [
        // Causes, and the seed stub flag.
        "cause: Edit",
        "cause: Resolve",
        "cause: Restore",
        "cause: CarryOver",
        "unknown_base: false",
        "unknown_base: true",
        "cause: Import",
        "cause: Seed",
        "cause: StartOver",
        "cause: System",
        "cause: Newer",
        "target: Check(",
        "stub: true",
        "stub: false",
        // Shapes.
        "shape: Other",
        "Text {",
        "from: None",
        "from: Some(",
        "Covered {",
        "Absent {",
        "moved_to: None",
        "moved_to: Some(",
        "present: true",
        "present: false",
        "Presence(Some(",
        "Presence(None)",
        "Added",
        "RestoredText",
        "StartedEmpty",
        "Alignment {",
        "Decision {",
        "MovedTo {",
        "Comment {",
        "Carried {",
        "oid: None",
        "oid: Some(",
        "Release {",
        "dismissed: true",
        "dismissed: false",
        // Keys.
        "BookPresence(",
        "BookHeader(",
        "ChapterFront(",
        "Verse(",
        "BookLayout(",
        "Check(",
        "Comment(",
        "Pins(",
        "Setting(",
        "ProjectMeta(",
        "Versification",
        "DeviceProfile(",
        "StoryBase(",
        "Frame(",
        "StoryRef(",
        "CarriedFile(",
        "BatchRelease(",
        "Unknown(",
    ] {
        if !text.contains(needle) {
            missing.push(needle);
        }
    }
    assert!(missing.is_empty(), "not covered: {missing:?}");
}

/// The build's [`stamp`], and the digest of the index of the probe's changes in a pending
/// file and in packs. Each encoding a released build wrote is pinned here with its
/// `INDEX_VERSION`, newest last. A change to what a prepared change keeps of any class the
/// probe holds, or to how the cache writes it, changes both: raise the version and add a
/// row, never edit one. Builds are kept apart by the stamp whether or not this is done; the
/// test makes the change seen.
#[test]
fn the_encoding_is_pinned() {
    const PINNED: &[(u32, u64, &str)] = &[
        (
            5,
            0x45f9_9a8a_0cf9_8153,
            "c347a50b9c98c4dfd86bee2bdaa44ce6e187b9b26fddfae4e7b295cb162283b5",
        ),
        (
            6,
            0xe9be_ed65_457e_0014,
            "c6e74a64914c1415dec7298408188f0afe55b72c5cb30678ef83eed564b7070a",
        ),
    ];
    let changes = probe();
    let mut files = packs_of(&changes);
    files.push(pending_of(&changes, 3));
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| f.section(project(), n as u32).unwrap())
        .collect();
    let bytes = Index::encode(project(), &sections);
    let current = (stamp(), PackId::of(&bytes).to_hex());
    assert!(
        PINNED.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "versions only grow"
    );
    let (newest, ..) = *PINNED.last().unwrap();
    assert_eq!(newest, INDEX_VERSION, "pin the encoding of INDEX_VERSION");
    for (version, stamp, digest) in PINNED {
        if (*stamp, *digest) == (current.0, current.1.as_str()) {
            assert_eq!(
                *version, INDEX_VERSION,
                "this encoding was pinned for version {version}"
            );
        }
    }
    let (_, stamp, digest) = *PINNED.last().unwrap();
    assert_eq!(
        (stamp, digest),
        (current.0, current.1.as_str()),
        "the cache's bytes changed: raise INDEX_VERSION to {} and pin ({:#x}, {:?}) in a new row",
        INDEX_VERSION + 1,
        current.0,
        current.1
    );
}

/// The stamp is what this build makes of the probe: every index it writes carries it, and an
/// index with another stamp, as a build that summarizes the probe otherwise writes, is not
/// read.
#[test]
fn an_index_of_a_build_that_summarizes_otherwise_is_not_read() {
    let file = pending_of(&every_kind(), 3);
    let section = file.section(project(), 0).unwrap();
    let bytes = Index::encode(project(), [&section]);
    assert_eq!(bytes[14..22], stamp().to_le_bytes());
    assert!(Index::parse(bytes.clone(), project()).is_ok());
    for bit in 0..64 {
        let mut other = bytes.clone();
        other[14 + bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            Index::parse(other, project()).err(),
            Some(IndexError::Stamp),
            "bit {bit}"
        );
    }
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
    let forward = Index::encode(project(), &sections);
    let backward = Index::encode(project(), sections.iter().rev());
    assert_eq!(forward, backward);
    // An index written from the sections of a parsed index is the same index.
    let index = Index::parse(forward.clone(), project()).unwrap();
    assert_eq!(Index::encode(project(), index.sections()), forward);
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

// ---------------------------------------------------------------------------------------
// Opens with a cache that no longer stands for the files
// ---------------------------------------------------------------------------------------

/// What an open of `files` gives: the fold, whether each file came from the cache, and the
/// facts of each file, which the open's rules and notices read.
struct Opened {
    fold: FoldState,
    from_cache: Vec<bool>,
    reports: Vec<Facts>,
}

fn open_with(files: &[File], project: ProjectId, index: Option<&Index>) -> Opened {
    let mut inputs = Vec::new();
    let mut from_cache = Vec::new();
    let mut reports = Vec::new();
    for (n, file) in files.iter().enumerate() {
        let n = n as u32;
        let cached = index
            .and_then(|i| i.section(&file.key()))
            .and_then(|s| s.load(n).ok());
        from_cache.push(cached.is_some());
        let loaded = cached.unwrap_or_else(|| file.load(project, n));
        inputs.extend(loaded.inputs);
        reports.push(loaded.facts);
    }
    let source: Arc<dyn ValueSource> =
        Arc::new(Files(files.iter().map(|f| f.bytes.clone()).collect()));
    Opened {
        fold: FoldState::fold_from(ProjectKind::Bible, Some(source), inputs),
        from_cache,
        reports,
    }
}

/// The index `index_bytes` (none when it does not parse) gives the open of `files` exactly
/// what an open without a cache gives: the same fold, and every file reported as reading it
/// reports it. Returns which files came from the cache.
fn opens_as_without(
    files: &[File],
    project: ProjectId,
    index_bytes: &[u8],
    how: &str,
) -> Vec<bool> {
    let index = Index::parse(index_bytes.to_vec(), project).ok();
    let read = open_with(files, project, None);
    let cached = open_with(files, project, index.as_ref());
    support::same_fold(&read.fold, &cached.fold, how);
    assert_eq!(
        read.reports, cached.reports,
        "{how}: what each file reports"
    );
    cached.from_cache
}

fn index_bytes_of(files: &[File], project: ProjectId) -> Vec<u8> {
    let sections: Vec<Section> = files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            f.section(project, n as u32)
                .expect("the file can be cached")
        })
        .collect();
    Index::encode(project, &sections)
}

/// A small project: a seeded Titus 1 in a pack, and two devices' concurrent edits of one
/// verse in pending files, so the fold holds a conflict and projects every derived file.
fn small_project() -> Vec<File> {
    small_project_with("one side")
}

/// [`small_project`] whose first edit has the words `words`.
fn small_project_with(words: &str) -> Vec<File> {
    use support::{Dev, seed_book, text, verse};
    let mut one = Dev::new(1);
    let mut two = Dev::new(2);
    let seed = seed_book(&mut one, &[3]);
    let base = support::on(&seed, &support::verse_key(1, 2)).id();
    let a = one.one(verse(1, 2, text(words, 2)), vec![base]);
    let b = two.one(verse(1, 2, text("other side", 2)), vec![base]);
    let lines: Vec<Verified> = seed.into_iter().map(Verified::Change).collect();
    let mut files = packs_of(&lines);
    files.push(pending_of(&[Verified::Change(a)], 1));
    files.push(pending_of(&[Verified::Change(b)], 2));
    files
}

/// A changed byte anywhere in a file (every byte of every file of a small project): the
/// file is read, reported as damaged and salvaged exactly as without the cache, and every
/// other file still comes from the cache.
#[test]
fn a_changed_byte_anywhere_opens_as_without_the_cache() {
    let files = small_project();
    let index = index_bytes_of(&files, project());
    let opened = open_with(
        &files,
        project(),
        Index::parse(index.clone(), project()).ok().as_ref(),
    );
    assert!(opened.from_cache.iter().all(|c| *c));
    assert!(opened.fold.conflict_count() > 0, "the edits conflict");
    for (n, file) in files.iter().enumerate() {
        for at in 0..file.bytes.len() {
            let mut changed = files.clone();
            changed[n].bytes[at] ^= 0x01;
            let how = format!("{} byte {at}", file.path);
            let from_cache = opens_as_without(&changed, project(), &index, &how);
            assert!(!from_cache[n], "{how}: the changed file is read");
            assert_eq!(
                from_cache.iter().filter(|c| **c).count(),
                files.len() - 1,
                "{how}"
            );
        }
    }
}

/// A file cut short at every length, and a pending file that grows: the size differs, so the
/// file is read.
#[test]
fn a_truncated_or_grown_file_opens_as_without_the_cache() {
    let files = small_project();
    let index = index_bytes_of(&files, project());
    for (n, file) in files.iter().enumerate() {
        for len in 0..file.bytes.len() {
            let mut cut = files.clone();
            cut[n].bytes.truncate(len);
            let how = format!("{} cut to {len} bytes", file.path);
            assert!(!opens_as_without(&cut, project(), &index, &how)[n], "{how}");
        }
    }
    let pending = files
        .iter()
        .position(|f| f.kind == FileKind::Pending)
        .unwrap();
    // Another program appends a line that verifies, then one that does not.
    let extra = verse_change(local(7), 1, 3, "appended");
    let mut grown = files.clone();
    grown[pending]
        .bytes
        .extend_from_slice(&tc_journal::line::encode_line(extra.id(), extra.raw()));
    assert!(!opens_as_without(&grown, project(), &index, "a line appended")[pending]);
    grown[pending].bytes.extend_from_slice(b"not a line");
    assert!(!opens_as_without(&grown, project(), &index, "bytes appended")[pending]);
}

/// A file replaced by another of exactly the same size at the same path: a pending file by
/// another save, and a pack by another pack (whose name then does not match its path).
#[test]
fn a_file_replaced_by_one_of_the_same_size_opens_as_without_the_cache() {
    let files = small_project();
    let index = index_bytes_of(&files, project());
    let pending = files
        .iter()
        .position(|f| f.kind == FileKind::Pending)
        .unwrap();
    let mut replaced = files.clone();
    // The other save differs from the first in its words alone, and has their length.
    let other = small_project_with("ONE SIDE");
    assert_eq!(other[pending].bytes.len(), files[pending].bytes.len());
    assert_ne!(other[pending].bytes, files[pending].bytes);
    replaced[pending].bytes = other[pending].bytes.clone();
    assert!(!opens_as_without(&replaced, project(), &index, "another save")[pending]);

    let pack = files.iter().position(|f| f.kind == FileKind::Pack).unwrap();
    let lines =
        tc_journal::pack::read_pack(&files[pack].path, &files[pack].bytes, project()).entries;
    let resealed = seal(
        &PackSpec {
            project: project(),
            sealer: local(1),
            sealed: WallTime(1_760_000_000_001),
        },
        &lines,
    )
    .unwrap()
    .packs
    .remove(0);
    assert_eq!(resealed.bytes.len(), files[pack].bytes.len());
    assert_ne!(resealed.bytes, files[pack].bytes);
    let mut replaced = files.clone();
    replaced[pack].bytes = resealed.bytes;
    let from_cache = opens_as_without(&replaced, project(), &index, "another pack");
    assert!(!from_cache[pack]);
}

/// Files added, removed and renamed since the cache was written: a new file is read, a
/// gone file is never asked for, and a file at a new path is read at that path.
#[test]
fn added_removed_and_renamed_files_open_as_without_the_cache() {
    let files = small_project();
    let index = index_bytes_of(&files, project());
    let pending = files
        .iter()
        .position(|f| f.kind == FileKind::Pending)
        .unwrap();
    let pack = files.iter().position(|f| f.kind == FileKind::Pack).unwrap();

    let mut added = files.clone();
    added.push(pending_of(
        &[Verified::Change(verse_change(
            local(5),
            1,
            1,
            "a later save",
        ))],
        5,
    ));
    let from_cache = opens_as_without(&added, project(), &index, "a file added");
    assert_eq!(from_cache, [vec![true; files.len()], vec![false]].concat());

    for gone in 0..files.len() {
        let mut removed = files.clone();
        removed.remove(gone);
        let from_cache = opens_as_without(&removed, project(), &index, "a file removed");
        assert!(from_cache.iter().all(|c| *c));
    }

    let mut renamed = files.clone();
    renamed[pending].path = ".tc/pending/a save (conflicted copy).batch".into();
    let month = PackPath::parse(&files[pack].path).unwrap();
    renamed[pack].path = PackPath {
        month: YearMonth::new(2020, 1).unwrap(),
        ..month
    }
    .to_string();
    let from_cache = opens_as_without(&renamed, project(), &index, "files renamed");
    assert!(!from_cache[pending] && !from_cache[pack]);
    let opened = open_with(&renamed, project(), None);
    assert!(
        matches!(&opened.reports[pack], Facts::Pack(facts) if facts.is_valid()),
        "a pack moved to another month is valid"
    );
}

/// A consolidation replaces two packs with one that lists them in `replaces` (§4.7): the
/// new pack is read, and the replaced packs, removed or still there, open as without.
#[test]
fn a_consolidation_opens_as_without_the_cache() {
    let files = corpus_files();
    let project = project_of(&files);
    let index = index_bytes_of(&files, project);
    let mut packs: Vec<(usize, tc_journal::pack::PackRead)> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.kind == FileKind::Pack)
        .map(|(n, f)| (n, tc_journal::pack::read_pack(&f.path, &f.bytes, project)))
        .collect();
    // Two packs of one sealer.
    let sealer = packs[0].1.header.as_ref().unwrap().sealer;
    packs.retain(|(_, p)| p.header.as_ref().unwrap().sealer == sealer);
    assert!(packs.len() >= 2, "the corpus has two packs of one sealer");
    let (a, b) = (&packs[0], &packs[1]);
    let merged = consolidate(
        &PackSpec {
            project,
            sealer,
            sealed: WallTime(1_900_000_000_000),
        },
        &[&a.1, &b.1],
    )
    .unwrap();
    assert_eq!(merged.len(), 1);
    let new = File {
        kind: FileKind::Pack,
        path: merged[0].path.to_string(),
        bytes: merged[0].bytes.clone(),
    };
    let mut both = files.clone();
    both.push(new.clone());
    let from_cache = opens_as_without(&both, project, &index, "consolidated, old packs kept");
    assert_eq!(from_cache.iter().filter(|c| !**c).count(), 1);
    let mut replaced: Vec<File> = files
        .iter()
        .enumerate()
        .filter(|(n, _)| *n != a.0 && *n != b.0)
        .map(|(_, f)| f.clone())
        .collect();
    replaced.push(new);
    opens_as_without(
        &replaced,
        project,
        &index,
        "consolidated, old packs removed",
    );
}

/// A damaged, truncated, foreign, older or newer `index.bin` is not used, or loses only the
/// damaged sections: the open is the open without a cache.
#[test]
fn a_damaged_or_foreign_index_opens_as_without_it() {
    let files = small_project();
    let index = index_bytes_of(&files, project());
    for at in (0..index.len()).step_by(7) {
        let mut damaged = index.clone();
        damaged[at] ^= 0x10;
        opens_as_without(&files, project(), &damaged, &format!("index byte {at}"));
    }
    for len in (0..index.len()).step_by(11) {
        let from_cache = opens_as_without(&files, project(), &index[..len], &format!("cut {len}"));
        assert!(
            from_cache.iter().all(|c| !*c),
            "a truncated index is not read"
        );
    }
    let foreign = {
        let sections: Vec<Section> = files
            .iter()
            .enumerate()
            .map(|(n, f)| f.section(project(), n as u32).unwrap())
            .collect();
        Index::encode(common::other_project(), &sections)
    };
    assert!(
        opens_as_without(&files, project(), &foreign, "another project's")
            .iter()
            .all(|c| !*c)
    );
    for version in [INDEX_VERSION - 1, INDEX_VERSION + 1] {
        let mut other = index.clone();
        other[8..12].copy_from_slice(&version.to_le_bytes());
        let how = format!("version {version}");
        assert!(
            opens_as_without(&files, project(), &other, &how)
                .iter()
                .all(|c| !*c)
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A changed byte anywhere in a file of the corpus of the operation-count tests.
    #[test]
    fn a_changed_byte_in_the_corpus_opens_as_without_the_cache(
        pick in any::<prop::sample::Index>(),
        at in any::<prop::sample::Index>(),
        xor in 1u8..,
    ) {
        let (files, project, index) = corpus_with_index();
        let project = *project;
        let n = pick.index(files.len());
        let mut changed = files.clone();
        let at = at.index(changed[n].bytes.len());
        changed[n].bytes[at] ^= xor;
        let from_cache =
            opens_as_without(&changed, project, index, &format!("{} byte {at}", files[n].path));
        prop_assert!(!from_cache[n]);
    }
}

fn corpus_with_index() -> &'static (Vec<File>, ProjectId, Vec<u8>) {
    static CORPUS: std::sync::OnceLock<(Vec<File>, ProjectId, Vec<u8>)> =
        std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let files = corpus_files();
        let project = project_of(&files);
        let index = index_bytes_of(&files, project);
        (files, project, index)
    })
}
