//! Importing translationCore 3 projects, against tC4's own import fixtures
//! (`fixtures/tc4/conformance-fixtures/import/`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_core::BookId;
use tc_import::tc3::{DecisionPins, read};
use tc_import::{ImportRefusal, SourceFile};
use tc_journal::fold::{DerivedFile, ProjectKind};
use tc_journal::keys::ToolId;
use tc_journal::record::ResourceRef;
use tc_journal::{Cause, GitOid, ImportSource, Record};
use tc_store::fault::FaultFs;
use tc_store::{FakePlatform, ManualClock, NewProject, OpenOptions, Store, StoreConfig};

fn import_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc4/conformance-fixtures/import")
}

fn source(rel: &str) -> SourceFile {
    SourceFile {
        name: rel.rsplit('/').next().unwrap().to_owned(),
        bytes: std::fs::read(import_dir().join(rel)).unwrap(),
    }
}

/// A folder of the fixtures zipped as a person would zip it.
fn zipped(rel: &str) -> SourceFile {
    let root = import_dir().join(rel);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                writer
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
        }
    }
    SourceFile {
        name: format!("{rel}.zip"),
        bytes: writer.finish().unwrap().into_inner(),
    }
}

fn pins() -> DecisionPins {
    let pin = |repo: &str, n: u8| ResourceRef {
        repo_path: repo.into(),
        sha: GitOid::parse(&format!("{n:040x}")).unwrap(),
    };
    DecisionPins {
        translation_notes: pin("git.door43.org/unfoldingWord/en_tn", 1),
        translation_words: pin("git.door43.org/unfoldingWord/en_twl", 2),
    }
}

fn chapters(usfm: &str) -> usize {
    tc_usfm::codec::decompose(usfm).unwrap().chapters.len()
}

/// Every tC3 case of tC4's import MANIFEST imports with tC4's language, books and chapter
/// counts, or refuses with tC4's code. The other parsers' cases are cut from the first
/// release (rulings/2026-10-24-H3-H4-H7-fast-release.md).
#[test]
fn every_tc3_manifest_case_imports_or_refuses_as_tc4_does() {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(import_dir().join("MANIFEST.json")).unwrap())
            .unwrap();
    let mut checked = 0;
    for case in manifest
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["parser"] == "tc3")
    {
        let names: Vec<&str> = match &case["file"] {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().map(|v| v.as_str().unwrap()).collect(),
            _ => panic!("unexpected"),
        };
        let files: Vec<SourceFile> = names
            .iter()
            .map(|n| {
                if import_dir().join(n).is_dir() {
                    zipped(n)
                } else {
                    source(n)
                }
            })
            .collect();
        let at = format!("{names:?}");
        match case["expect"].as_str().unwrap() {
            "accept" => {
                let import = read(&files).unwrap_or_else(|e| panic!("{at}: {e}"));
                assert_eq!(import.language, case["language"], "{at}");
                let books: Vec<&str> = import.books.iter().map(|b| b.book.code()).collect();
                let wanted: Vec<&str> = case["books"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b.as_str().unwrap())
                    .collect();
                assert_eq!(books, wanted, "{at}");
                for book in &import.books {
                    assert_eq!(
                        chapters(&book.usfm) as u64,
                        case["counts"]["chapters"][book.book.code()]
                            .as_u64()
                            .unwrap(),
                        "{at} {}",
                        book.book.code()
                    );
                }
                import
                    .records(&pins())
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                let not_kept: Vec<&String> =
                    import.books.iter().flat_map(|b| &b.not_kept).collect();
                if names.len() == 3 {
                    // One tC3 note decision checks every occurrence; no check list holds it.
                    assert_eq!(not_kept.len(), 1, "{not_kept:?}");
                } else {
                    assert!(not_kept.is_empty(), "{at}: {not_kept:?}");
                }
            }
            "refuse" => {
                let refusal = read(&files)
                    .err()
                    .unwrap_or_else(|| panic!("{at} imported"));
                assert_eq!(refusal.code(), case["code"].as_str(), "{at}: {refusal}");
            }
            other => panic!("{other}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// tC4's golden facts for the Titus fixture: 46 alignment records, none invalid, and the 5
/// decisions that hold the translator's work.
#[test]
fn a_tc3_project_reads_its_alignments_and_decisions() {
    let import = read(&[source("tc3/cfm_fbt_tit_book.zip")]).unwrap();
    assert_eq!(import.name, "cfm_fbt");
    assert_eq!(import.licenses, ["CC BY-SA 4.0"]);
    let titus = &import.books[0];
    assert_eq!(titus.book, BookId::Titus);
    assert_eq!(titus.alignments().len(), 46);
    assert!(titus.alignments().iter().all(|(_, _, v)| !v.marked_invalid));
    let decisions = titus.decision_count(ToolId::TranslationNotes)
        + titus.decision_count(ToolId::TranslationWords);
    assert_eq!(decisions, 5);
    let records = import.records(&pins()).unwrap();
    let checks = records
        .iter()
        .filter(|r| matches!(r, Record::Check { .. }))
        .count();
    assert_eq!(checks, 5);
}

/// tC4 #796 (e79a2795): a tC3 alignment that holds a link whose words are no longer the
/// verse's words (the text was edited outside tC3) is marked invalid, and kept as it was.
#[test]
fn a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid() {
    // Luke 1 holds a linked record; Titus holds none.
    let original = source("tc3/multi/en_kjv_luk_book.zip");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&original.bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "luk/1.json" || name.ends_with("/luk/1.json") && !name.contains("apps/") {
            let mut verses: Value = serde_json::from_slice(&bytes).unwrap();
            for (_, text) in verses.as_object_mut().unwrap() {
                *text = Value::from("Edited outside translationCore");
            }
            bytes = serde_json::to_vec(&verses).unwrap();
        }
        // tC3's own invalid markers go, so only the words can mark a record.
        if entry.is_dir() || name.contains("tools/wordAlignment/invalid/") {
            continue;
        }
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let edited = SourceFile {
        name: original.name,
        bytes: writer.finish().unwrap().into_inner(),
    };
    let linked: Vec<String> = read(&[source("tc3/multi/en_kjv_luk_book.zip")])
        .unwrap()
        .books[0]
        .alignments()
        .iter()
        .filter(|(c, _, v)| c.get() == 1 && v.alignments.iter().any(|g| !g.bottom_words.is_empty()))
        .map(|(c, v, _)| format!("{c}:{v}"))
        .collect();
    assert!(!linked.is_empty());
    let import = read(&[edited]).unwrap();
    let invalid: Vec<String> = import.books[0]
        .alignments()
        .iter()
        .filter(|(_, _, v)| v.marked_invalid)
        .map(|(c, v, _)| format!("{c}:{v}"))
        .collect();
    assert_eq!(
        invalid, linked,
        "every linked record of the edited chapter, and only those"
    );
}

/// `original` with each `(name, bytes)` written over the entry of that name, or added.
fn with_entries(original: &SourceFile, entries: &[(&str, &[u8])]) -> SourceFile {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&original.bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        if entry.is_dir() || entries.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        writer.start_file(name, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    SourceFile {
        name: original.name.clone(),
        bytes: writer.finish().unwrap().into_inner(),
    }
}

/// A tC3 alignment that holds links but no verse of the text can hold is named in the
/// book's report, never dropped without a word; tC4 skips each of these silently (a
/// declared difference). tC3's own empty records, such as `front`, are no loss.
#[test]
fn a_tc3_alignment_with_no_verse_to_hold_it_is_reported() {
    let original = source("tc3/multi/en_kjv_luk_book.zip");
    let chapter_one = "apps/translationCore/alignmentData/luk/1.json";
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&original.bytes)).unwrap();
    let mut verses: Value = {
        let mut bytes = Vec::new();
        archive
            .by_name(chapter_one)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    };
    let linked = verses
        .as_object()
        .unwrap()
        .values()
        .find(|r| {
            r["alignments"]
                .as_array()
                .unwrap()
                .iter()
                .any(|g| !g["bottomWords"].as_array().unwrap().is_empty())
        })
        .unwrap()
        .clone();
    let empty = serde_json::json!({ "alignments": [], "wordBank": [] });
    verses["999"] = linked.clone();
    verses["front"] = empty.clone();
    verses["1a"] = linked.clone();
    verses["998"] = empty;
    let chapter_one_bytes = serde_json::to_vec(&verses).unwrap();
    let far_chapter = serde_json::to_vec(&serde_json::json!({ "1": linked })).unwrap();
    let before = read(std::slice::from_ref(&original)).unwrap().books[0]
        .alignments()
        .len();
    let edited = with_entries(
        &original,
        &[
            (chapter_one, &chapter_one_bytes),
            (
                "apps/translationCore/alignmentData/luk/70000.json",
                &far_chapter,
            ),
            ("apps/translationCore/alignmentData/luk/30.json", b"[]"),
        ],
    );
    let import = read(&[edited]).unwrap();
    let luke = &import.books[0];
    assert_eq!(
        luke.alignments().len(),
        before,
        "the verses the text holds still import"
    );
    let mut not_kept: Vec<&str> = luke
        .not_kept
        .iter()
        .map(String::as_str)
        .filter(|n| n.starts_with("wordAlignment"))
        .collect();
    not_kept.sort_unstable();
    assert_eq!(
        not_kept,
        [
            "wordAlignment LUK 1:1a (no such verse in the text)",
            "wordAlignment LUK 1:999 (no such verse in the text)",
            "wordAlignment LUK 30 (not a chapter of verses)",
            "wordAlignment LUK 70000 (no such chapter)",
        ]
    );
}

fn store(fs: &FaultFs) -> Store {
    let store = Store::start(StoreConfig {
        app_data: PathBuf::from("/app"),
        projects: PathBuf::from("/projects"),
        vfs: Arc::new(fs.clone()),
        platform: Arc::new(FakePlatform::default()),
        clock: Arc::new(ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    store
}

/// INV-SCRIPTURE-BYTES-UNCHANGED and INV-IMPORT-IS-WHOLE-OR-ABSENT: the imported project is
/// made whole in one create, its book file is tC3's text byte for byte, it is a valid
/// Scripture Burrito, and every record says it came from tC3.
#[test]
fn a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project() {
    let import = read(&[source("tc3/cfm_fbt_tit_book.zip")]).unwrap();
    let records = import.records(&pins()).unwrap();
    let fs = FaultFs::new();
    let store = store(&fs);
    let folder = store
        .create_imported(
            &NewProject {
                name: import.name.clone(),
                kind: ProjectKind::Bible,
                records,
                books: Vec::new(),
            },
            Cause::Import(ImportSource::Tc3 { user: None }),
        )
        .unwrap();
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let fold = handle.snapshot().fold().clone();
    let text = fold.render(&DerivedFile::Usfm(BookId::Titus)).unwrap();
    assert_eq!(String::from_utf8(text).unwrap(), import.books[0].usfm);
    for id in fold.history(&tc_journal::RecordKey::BookPresence(BookId::Titus)) {
        assert_eq!(
            fold.body(&id).unwrap().cause,
            Cause::Import(ImportSource::Tc3 { user: None })
        );
    }
    handle.close().unwrap();
    let mut project = tc_burrito::MemProject::new();
    for (path, bytes) in fs.files_under(&folder) {
        let rel = path
            .strip_prefix(&folder)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.starts_with(".tc/") {
            project.insert(rel, bytes);
        }
    }
    let metadata = project.get("metadata.json").unwrap().to_vec();
    let problems = tc_burrito::validate_project(&metadata, &project);
    assert!(problems.is_empty(), "{problems:#?}");
    let _ = ImportRefusal::NoText {
        file: String::new(),
    };
}

/// INV-IMPORT-IS-WHOLE-OR-ABSENT under a kill at every write point: after the next start the
/// projects folder holds no project and no staging folder, or the whole imported project,
/// valid, with tC3's text.
#[test]
fn crash_import_leaves_no_project_or_a_whole_one() {
    let import = read(&[source("tc3/cfm_fbt_tit_book.zip")]).unwrap();
    let records = import.records(&pins()).unwrap();
    let fs = FaultFs::new();
    drop(store(&fs));
    let base = fs.crash(false);
    let mut k = 0;
    loop {
        let fs = base.copy();
        let store1 = store(&fs);
        fs.kill_before(k, false);
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store1.create_imported(
                &NewProject {
                    name: import.name.clone(),
                    kind: ProjectKind::Bible,
                    records: records.clone(),
                    books: Vec::new(),
                },
                Cause::Import(ImportSource::Tc3 { user: None }),
            )
        }));
        let dead = fs.is_dead();
        drop(store1);
        let after = fs.crash(true);
        let store2 = store(&after);
        let projects = Path::new("/projects");
        let left: Vec<String> = after
            .files_under(projects)
            .keys()
            .map(|p| {
                p.strip_prefix(projects)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            !left.iter().any(|p| p.starts_with(".staging-")),
            "the start cleanup removes the staging folder (N1): {left:?}"
        );
        let target = projects.join(&import.name);
        if after
            .peek(&target.join("ingredients/tc/project.json"))
            .is_some()
        {
            let handle = store2.open(&target, &OpenOptions::default()).unwrap();
            handle.wait_idle();
            let text = handle
                .snapshot()
                .fold()
                .render(&DerivedFile::Usfm(BookId::Titus));
            assert_eq!(
                text.map(|t| String::from_utf8(t).unwrap()),
                Some(import.books[0].usfm.clone())
            );
            handle.close().unwrap();
        } else {
            assert!(
                !matches!(made, Ok(Ok(_))) || !dead,
                "a reported import is there"
            );
        }
        if !dead {
            break;
        }
        k += 1;
        assert!(k < 3000);
    }
    assert!(k > 10, "{k} kill points");
}
