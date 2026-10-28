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

/// tC3's Luke with 1:1 and 1:2 bridged as `1-2` in the text and in the alignment data
/// (`fixtures/tc3/make_bridged.py`).
fn bridged_luke() -> SourceFile {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc3/en_kjv_luk_bridged.zip");
    SourceFile {
        name: "en_kjv_luk_bridged.zip".to_owned(),
        bytes: std::fs::read(path).unwrap(),
    }
}

const LUKE_ONE: &str = "apps/translationCore/alignmentData/luk/1.json";

/// The alignment data of `entry` in `zip`.
fn entry_json(zip: &SourceFile, entry: &str) -> serde_json::Map<String, Value> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&zip.bytes)).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name(entry)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    serde_json::from_slice::<Value>(&bytes)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}

/// `zip` with Luke 1's alignment data edited by `edit`.
fn with_luke_one(
    zip: &SourceFile,
    edit: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> SourceFile {
    let mut verses = entry_json(zip, LUKE_ONE);
    edit(&mut verses);
    with_entries(zip, &[(LUKE_ONE, &serde_json::to_vec(&verses).unwrap())])
}

/// Luke 1:1's record in tC3's own fixture: the one record of Luke 1 with links.
fn luke_one_one() -> Value {
    entry_json(&source("tc3/multi/en_kjv_luk_book.zip"), LUKE_ONE)["1"].clone()
}

/// The import's alignment lines of its report, sorted.
fn alignment_report(import: &tc_import::tc3::Tc3Import) -> Vec<String> {
    let mut lines: Vec<String> = import.books[0]
        .not_kept
        .iter()
        .filter(|n| n.starts_with("wordAlignment"))
        .cloned()
        .collect();
    lines.sort_unstable();
    lines
}

/// The record imported for Luke 1:1, with the verse its span ends at.
fn luke_one_one_imported(
    import: &tc_import::tc3::Tc3Import,
) -> (u16, &tc_journal::record::AlignmentValue) {
    let (_, _, value) = import.books[0]
        .alignments()
        .iter()
        .find(|(c, v, _)| c.get() == 1 && v.get() == 1)
        .expect("Luke 1:1 holds an alignment");
    (value.span_end.get(), value)
}

/// tC3 keys a bridge's alignment by the span, as the text: it imports onto the span, its
/// words fitting the span's.
#[test]
fn a_bridged_alignment_imports_onto_the_texts_span() {
    let import = read(&[bridged_luke()]).unwrap();
    let (end, value) = luke_one_one_imported(&import);
    assert_eq!(end, 2);
    assert!(!value.marked_invalid, "the span's words fit");
    assert!(value.alignments.iter().any(|g| !g.bottom_words.is_empty()));
    assert_eq!(alignment_report(&import), Vec::<String>::new());
    import.records(&pins()).unwrap();
}

/// A record keyed by a verse the text's span covers lands on the span, as a check decision
/// does; its words are one verse's, so it is marked for review, never dropped.
#[test]
fn an_alignment_of_a_verse_inside_a_bridge_lands_on_the_span_for_review() {
    let zip = with_luke_one(&bridged_luke(), |verses| {
        verses.remove("1-2");
        verses.insert("2".to_owned(), luke_one_one());
    });
    let import = read(&[zip]).unwrap();
    let (end, value) = luke_one_one_imported(&import);
    assert_eq!(end, 2);
    assert!(
        value.marked_invalid,
        "one verse's words do not fit the span"
    );
    assert_eq!(alignment_report(&import), Vec::<String>::new());
}

/// Two records for one span: the one keyed as the text is, which tC3 shows, is kept; the
/// other is reported.
#[test]
fn a_second_alignment_of_a_bridge_is_reported() {
    let zip = with_luke_one(&bridged_luke(), |verses| {
        verses.insert("1".to_owned(), luke_one_one());
    });
    let import = read(&[zip]).unwrap();
    let (end, value) = luke_one_one_imported(&import);
    assert_eq!(end, 2);
    assert!(!value.marked_invalid, "the span's own record was kept");
    assert_eq!(
        alignment_report(&import),
        ["wordAlignment LUK 1:1 (another alignment of this verse was kept)"]
    );
}

/// A bridge the text does not have (the text keeps 1:1 and 1:2 apart) has no place.
#[test]
fn a_bridge_the_text_does_not_have_is_reported() {
    let span = entry_json(&bridged_luke(), LUKE_ONE)["1-2"].clone();
    let zip = with_luke_one(&source("tc3/multi/en_kjv_luk_book.zip"), |verses| {
        verses.insert("1-2".to_owned(), span);
    });
    let import = read(&[zip]).unwrap();
    assert_eq!(
        luke_one_one_imported(&import).0,
        1,
        "the text's own verses still import"
    );
    assert_eq!(
        alignment_report(&import),
        ["wordAlignment LUK 1:1-2 (the text bridges these verses differently)"]
    );
}

/// A verse the text lacks has no place; the bridge beside it still imports.
#[test]
fn an_alignment_of_a_verse_the_text_lacks_is_reported() {
    let zip = with_luke_one(&bridged_luke(), |verses| {
        verses.insert("999".to_owned(), luke_one_one());
        verses.insert(
            "998".to_owned(),
            serde_json::json!({ "alignments": [], "wordBank": [] }),
        );
    });
    let import = read(&[zip]).unwrap();
    assert_eq!(luke_one_one_imported(&import).0, 2);
    assert_eq!(
        alignment_report(&import),
        ["wordAlignment LUK 1:999 (no such verse in the text)"],
        "a record with no links is no loss"
    );
}

/// A key that is not a verse number has no place; tC3's empty `front` records are no loss.
#[test]
fn an_alignment_keyed_by_no_verse_number_is_reported() {
    let zip = with_luke_one(&bridged_luke(), |verses| {
        verses.insert("1a".to_owned(), luke_one_one());
        verses.insert(
            "front".to_owned(),
            serde_json::json!({ "alignments": [], "wordBank": [] }),
        );
    });
    let import = read(&[zip]).unwrap();
    assert_eq!(luke_one_one_imported(&import).0, 2);
    assert_eq!(
        alignment_report(&import),
        ["wordAlignment LUK 1:1a (not a verse number)"]
    );
}

/// A chapter past 65535, or a chapter file that is no list of verses, has no place.
#[test]
fn an_alignment_chapter_with_no_place_is_reported() {
    let far = serde_json::to_vec(&serde_json::json!({ "1": luke_one_one() })).unwrap();
    let zip = with_entries(
        &bridged_luke(),
        &[
            ("apps/translationCore/alignmentData/luk/70000.json", &far),
            ("apps/translationCore/alignmentData/luk/30.json", b"[]"),
        ],
    );
    let import = read(&[zip]).unwrap();
    assert_eq!(luke_one_one_imported(&import).0, 2);
    assert_eq!(
        alignment_report(&import),
        [
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
