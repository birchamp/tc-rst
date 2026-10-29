//! tC4's tests ported (`docs/TC4-DELTA-2026-10-16.md` §3.10 and §4 "tc-store"): the staging
//! rename, the start cleanup, the training snapshot, and the open's power-loss scenarios that
//! need no git; the ones that do (a reopen after packs and stale files, an out-of-band edit
//! that is still recorded) are here too, since the store records outside edits (§11.1).

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tc_journal::command::{Command, Draft, DraftField};
use tc_journal::fold::{Notice, ProjectKind};
use tc_journal::record::Scope;
use tc_store::fault::FaultFs;
use tc_store::{CreateError, EditorText, Hooks, NewProject, OpenOptions, Point};

fn titus_project(name: &str) -> NewProject {
    NewProject {
        name: name.into(),
        kind: ProjectKind::Bible,
        records: first_records(),
        books: vec![(
            TIT,
            Scope::Whole,
            tc_journal::command::SeedSource { usfm: ult("TIT") },
        )],
    }
}

/// `test/sync/openFromDoor43.test.ts:1000-1025`, `syncRig.integration.test.ts:134` (#725).
#[test]
fn a_collision_at_the_rename_keeps_the_destination_byte_for_byte() {
    let hooks = Arc::new(Hooks::default());
    let rig = Arc::new(rig_with_hooks(FaultFs::new(), hooks.clone()));
    rig.store.wait_cleanup();
    hooks.hold(Point::BeforeStagingRename);
    let creating = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create(&titus_project("Titus"))
    });
    hooks.wait_reached(Point::BeforeStagingRename, 1);
    // Another project appears at the name between the check and the rename.
    let theirs = Path::new(PROJECTS).join("Titus");
    rig.fs
        .put(&theirs.join("notes.txt"), b"someone else's work", None);
    let before = files(&rig.fs, &theirs);
    hooks.release(Point::BeforeStagingRename);
    let result = creating.join().unwrap();
    assert!(
        matches!(result, Err(CreateError::NameExists(_))),
        "{result:?}"
    );
    assert_eq!(
        files(&rig.fs, &theirs),
        before,
        "the destination keeps every byte"
    );
    assert!(
        !files(&rig.fs, Path::new(PROJECTS))
            .keys()
            .any(|p| p.starts_with(".staging-")),
        "only the staging folder is removed"
    );
}

/// tC4 #676: the import shell waits for `recoverySettled()`.
#[test]
fn an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes() {
    let fs = FaultFs::new();
    let stale = Path::new(PROJECTS).join(".staging-0123456789abcdef");
    fs.put(&stale.join("ingredients/tc/project.json"), b"{}", None);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let rig = Arc::new(rig_with_hooks(fs.clone(), hooks.clone()));
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let creating = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create(&titus_project("Titus"))
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !creating.is_finished(),
        "a create waits for the start cleanup"
    );
    assert!(
        !files(&fs, Path::new(PROJECTS))
            .keys()
            .any(|p| p.starts_with(".staging-") && !p.starts_with(".staging-0123456789abcdef")),
        "no staging folder of the create exists while the cleanup runs"
    );
    hooks.release(Point::BeforeStagingRemoval);
    let created: PathBuf = creating.join().unwrap().unwrap();
    assert!(
        fs.peek(&stale.join("ingredients/tc/project.json"))
            .is_none()
    );
    assert!(fs.peek(&created.join("metadata.json")).is_some());
}

/// `test/trainingBookLoading.test.ts`: training during a book switch never pairs this book's
/// alignments with another book's text.
#[test]
fn training_takes_text_and_alignments_from_one_snapshot_and_this_books_editor_text_only() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let snapshot = handle.snapshot();
    // A later save is not in the snapshot the training took.
    save_verse(&handle, 1, 1, "changed after the snapshot").unwrap();
    let other_book = EditorText {
        book: tc_journal::BookId::Jonah,
        chapter: tc_journal::ChapterNo::new(1).unwrap(),
        verse: tc_journal::VerseNo::new(1).unwrap(),
        words: "Jonah's unsaved words".into(),
    };
    let input = snapshot.training_input(TIT, Some(&other_book));
    let first = input
        .iter()
        .find(|v| v.chapter.get() == 1 && v.verse.get() == 1)
        .unwrap();
    assert_eq!(first.words, "Paul, a servant of God");
    let this_book = EditorText {
        book: TIT,
        ..other_book
    };
    let input = snapshot.training_input(TIT, Some(&this_book));
    let first = input
        .iter()
        .find(|v| v.chapter.get() == 1 && v.verse.get() == 1)
        .unwrap();
    assert_eq!(first.words, "Jonah's unsaved words");
    handle.close().unwrap();
}

/// `test/sync/syncOperation.test.ts:1178-1476`: a torn, missing or empty `metadata.json`
/// never locks the project.
#[test]
fn a_torn_missing_or_empty_metadata_json_never_locks_the_project() {
    let (rig, folder) = titus();
    let metadata = folder.join("metadata.json");
    let whole = rig.fs.peek(&metadata).unwrap();
    for broken in [
        Some(whole[..whole.len() / 2].to_vec()),
        Some(Vec::new()),
        None,
    ] {
        let fs = rig.fs.copy();
        match &broken {
            Some(bytes) => fs.put(&metadata, bytes, None),
            None => fs.delete(&metadata).unwrap(),
        }
        let rig2 = rig_as(fs.clone(), APP, "host-1");
        let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
        assert_eq!(handle.facts().mode, tc_store::Mode::Writable);
        save_verse(&handle, 1, 1, "still works").unwrap();
        handle.wait_idle();
        handle.close().unwrap();
        assert_valid_burrito(&fs, &folder);
    }
}

/// `LICENSE.md` not UTF-8: the project still opens.
#[test]
fn a_file_that_is_not_utf8_never_stops_an_open() {
    let (rig, folder) = titus();
    rig.fs.put(
        &folder.join("ingredients/LICENSE.md"),
        &[0xff, 0xfe, 0x00, 0x80],
        None,
    );
    rig.fs.put(
        &folder.join("ingredients/TIT.usfm"),
        &[0xc3, 0x28, 0xa0],
        None,
    );
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(handle.snapshot().fold().book_usfm(TIT).is_some());
    handle.wait_idle();
    handle.close().unwrap();
}

/// Packs written and derived files stale, then a kill or a power cut: the reopen
/// materializes forward and records no outside edit.
#[test]
fn a_reopen_after_packs_and_stale_files_materializes_forward_and_records_no_outside_edit() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    // The probe saves in its own session: an open seals what an earlier one left (§6.1 O15).
    let probe = rig.fs.copy();
    save_verse(&handle, 2, 1, "speak what fits sound teaching").unwrap();
    // A checkpoint whose seal ran and whose materialization did not: kill before the
    // first write of the materialization, the heads of the first file it writes (§5.10 M3).
    handle.wait_idle();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    let probe_handle = probe_rig
        .store
        .open(&folder, &OpenOptions::default())
        .unwrap();
    save_verse(&probe_handle, 2, 1, "speak what fits sound teaching").unwrap();
    probe_handle.wait_idle();
    probe.clear_log();
    let start = probe.ops();
    probe_handle
        .checkpoint(tc_store::CheckpointReason::LeavingProject)
        .wait()
        .unwrap();
    let intent = probe
        .log()
        .iter()
        .find(|op| slashed(&op.path).contains(".tc/heads"))
        .map(|op| op.index - start)
        .unwrap();
    drop(probe_handle);
    rig.fs.kill_before(intent, false);
    let _ = handle
        .checkpoint(tc_store::CheckpointReason::LeavingProject)
        .wait();
    drop(handle);
    for power_cut in [false, true] {
        let rig2 = rig_as(rig.fs.crash(power_cut), APP, "host-1");
        assert!(!packs(&rig2.fs, &folder).is_empty());
        let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        let notices = handle.notices();
        assert!(
            !notices.iter().any(|n| matches!(
                n.notice,
                Notice::OutsideEditHeld { .. }
                    | Notice::StorageDamage { .. }
                    | Notice::OutsideEditRecorded { .. }
            )),
            "the app's own stale files are no outside edit: {notices:?}"
        );
        let usfm =
            String::from_utf8(rig2.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap()).unwrap();
        assert!(usfm.contains("speak what fits sound teaching"));
        handle.close().unwrap();
        assert_valid_burrito(&rig2.fs, &folder);
    }
}

/// APP-DESIGN R22: a draft write never delays a command.
#[test]
fn a_held_draft_write_never_delays_a_save() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.hold(Point::BeforeDraftWrite);
    let shown = handle.mint(&verse(1, 1), None).unwrap();
    handle
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![shown],
                text: "typing".into(),
            },
            at: tc_journal::WallTime(0),
        })
        .wait()
        .unwrap();
    hooks.wait_reached(Point::BeforeDraftWrite, 1);
    let saving = handle.submit(Command::SaveVerse {
        shown: handle.mint(&verse(1, 2), None).unwrap(),
        words: "saved while a draft write waits".into(),
    });
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while !saving.is_ready() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    // Read before the release, so a save that waits behind the draft write fails, not hangs.
    let served = saving.is_ready();
    hooks.release(Point::BeforeDraftWrite);
    assert!(served, "the save waited behind the held draft write");
    saving.wait().unwrap();
    handle.close().unwrap();
}

/// `docs/TC4-DELTA-2026-10-16.md` §3.3, N1 for a join: the start cleanup says which staging
/// folders it removed and what each was for, so the app can tell the person that opening a
/// project from Door43 did not finish (`refusal.open-door43.removed`). A staging folder for
/// an import or a join is handed out only after the cleanup, so the cleanup never removes
/// one that is in use.
#[test]
fn the_start_cleanup_reports_a_join_it_removed_and_hands_out_no_staging_folder_before_it_ends() {
    use tc_store::{RemovedStaging, StagingKind};
    let fs = FaultFs::new();
    let join = Path::new(PROJECTS).join(".staging-join-00112233445566ff");
    fs.put(&join.join(".git/HEAD"), b"ref: refs/heads/main\n", None);
    let old = Path::new(PROJECTS).join(".staging-0123456789abcdef");
    fs.put(&old.join("x"), b"x", None);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let rig = Arc::new(rig_with_hooks(fs.clone(), hooks.clone()));
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let asking = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.staging(StagingKind::Import)
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !asking.is_finished(),
        "a staging folder waits for the cleanup"
    );
    hooks.release(Point::BeforeStagingRemoval);
    let staging = asking.join().unwrap();
    assert!(
        staging
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".staging-import-")
    );
    let removed = rig.store.removed_at_start();
    assert_eq!(
        removed,
        vec![
            RemovedStaging {
                folder: ".staging-0123456789abcdef".into(),
                kind: None,
            },
            RemovedStaging {
                folder: ".staging-join-00112233445566ff".into(),
                kind: Some(StagingKind::Join),
            },
        ]
    );
    assert!(fs.peek(&join.join(".git/HEAD")).is_none());
}

/// `test/sync/syncOperation.test.ts` power-loss scenarios: packs written and derived files
/// stale, then a crash, plus a real edit of the book made outside the app before the reopen.
/// The reopen keeps the file's bytes in a copy and asks; *Import* takes the edit in against the
/// verse the stale file showed and writes the book forward: the edit and the save both stay.
#[test]
fn a_reopen_after_packs_and_stale_files_keeps_an_outside_edit_of_the_book() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 2, 1, "speak what fits sound teaching").unwrap();
    handle.wait_idle();
    // Killed before the materialization: the save is in a pending file, TIT.usfm is stale.
    let fs = rig.fs.crash(true);
    drop(handle);
    let usfm = folder.join("ingredients/TIT.usfm");
    let mut edited = fs.peek(&usfm).unwrap();
    let at = edited.windows(3).position(|w| w == b"___").unwrap();
    edited[at..at + 3].copy_from_slice(b"Pau");
    // The verse the edit is in: the last `\c` and `\v` before it.
    let before = String::from_utf8_lossy(&edited[..at]).into_owned();
    let number = |marker: &str| -> u16 {
        let from = before.rfind(marker).unwrap() + marker.len();
        before[from..]
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let (chapter, verse) = (number("\\c "), number("\\v "));
    fs.put(&usfm, &edited, None);
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    // The edit waits for the person's answer (a declared difference from tC4, which takes it
    // in on its own at open); *Import* takes it in against the verse the stale file showed.
    assert_eq!(handle.waiting_books().len(), 1, "{:?}", handle.notices());
    let copies = copies_of(&handle, "ingredients/TIT.usfm");
    assert_eq!(copies.len(), 1);
    assert_eq!(
        fs.peek(&folder.join(&copies[0])).as_deref(),
        Some(edited.as_slice()),
        "the person's bytes are kept"
    );
    assert_eq!(counts(&import_waiting(&handle)), vec![(1, 0)]);
    assert_eq!(
        words(&handle, 2, 1).as_deref(),
        Some("speak what fits sound teaching")
    );
    assert_eq!(words(&handle, chapter, verse).as_deref(), Some("Pau"));
    let written = String::from_utf8(fs.peek(&usfm).unwrap()).unwrap();
    assert!(
        written.contains("speak what fits sound teaching"),
        "{written}"
    );
    assert!(written.contains("Pau"));
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
}

/// `syncOperation.test.ts`, a file that is not UTF-8: a derived file replaced by bytes that
/// are not text still lets the project open and save. The bytes are kept in a copy, the person
/// is asked with the reason the file cannot be read, and *Not now* writes it from the project.
#[test]
fn a_derived_file_that_is_not_utf8_never_stops_the_project() {
    let (rig, folder) = titus();
    let fs = rig.fs.crash(false);
    let usfm = folder.join("ingredients/TIT.usfm");
    let garbage: Vec<u8> = (0..300u32).map(|i| (i % 7 * 37 + 128) as u8).collect();
    fs.put(&usfm, &garbage, None);
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(handle.facts().mode, tc_store::Mode::Writable);
    save_verse(&handle, 1, 1, "still saves").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    // Kept, held and asked about with the reason it cannot be read (*Not now* only).
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{:?}", handle.notices());
    assert!(books[0].kept.unreadable.is_some());
    let copies = copies_of(&handle, "ingredients/TIT.usfm");
    assert_eq!(copies.len(), 1);
    assert_eq!(
        fs.peek(&folder.join(&copies[0])),
        Some(garbage),
        "the bytes are kept whole"
    );
    handle.not_now("ingredients/TIT.usfm").wait().unwrap();
    handle.wait_idle();
    let written = String::from_utf8(fs.peek(&usfm).unwrap()).unwrap();
    assert!(written.contains("still saves"), "{written}");
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
}

/// The first records of a project whose language has `tag` and the name "Lengua".
fn project_in(name: &str, tag: &str) -> NewProject {
    let languages =
        format!(r#"[{{"name":{{"en":"Lengua"}},"scriptDirection":"rtl","tag":"{tag}"}}]"#);
    let mut project = titus_project(name);
    for record in &mut project.records {
        if let tc_journal::Record::ProjectMeta {
            field: tc_journal::MetaField::Languages,
            value,
        } = record
        {
            *value = Some(tc_journal::json::parse(languages.as_bytes()).unwrap());
        }
    }
    project
}

/// tC4 #582: Door43 lists codes with a part over eight letters, which a Scripture Burrito
/// cannot hold. tC4 stored "und" and dropped the language's name and direction. Create refuses
/// before any folder exists, so no half project and no staging folder is left.
#[test]
fn create_refuses_a_language_tag_the_burrito_cannot_hold() {
    let rig = rig(FaultFs::new());
    for tag in ["acm-x-lebarabic", "alk-alakatapue"] {
        let result = rig.store.create(&project_in("Lengua", tag));
        assert!(
            matches!(
                result,
                Err(CreateError::Refused(tc_store::CommandError::Refused(
                    tc_store::Refusal::NotValidInBurrito(_)
                )))
            ),
            "{tag}: {result:?}"
        );
        assert!(
            files(&rig.fs, Path::new(PROJECTS)).is_empty(),
            "{tag}: something was written"
        );
    }
}

/// tC4 #386: regional and private-use tags are kept as written, with the name and direction.
#[test]
fn create_keeps_regional_language_tags() {
    let rig = rig(FaultFs::new());
    for (n, tag) in ["es-419", "pt-BR", "x-ug", "acm-x-lebarabi-c"]
        .into_iter()
        .enumerate()
    {
        let name = format!("Lengua{n}");
        let folder = rig.store.create(&project_in(&name, tag)).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&rig.fs.peek(&folder.join("metadata.json")).unwrap()).unwrap();
        let language = &metadata["languages"][0];
        assert_eq!(language["tag"], tag);
        assert_eq!(language["name"]["en"], "Lengua");
        assert_eq!(language["scriptDirection"], "rtl");
        assert_valid_burrito(&rig.fs, &folder);
    }
}
