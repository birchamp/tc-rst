//! Crash injection (`docs/STORAGE.md` §16.2, §6.4): every write point of every command, of
//! a seal, a consolidation, a materialization, a create and the open's own repairs is a
//! kill point; at each, the files a killed process leaves and the files a power cut leaves
//! (under both power-loss models) are opened again, and the open must show the action's
//! before or after state, every save that was reported saved, no half batch, a valid
//! burrito and no write left unsettled in the placement ledger, and a second open must change
//! nothing.

use crate::common::*;
use std::collections::BTreeMap;
use std::path::Path;
use tc_journal::command::{Command, Draft, DraftField, SectionValue};
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};
use tc_store::fault::{FaultFs, PowerLoss};
use tc_store::{OpenOptions, ProjectHandle, Stored};

/// What an action reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// "Saved" was shown.
    Saved,
    /// "Saved, but the disk reported an error" (`SavedWithDiskError`, §5.0): the file is in
    /// place, but a flush after its rename failed, so a power cut may still lose it.
    /// [`kill_everywhere`] hands checks `Saved` for a row the save must survive and
    /// `NotSaved` for a power cut under the strict model.
    SavedWithDiskError,
    NotSaved,
}

/// The files that matter after an open: everything but the lock's content and the fold
/// cache, which every open may write again.
fn project_files(fs: &FaultFs, folder: &Path) -> BTreeMap<String, Vec<u8>> {
    files(fs, folder)
        .into_iter()
        .filter(|(p, _)| p != ".tc/lock" && p != ".tc/index.bin")
        .collect()
}

/// Opens the folder on `fs` and checks what every crash must leave. Returns the handle.
fn reopen_checked(fs: FaultFs, folder: &Path) -> (Rig, ProjectHandle) {
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig
        .store
        .open(folder, &OpenOptions::default())
        .unwrap_or_else(|e| panic!("the open after a crash failed: {e}"));
    handle.wait_idle();
    let all = files(&rig.fs, folder);
    let manifest = String::from_utf8_lossy(&all[".tc/materialized.json"]).into_owned();
    assert!(
        !manifest.contains("\"state\":\"writing\""),
        "a write is left unsettled in the ledger after the open: {manifest}"
    );
    assert!(
        !all.keys().any(|p| p.contains("/.~") || p.starts_with(".~")),
        "a temporary file is left: {:?}",
        all.keys().filter(|p| p.contains(".~")).collect::<Vec<_>>()
    );
    assert_valid_burrito(&rig.fs, folder);
    assert_commits_valid(&rig.fs, folder);
    (rig, handle)
}

/// A second open after a close changes nothing in the project's files.
fn second_open_changes_nothing(rig: &Rig, handle: ProjectHandle, folder: &Path) {
    handle.wait_idle();
    handle.close().unwrap();
    let before = project_files(&rig.fs, folder);
    let again = rig.store.open(folder, &OpenOptions::default()).unwrap();
    again.wait_idle();
    again.close().unwrap();
    let after = project_files(&rig.fs, folder);
    let changed: Vec<&String> = after
        .keys()
        .chain(before.keys())
        .filter(|k| before.get(*k) != after.get(*k))
        .collect();
    assert!(changed.is_empty(), "a second open changed {changed:?}");
}

/// Runs `action` killed before each of its write points in turn, once under each
/// power-loss model (the model decides what a flush makes durable while the action runs, so
/// it is set before the action), and calls `check` on each reopened project: after the killed
/// process, and after a power cut.
pub(crate) fn kill_everywhere(
    base: &FaultFs,
    folder: &Path,
    action: &dyn Fn(&ProjectHandle) -> Outcome,
    check: &dyn Fn(&ProjectHandle, Outcome),
) -> u64 {
    kill_everywhere_after(base, folder, &|_, _| {}, action, check)
}

/// [`kill_everywhere`] for an action that follows `setup`, which runs on the opened project
/// before the kill is armed.
pub(crate) fn kill_everywhere_after(
    base: &FaultFs,
    folder: &Path,
    setup: &dyn Fn(&FaultFs, &ProjectHandle),
    action: &dyn Fn(&ProjectHandle) -> Outcome,
    check: &dyn Fn(&ProjectHandle, Outcome),
) -> u64 {
    kill_everywhere_full(base, folder, setup, action, &|handle, _, outcome| {
        check(handle, outcome);
    })
}

/// [`kill_everywhere_after`] whose check also sees the files of the reopened project.
pub(crate) fn kill_everywhere_full(
    base: &FaultFs,
    folder: &Path,
    setup: &dyn Fn(&FaultFs, &ProjectHandle),
    action: &dyn Fn(&ProjectHandle) -> Outcome,
    check: &dyn Fn(&ProjectHandle, &FaultFs, Outcome),
) -> u64 {
    kill_everywhere_between(base, folder, setup, action, &|_| {}, check)
}

/// [`kill_everywhere_full`] where `between` changes the files the crash left before the open
/// that follows (a teammate's file arrives, a cloud client syncs): the reopened fold is then
/// not the one the killed action worked from.
pub(crate) fn kill_everywhere_between(
    base: &FaultFs,
    folder: &Path,
    setup: &dyn Fn(&FaultFs, &ProjectHandle),
    action: &dyn Fn(&ProjectHandle) -> Outcome,
    between: &dyn Fn(&FaultFs),
    check: &dyn Fn(&ProjectHandle, &FaultFs, Outcome),
) -> u64 {
    let mut points = 0;
    for model in [PowerLoss::KeepSyncedNames, PowerLoss::Strict] {
        for torn in [false, true] {
            let mut k = 0;
            loop {
                let fs = base.copy();
                fs.set_power_loss(model);
                let rig = rig_as(fs.clone(), APP, "host-1");
                let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
                handle.wait_idle();
                setup(&fs, &handle);
                fs.kill_before(k, torn);
                let outcome = action(&handle);
                let dead = fs.is_dead();
                drop(handle);
                drop(rig);
                // A killed process leaves what the page cache holds, whatever the model.
                let cuts: &[bool] = match model {
                    PowerLoss::KeepSyncedNames => &[false, true],
                    PowerLoss::Strict => &[true],
                };
                for &power_cut in cuts {
                    let after = fs.crash(power_cut);
                    between(&after);
                    let (rig2, reopened) = reopen_checked(after, folder);
                    check(&reopened, &rig2.fs, row_outcome(outcome, model, power_cut));
                    let held = ids_on_disk(&rig2.fs, folder);
                    second_open_changes_nothing(&rig2, reopened, folder);
                    // A restart finishes the work the kill left (its close seals and
                    // materializes): a power cut after that loses no change. A process killed
                    // between a folder's creation and the flush of its entry leaves a folder
                    // that only the page cache holds, which the restart must not trust.
                    let cut = rig2.fs.crash(true);
                    let kept = ids_on_disk(&cut, folder);
                    let lost: Vec<_> = held.difference(&kept).collect();
                    assert!(
                        lost.is_empty(),
                        "a power cut after the restart lost {lost:?} (killed before write point {k}, {model:?}, torn {torn})"
                    );
                }
                points += 1;
                if !dead {
                    break;
                }
                k += 1;
                assert!(k < 500, "the action never ends");
            }
        }
    }
    points
}

/// What a check may assert of an outcome in one row: a save the disk warned about counts as
/// saved unless a power cut under the strict model may have dropped its rename.
fn row_outcome(outcome: Outcome, model: PowerLoss, power_cut: bool) -> Outcome {
    match outcome {
        Outcome::SavedWithDiskError if power_cut && model == PowerLoss::Strict => Outcome::NotSaved,
        Outcome::SavedWithDiskError => Outcome::Saved,
        other => other,
    }
}

fn base_titus() -> (FaultFs, std::path::PathBuf) {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    (rig.fs.crash(false), folder)
}

pub(crate) fn outcome_of(result: Result<tc_store::StoreReply, tc_store::CommandError>) -> Outcome {
    match result {
        Ok(reply) => match reply.stored {
            Stored::Durable => Outcome::Saved,
            Stored::WrittenNotDurable(_) => Outcome::SavedWithDiskError,
            Stored::Unchanged => Outcome::NotSaved,
        },
        Err(_) => Outcome::NotSaved,
    }
}

/// The words of TIT c:v in the project on `fs`.
fn words_on(fs: &FaultFs, folder: &Path, c: u16, v: u16) -> Option<String> {
    let rig = rig_as(fs.copy(), APP, "host-1");
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    let out = words(&handle, c, v);
    handle.close().unwrap();
    out
}

#[test]
fn crash_save_verse_at_every_write_point() {
    let (base, folder) = base_titus();
    let before = words_on(&base, &folder, 1, 2);
    let new = "in hope of eternal life";
    let points = kill_everywhere(
        &base,
        &folder,
        &|handle| outcome_of(save_verse(handle, 1, 2, new)),
        &|handle, outcome| {
            let now = words(handle, 1, 2);
            match outcome {
                Outcome::Saved => assert_eq!(now.as_deref(), Some(new)),
                Outcome::NotSaved | Outcome::SavedWithDiskError => assert!(
                    now == before || now.as_deref() == Some(new),
                    "neither the before nor the after state: {now:?}"
                ),
            }
            assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, a servant"));
        },
    );
    assert!(points > 4, "{points} kill points");
}

#[test]
fn crash_section_save_is_all_or_nothing() {
    let (base, folder) = base_titus();
    let action = |handle: &ProjectHandle| {
        let snapshot = handle.snapshot();
        let mut shown = Vec::new();
        for v in 2..=4 {
            let key = verse(1, v);
            let token = snapshot.token(&key, None).unwrap();
            let value = match snapshot.fold().projected(&key) {
                Some(Record::Verse {
                    value:
                        VerseValue::Text {
                            marker,
                            layout,
                            last,
                            from,
                            ..
                        },
                    ..
                }) => VerseValue::Text {
                    marker,
                    words: format!("section words {v}"),
                    layout,
                    last,
                    from,
                },
                other => panic!("{other:?}"),
            };
            shown.push((token, Some(SectionValue::Verse(value))));
        }
        outcome_of(
            handle
                .submit(Command::SaveSection {
                    book: TIT,
                    shown,
                    layout: None,
                })
                .wait(),
        )
    };
    kill_everywhere(&base, &folder, &action, &|handle, outcome| {
        let saved: Vec<bool> = (2..=4)
            .map(|v| words(handle, 1, v).as_deref() == Some(format!("section words {v}").as_str()))
            .collect();
        assert!(
            saved.iter().all(|s| *s) || saved.iter().all(|s| !*s),
            "a half batch: {saved:?}"
        );
        if outcome == Outcome::Saved {
            assert!(saved[0]);
        }
    });
}

#[test]
fn crash_draft_write_never_affects_saves() {
    let (base, folder) = base_titus();
    let action = |handle: &ProjectHandle| {
        let shown = handle.mint(&verse(1, 5), None).unwrap();
        handle
            .submit(Command::Draft {
                draft: Draft {
                    field: DraftField::Words,
                    shown: vec![shown],
                    text: "draft text".into(),
                },
                at: tc_journal::WallTime(0),
            })
            .wait()
            .unwrap();
        handle.wait_idle();
        Outcome::NotSaved
    };
    kill_everywhere(&base, &folder, &action, &|handle, _| {
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, a servant"));
        let offered = handle.offered_drafts();
        assert!(offered.len() <= 1);
        if let Some(draft) = offered.first() {
            assert_eq!(draft.draft.text, "draft text");
        }
    });
}

/// A seal in a month that has no pack folder yet makes the folder, and a process killed
/// between its creation and the flush of its entry leaves a folder only the page cache holds.
/// The restart's seal must not trust it: it writes its pack there, removes the pending files
/// the pack holds, and a power cut would take the folder, the pack and with them the saves
/// (the crash matrix found this under load: a kill between `create_dir` and the flush of its
/// parent, then a restart that finishes the seal, then a power cut).
#[test]
fn crash_checkpoint_into_a_months_folder_that_does_not_exist_yet() {
    let (base, folder) = base_titus();
    // The saves wait unsealed in the session the checkpoint runs in: an open seals what an
    // earlier session left (§6.1 O15), which would make the month folder before the action.
    let save = |_: &FaultFs, handle: &ProjectHandle| {
        for v in 2..=5 {
            save_verse(handle, 1, v, &format!("sealed {v}")).unwrap();
        }
        handle.wait_idle();
    };
    let action = |handle: &ProjectHandle| {
        let _ = handle
            .checkpoint(tc_store::CheckpointReason::LeavingProject)
            .wait();
        Outcome::NotSaved
    };
    let points = in_a_later_month(|| {
        // The month folder is made by the checkpoint under test, not by the project's first.
        let probe = base.copy();
        let rig = rig_as(probe.clone(), APP, "host-1");
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        save(&probe, &handle);
        probe.clear_log();
        action(&handle);
        let made = probe
            .log()
            .iter()
            .filter(|op| {
                op.kind == tc_store::fault::OpKind::CreateDir
                    && is_inside(&op.path, &["ingredients", "tc", "changes"])
            })
            .count();
        assert!(
            made > 0,
            "the checkpoint made no pack folder: the test proves nothing"
        );
        drop(handle);
        kill_everywhere_after(&base, &folder, &save, &action, &|handle, _| {
            for v in 2..=5 {
                assert_eq!(
                    words(handle, 1, v).as_deref(),
                    Some(format!("sealed {v}").as_str()),
                    "a sealed save was lost"
                );
            }
        })
    });
    assert!(points > 10, "{points}");
}

/// §6.2 R4 to R8, §16.7: the ledger tells the bytes of the app's own interrupted write from an
/// outside edit when the fold the open ends with is not the one the killed job worked from. A
/// teammate changed the verse the job was writing before the next open: the file the job left
/// holds the verse as the job projected it, which the new projection does not name, and only
/// the ledger's `Writing` state says it is the app's own. Read as an edit it would be recorded
/// and make a conflict between the app's text and the teammate's. A kill between the ledger's
/// entry and the rename leaves the bytes before the write, which a backup tool could have put
/// back with their size and time: R9 keeps them (an extra conflict, owner 2026-10-19), never
/// the job's own new bytes.
#[test]
fn crash_materialize_ledger_tells_its_bytes_from_an_outside_edit() {
    use std::sync::Mutex;
    use tc_journal::fold::{ConflictKind, Notice};
    use tc_journal::record::VerseValue;
    use tc_journal::{BookId, ChangeId, ChapterNo, DeviceId, VerseNo};
    use tc_store::ledger::State;
    use tc_store::manifest::Manifest;
    let (base, folder) = base_titus();
    let written: Mutex<Option<ChangeId>> = Mutex::new(None);
    // The kill left the bytes a write under way replaced (R9).
    let before_rename = Mutex::new(false);
    let points = kill_everywhere_between(
        &base,
        &folder,
        &|_, handle| {
            let reply = save_verse(handle, 1, 2, "B, written by the killed job").unwrap();
            *written.lock().unwrap() = Some(reply.changes[0]);
        },
        &|handle| {
            let _ = handle.leave(tc_store::Leave::Book(TIT)).wait();
            Outcome::NotSaved
        },
        &|fs| {
            let usfm = fs.peek(&folder.join("ingredients/TIT.usfm"));
            let manifest = fs
                .peek(&folder.join(".tc/materialized.json"))
                .and_then(|m| serde_json::from_slice::<Manifest>(&m).ok());
            *before_rename.lock().unwrap() = matches!(
                (manifest.as_ref().and_then(|m| m.paths.get("ingredients/TIT.usfm")), &usfm),
                (Some(entry), Some(bytes)) if matches!(&entry.state,
                    State::Writing { old: Some(old), .. }
                        if old.md5 == tc_journal::Md5::of(bytes).to_hex())
            );
            let parent = written.lock().unwrap().expect("the job's save");
            let record = Record::Verse {
                book: BookId::Titus,
                chapter: ChapterNo::new(1).unwrap(),
                verse: VerseNo::new(2).unwrap(),
                value: VerseValue::Text {
                    marker: tc_journal::record::MarkerWs::canonical(),
                    words: "C, a teammate's later words".into(),
                    layout: "\n".into(),
                    last: VerseNo::new(2).unwrap(),
                    from: None,
                },
            };
            crate::commands::teammate_batch(
                fs,
                &folder,
                DeviceId::local([7; 15]),
                1,
                vec![(record, vec![parent])],
            );
        },
        &|handle, fs, _| {
            handle.wait_idle();
            let edits: Vec<_> = handle
                .notices()
                .into_iter()
                .filter(|n| matches!(n.notice, Notice::OutsideEditRecorded { .. }))
                .collect();
            let conflicts = handle
                .snapshot()
                .fold()
                .conflicts_of(ConflictKind::VerseText)
                .count();
            if *before_rename.lock().unwrap() {
                for edit in &edits {
                    assert!(
                        matches!(
                            edit.notice,
                            Notice::OutsideEditRecorded {
                                unknown_base: true,
                                ..
                            }
                        ),
                        "{edit:?}"
                    );
                    let copy = edit.copy.as_ref().expect("a copy");
                    let kept = fs.peek(&folder.join(copy)).expect("the copy");
                    assert!(
                        !String::from_utf8_lossy(&kept).contains("B, written by the killed job"),
                        "the job's own bytes were recorded: {edit:?}"
                    );
                }
                assert!(conflicts <= 1, "{conflicts} conflicts");
                return;
            }
            assert!(
                edits.is_empty(),
                "the app's own bytes were recorded: {edits:?}"
            );
            assert_eq!(conflicts, 0, "a conflict with the app's own text");
            assert_eq!(
                words(handle, 1, 2).as_deref(),
                Some("C, a teammate's later words")
            );
        },
    );
    assert!(points > 20, "{points} kill points");
}

#[test]
fn crash_create_leaves_no_project_or_a_whole_one() {
    let fs = FaultFs::new();
    let rig = rig(fs.clone());
    drop(rig);
    let base = fs.crash(false);
    let mut k = 0;
    loop {
        let fs = base.copy();
        let rig = rig_as(fs.clone(), APP, "host-1");
        fs.kill_before(k, false);
        let created = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rig.store.create(&tc_store::NewProject {
                name: "Titus".into(),
                kind: tc_journal::fold::ProjectKind::Bible,
                records: first_records(),
                books: vec![(
                    TIT,
                    tc_journal::record::Scope::Whole,
                    tc_journal::command::SeedSource { usfm: ult("TIT") },
                )],
            })
        }));
        let dead = fs.is_dead();
        drop(rig);
        let after = fs.crash(true);
        let rig2 = rig_as(after, APP, "host-1");
        let all = files(&rig2.fs, Path::new(PROJECTS));
        assert!(
            !all.keys().any(|p| p.starts_with(".staging-")),
            "the start cleanup removes staging folders (N1)"
        );
        let target = Path::new(PROJECTS).join("Titus");
        if rig2
            .fs
            .peek(&target.join("ingredients/tc/project.json"))
            .is_some()
        {
            // N2: a whole project.
            let handle = rig2.store.open(&target, &OpenOptions::default()).unwrap();
            assert!(handle.snapshot().fold().book_usfm(TIT).is_some());
            handle.wait_idle();
            handle.close().unwrap();
            assert_valid_burrito(&rig2.fs, &target);
        } else {
            assert!(
                !matches!(created, Ok(Ok(_))),
                "a create reported done is gone after a power cut (killed before write point {k})"
            );
        }
        if !dead {
            break;
        }
        k += 1;
        assert!(k < 2000);
    }
}

#[test]
fn crash_during_open_repairs_lose_nothing() {
    // A save whose materialization never ran, and a crash inside the next open's repairs.
    let (base, folder) = base_titus();
    let fs = base.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    // Held, so the materialization a save starts three seconds later cannot run first.
    rig.store.hooks().hold(tc_store::Point::BeforeMaterialize);
    save_verse(&handle, 2, 1, "speak what fits").unwrap();
    let base = fs.crash(true);
    rig.store
        .hooks()
        .release(tc_store::Point::BeforeMaterialize);
    drop(handle);
    let mut k = 0;
    loop {
        let fs = base.copy();
        let rig = rig_as(fs.clone(), APP, "host-1");
        fs.kill_before(k, false);
        let opened = rig.store.open(&folder, &OpenOptions::default());
        if let Ok(h) = &opened {
            h.wait_idle();
        }
        let dead = fs.is_dead();
        drop(opened);
        drop(rig);
        for power_cut in [false, true] {
            let (rig2, reopened) = reopen_checked(fs.crash(power_cut), &folder);
            let shown = reopened.snapshot();
            let key = RecordKey::Verse(
                TIT,
                tc_journal::ChapterNo::new(2).unwrap(),
                tc_journal::VerseNo::new(1).unwrap(),
            );
            assert!(matches!(
                shown.fold().projected(&key),
                Some(Record::Verse { value: VerseValue::Text { ref words, .. }, .. }) if words == "speak what fits"
            ));
            second_open_changes_nothing(&rig2, reopened, &folder);
        }
        if !dead {
            break;
        }
        k += 1;
        assert!(k < 2000);
    }
}

/// A seal killed after it renamed its pack into place and before it flushed the pack's
/// folder leaves a pack only the page cache holds. The next open finds every change of the
/// pending file in it and removes the pending file (O7); unless the open flushes the pack
/// first, a power cut then takes both copies of the save.
#[test]
fn an_unflushed_pack_is_flushed_before_the_open_removes_what_it_holds() {
    use tc_store::fault::OpKind;
    let (base, folder) = base_titus();
    base.set_power_loss(PowerLoss::Strict);
    let seal_after_save = |fs: &FaultFs| {
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        save_verse(&handle, 1, 3, "kept by the pack").unwrap();
        handle.wait_idle();
        (rig, handle)
    };
    // The write point of the flush of the pack's folder, from a run on a copy.
    let probe = base.copy();
    let (_rig, handle) = seal_after_save(&probe);
    let start = probe.ops();
    probe.clear_log();
    handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait()
        .unwrap();
    let log = probe.log();
    drop(handle);
    let changes = folder.join("ingredients/tc/changes");
    let rename = log
        .iter()
        .position(|op| op.kind == OpKind::Rename && op.path.starts_with(&changes))
        .unwrap();
    let flush = &log[rename + 1];
    assert_eq!(flush.kind, OpKind::SyncDir, "{log:#?}");
    let fs = base.copy();
    let (rig, handle) = seal_after_save(&fs);
    fs.kill_before(flush.index - start, false);
    let _ = handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait();
    assert!(fs.is_dead());
    drop(handle);
    drop(rig);
    // The next process opens the folder and removes the pending file.
    let next = fs.crash(false);
    let rig = rig_as(next.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(
        pending_files(&next, &folder).is_empty(),
        "O7 removed it {:?} {:?}",
        files(&next, &folder).keys().collect::<Vec<_>>(),
        handle.notices()
    );
    drop(handle);
    drop(rig);
    // A power cut.
    let rig = rig_as(next.crash(true), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(words(&handle, 1, 3).as_deref(), Some("kept by the pack"));
    handle.close().unwrap();
}

/// A save a scanner kept from being written, then the person saves again, and the process
/// stops at any write point of the second save: every reopen shows no other computer saving
/// as this one and no conflict on the verse, the verse holds the second text whenever that
/// save was reported saved, and what it holds is always one of the two attempts (or, after a
/// power cut that took the failed attempt's file, the text before).
#[test]
fn crash_held_save_and_retry() {
    use tc_journal::fold::{Anomaly, Notice, RotationReason};
    let (base, folder) = base_titus();
    let before = words_on(&base, &folder, 1, 2);
    let pending = folder.join(".tc/pending");
    let points = kill_everywhere_after(
        &base,
        &folder,
        &|fs, handle| {
            fs.hold_new_files(&pending, None);
            assert!(save_verse(handle, 1, 2, "A, the held attempt").is_err());
            fs.release_new_files(&pending);
        },
        &|handle| outcome_of(save_verse(handle, 1, 2, "B, the retry")),
        &|handle, outcome| {
            let now = words(handle, 1, 2);
            if outcome == Outcome::Saved {
                assert_eq!(now.as_deref(), Some("B, the retry"));
            } else {
                assert!(
                    now == before
                        || now.as_deref() == Some("A, the held attempt")
                        || now.as_deref() == Some("B, the retry"),
                    "{now:?}"
                );
            }
            assert!(
                !handle
                    .snapshot()
                    .fold()
                    .anomalies()
                    .any(|a| matches!(a, Anomaly::DuplicateDevice { .. })),
                "another computer saved as this one"
            );
            assert_eq!(handle.snapshot().fold().conflict_count(), 0);
            assert!(!handle.notices().iter().any(|n| matches!(
                n.notice,
                Notice::DeviceRotated {
                    reason: RotationReason::DuplicateDevice,
                    ..
                }
            )));
        },
    );
    assert!(points > 4, "{points} kill points");
}

/// Every derived file of the project is what the fold projects.
fn assert_files_equal_projection(handle: &ProjectHandle, fs: &FaultFs, folder: &Path) {
    let snapshot = handle.snapshot();
    for file in snapshot.fold().derived_files() {
        let on_disk = fs.peek(&folder.join(file.path()));
        assert_eq!(
            on_disk,
            snapshot.fold().render(&file),
            "{} is not what the project says",
            file.path()
        );
    }
}

/// A pin change rewrites every book's files, book by book: a process stopped between two
/// books leaves a valid burrito, and the next open writes the rest.
#[test]
fn crash_whole_project_materialize_group_by_group() {
    use tc_journal::command::SeedSource;
    use tc_journal::record::Scope;
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    for book in [tc_journal::BookId::Philemon, tc_journal::BookId::SecondJohn] {
        handle
            .submit(Command::AddBook {
                book,
                scope: Scope::Whole,
                seed: SeedSource {
                    usfm: format!(
                        "\\id {0} source\n\\usfm 3.0\n\\h {0}\n\\mt {0}\n\\c 1\n\\p\n\\v 1 One.\n",
                        book.code()
                    ),
                },
            })
            .wait()
            .unwrap();
    }
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let base = rig.fs.crash(false);
    let pins = tc_journal::RecordKey::Pins(tc_journal::PinGroup::Lexicon);
    let action = |handle: &ProjectHandle| {
        let shown = handle.mint(&pins, None).unwrap();
        let sent = handle
            .submit(Command::SetPins {
                shown,
                value: lexicon_pin(),
            })
            .wait();
        let _ = handle.leave(tc_store::Leave::Mode).wait();
        outcome_of(sent)
    };
    let points = kill_everywhere_full(&base, &folder, &|_, _| {}, &action, &|handle, fs, _| {
        assert_files_equal_projection(handle, fs, &folder);
    });
    assert!(points > 20, "{points}");
}

fn lexicon_pin() -> tc_journal::record::PinGroupValue {
    use std::collections::BTreeMap;
    use tc_journal::json::{Json, JsonMap};
    use tc_journal::keys::SlotName;
    use tc_journal::record::{PinGroupValue, SlotValue};
    PinGroupValue(BTreeMap::from([(
        SlotName::parse("ugl").unwrap(),
        SlotValue(Json::Obj(JsonMap::from([
            (
                "repoPath".to_owned(),
                Json::from("git.door43.org/unfoldingWord/en_ugl"),
            ),
            ("sha".to_owned(), Json::from("0".repeat(40))),
            ("version".to_owned(), Json::from("v1")),
        ]))),
    )]))
}

// ---------------------------------------------------------------------------------------
// The checkpoint's git rows (§6.4 G1 to G3)
// ---------------------------------------------------------------------------------------

/// G1: the commit object is written and the process stops before the branch moves. The
/// objects are unreachable and harmless; the next checkpoint commits the same changes once.
#[test]
fn crash_g1_objects_written_ref_not_moved_commits_again() {
    use tc_store::{Failure, Hooks, Point};
    let hooks = std::sync::Arc::new(Hooks::default());
    let (base, folder) = titus();
    let fs = base.fs.copy();
    let rig = rig_with_hooks(fs.clone(), hooks.clone());
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let before = commits_of(&repo_of(&fs, &folder));
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    hooks.fail(Point::BeforeHeadUpdate, Failure::Stop);
    let stopped = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait();
    assert!(stopped.is_err(), "{stopped:?}");
    let after = fs.crash(true);
    hooks.heal(Point::BeforeHeadUpdate);
    handle.close().unwrap();
    assert_eq!(commits_of(&repo_of(&after, &folder)), before);
    let (rig2, handle) = reopen_checked(after, &folder);
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    let repo = repo_of(&rig2.fs, &folder);
    assert_eq!(commits_of(&repo).len(), before.len() + 1, "one commit");
    let usfm = String::from_utf8(
        commit_files(&repo, commits_of(&repo)[0])["ingredients/TIT.usfm"].clone(),
    )
    .unwrap();
    assert!(usfm.contains("Paul, a servant of God"));
    let again = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert_eq!(again.skipped, Some(tc_store::CheckpointSkip::NoChanges));
    handle.close().unwrap();
    assert_commits_valid(&rig2.fs, &folder);
}

/// G2: a stopped process leaves the branch's lock file. The next open removes it, and the
/// checkpoint commits.
#[test]
fn crash_g2_ref_lock_left_is_removed() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    handle.wait_idle();
    let after = rig.fs.crash(false);
    drop(handle);
    let lock = git_dir_of(&after, &folder).join("refs/heads/main.lock");
    std::fs::write(&lock, b"a stopped process").unwrap();
    let (rig2, handle) = reopen_checked(after, &folder);
    assert!(!lock.exists());
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    handle.close().unwrap();
    assert_commits_valid(&rig2.fs, &folder);
}

/// G3: a power cut loses the update of the branch's reference. HEAD is the earlier commit, and
/// the next checkpoint commits again; the project lost nothing, because git is not the source of
/// truth.
#[test]
fn crash_g3_lost_ref_update_commits_again() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let reference = git_dir_of(&rig.fs, &folder).join("refs/heads/main");
    let old = std::fs::read(&reference).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some());
    handle.close().unwrap();
    let after = rig.fs.crash(false);
    std::fs::write(git_dir_of(&after, &folder).join("refs/heads/main"), &old).unwrap();
    let (rig2, handle) = reopen_checked(after, &folder);
    let repo = repo_of(&rig2.fs, &folder);
    let lost = commits_of(&repo).len();
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    assert_eq!(commits_of(&repo).len(), lost + 1);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig2.fs, &folder);
}

/// §16.2, rows M1 to M6 with the two kinds of path the groups skip or place: a held file (its
/// bytes were changed without its time, so the app leaves it as it is) and a carried file that
/// went missing (it is placed again from the object database). Killed at every write point of
/// a save, the window focus that finds the carried file gone and the materialization, the
/// reopened project has the held file's bytes untouched, the carried file back or still
/// pending its placement, every save that was reported saved, and a valid burrito.
#[test]
fn crash_materialize_with_a_carried_file_and_a_held_file() {
    const SONG: &str = "ingredients/audio/song.mp3";
    let (rig, folder) = titus();
    let held_rel = "ingredients/TIT.usfm";
    let song = b"an hour of singing, in a few bytes".to_vec();
    {
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
        rig.fs.put_unseen(&folder.join(SONG), &song, None);
        handle.focus().wait().unwrap();
        handle.leave(tc_store::Leave::Mode).wait().unwrap();
        handle.wait_idle();
        handle.close().unwrap();
    }
    settle_entries(&rig, &folder);
    let damaged = damage_file(&rig, &folder, held_rel);
    let base = rig.fs.crash(false);
    let song_path = folder.join(SONG);
    let setup = |fs: &FaultFs, _: &ProjectHandle| {
        fs.delete(&song_path).unwrap();
    };
    let action = |handle: &ProjectHandle| {
        let reply = save_verse(handle, 1, 9, "holding to the word");
        let _ = handle.focus().wait();
        let _ = handle.leave(tc_store::Leave::Mode).wait();
        outcome_of(reply)
    };
    let points = kill_everywhere_full(&base, &folder, &setup, &action, &|handle, fs, outcome| {
        if outcome == Outcome::Saved {
            assert_eq!(words(handle, 1, 9).as_deref(), Some("holding to the word"));
        }
        handle.wait_idle();
        // The person's bytes in the held file are never written over.
        assert_eq!(
            fs.peek(&folder.join(held_rel)),
            Some(damaged.clone()),
            "the held file was written over"
        );
        // The carried file is back, or the next materialization places it: a missing file is
        // never read as a removal.
        handle.focus().wait().unwrap();
        handle.wait_idle();
        assert_eq!(fs.peek(&folder.join(SONG)), Some(song.clone()));
    });
    assert!(points > 10, "{points}");
}
