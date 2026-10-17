//! Crash injection (`docs/STORAGE.md` §16.2, §6.4): every write point of every command, of
//! a seal, a consolidation, a materialization, a create and the open's own repairs is a
//! kill point; at each, the files a killed process leaves and the files a power cut leaves
//! (under both power-loss models) are opened again, and the open must show the action's
//! before or after state, every save that was reported saved, no half batch, a valid
//! burrito and no intent file, and a second open must change nothing.

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
    assert!(
        !all.contains_key(".tc/materialize.intent.json"),
        "an intent file is left after the open"
    );
    assert!(
        !all.keys().any(|p| p.contains("/.~") || p.starts_with(".~")),
        "a temporary file is left: {:?}",
        all.keys().filter(|p| p.contains(".~")).collect::<Vec<_>>()
    );
    assert_valid_burrito(&rig.fs, folder);
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

/// Runs `action` killed before each of its write points in turn, under every crash
/// model, and calls `check` on each reopened project.
pub(crate) fn kill_everywhere(
    base: &FaultFs,
    folder: &Path,
    action: &dyn Fn(&ProjectHandle) -> Outcome,
    check: &dyn Fn(&ProjectHandle, Outcome),
) -> u64 {
    let mut points = 0;
    for torn in [false, true] {
        let mut k = 0;
        loop {
            let fs = base.copy();
            let rig = rig_as(fs.clone(), APP, "host-1");
            let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
            handle.wait_idle();
            fs.kill_before(k, torn);
            let outcome = action(&handle);
            let dead = fs.is_dead();
            drop(handle);
            drop(rig);
            for (power_cut, model) in [
                (false, PowerLoss::Strict),
                (true, PowerLoss::Strict),
                (true, PowerLoss::KeepSyncedNames),
            ] {
                fs.set_power_loss(model);
                let after = fs.crash(power_cut);
                let (rig2, reopened) = reopen_checked(after, folder);
                check(&reopened, outcome);
                second_open_changes_nothing(&rig2, reopened, folder);
            }
            points += 1;
            if !dead {
                break;
            }
            k += 1;
            assert!(k < 500, "the action never ends");
        }
    }
    points
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
        Ok(reply) if matches!(reply.stored, Stored::Durable | Stored::WrittenNotDurable(_)) => {
            Outcome::Saved
        }
        _ => Outcome::NotSaved,
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
                Outcome::NotSaved => assert!(
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
            .submit(Command::Draft(Draft {
                field: DraftField::Words,
                shown: vec![shown],
                text: "draft text".into(),
            }))
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

#[test]
fn crash_checkpoint_seal_and_materialize_lose_nothing() {
    let (base, folder) = base_titus();
    // Saves waiting to be sealed.
    let fs = base.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    for v in 2..=5 {
        save_verse(&handle, 1, v, &format!("sealed {v}")).unwrap();
    }
    handle.wait_idle();
    // The files before the close seals them: a kill after the saves.
    let base = fs.crash(false);
    drop(handle);
    let action = |handle: &ProjectHandle| {
        let _ = handle
            .checkpoint(tc_store::CheckpointReason::LeavingProject)
            .wait();
        Outcome::NotSaved
    };
    let points = kill_everywhere(&base, &folder, &action, &|handle, _| {
        for v in 2..=5 {
            assert_eq!(
                words(handle, 1, v).as_deref(),
                Some(format!("sealed {v}").as_str()),
                "a sealed save was lost"
            );
        }
    });
    assert!(points > 10, "{points}");
}

#[test]
fn crash_materialize_rewrites_forward() {
    let (base, folder) = base_titus();
    let action = |handle: &ProjectHandle| {
        let reply = save_verse(handle, 1, 9, "holding to the word");
        let _ = handle.leave(tc_store::Leave::Book(TIT)).wait();
        outcome_of(reply)
    };
    kill_everywhere(&base, &folder, &action, &|handle, outcome| {
        if outcome == Outcome::Saved {
            assert_eq!(words(handle, 1, 9).as_deref(), Some("holding to the word"));
        }
        // The derived file holds what the fold projects after the open.
        handle.wait_idle();
    });
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
            assert!(!matches!(created, Ok(Ok(_))) || !dead);
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
    save_verse(&handle, 2, 1, "speak what fits").unwrap();
    let base = fs.crash(true);
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
