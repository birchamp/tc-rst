//! O7 keeps every value readable when salvaging one pending file replaces another.

use crate::common::*;
use tc_journal::Verified;
use tc_journal::fold::Notice;
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_store::fault::FaultFs;
use tc_store::{CheckpointReason, OpenOptions, ProjectHandle, Store};

fn assert_saved_history(handle: &ProjectHandle, lines: &[Verified]) {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    for line in lines {
        let change = line.as_change().unwrap();
        assert!(
            fold.history(&change.body().record.key())
                .contains(&change.id()),
            "saved change {} is absent from History",
            change.id()
        );
        assert_eq!(
            fold.body(&change.id()).as_ref(),
            Some(change.body()),
            "saved change {} is listed but its value cannot be read",
            change.id()
        );
        assert_eq!(fold.raw(&change.id()).as_ref(), Some(change.raw()));
    }
}

fn recover_over_an_existing_salvage(damaged_name: &str, damaged_sorts_first: bool) {
    let fs = FaultFs::new();
    let (mut settings, _) = config(&fs, APP, "host-1");
    // With one reader, the largest journal file supplies each duplicated value first.
    // The invalid tail below makes that file the damaged source in both path orders.
    settings.threads = 1;
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    let rig = Rig { fs, store };
    let folder = create_titus(&rig, "Titus");
    let project = project_of(&rig.fs, &folder);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let device = handle.facts().device;
    save_verse(&handle, 1, 1, "first saved words").unwrap();
    save_verse(&handle, 1, 1, "second saved words").unwrap();
    save_verse(&handle, 1, 2, "saved words of another verse").unwrap();
    let mut lines: Vec<Verified> = pending_files(&rig.fs, &folder)
        .into_iter()
        .flat_map(|rel| {
            let bytes = rig.fs.peek(&folder.join(rel)).unwrap();
            let read = read_pending(&bytes, project);
            assert_eq!(read.status, PendingStatus::Valid);
            read.entries
        })
        .collect();
    lines.sort_by_key(Verified::sort_key);
    assert_eq!(lines.len(), 3, "each public save supplies its own change");
    assert_saved_history(&handle, &lines);
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "the saved changes are sealed");
    handle.close().unwrap();
    assert!(pending_files(&rig.fs, &folder).is_empty());
    assert_valid_burrito(&rig.fs, &folder);

    let header = PendingHeader {
        device,
        project,
        purpose: Purpose::Salvage,
        reconciles: None,
    };
    let existing = encode_pending(&header, &lines).unwrap();
    assert_eq!(damaged_name < existing.name.as_str(), damaged_sorts_first);
    let existing_rel = format!(".tc/pending/{}", existing.name);
    rig.fs
        .put_unseen(&folder.join(&existing_rel), &existing.bytes, None);
    let mut damaged = encode_pending(
        &PendingHeader {
            purpose: Purpose::Save,
            ..header
        },
        &lines,
    )
    .unwrap()
    .bytes;
    let largest_pack = packs(&rig.fs, &folder)
        .into_iter()
        .map(|rel| rig.fs.peek(&folder.join(rel)).unwrap().len())
        .max()
        .unwrap();
    damaged.resize(largest_pack.max(damaged.len()) + 4096, b'!');
    damaged.push(b'\n');
    let read = read_pending(&damaged, project);
    assert!(matches!(read.status, PendingStatus::Damaged(_)));
    assert_eq!(read.entries, lines, "only the appended tail is damaged");
    let damaged_rel = format!(".tc/pending/{damaged_name}");
    rig.fs
        .put_unseen(&folder.join(&damaged_rel), &damaged, None);
    // Pending files are outside the Burrito, including deliberately damaged ones.
    assert_valid_burrito(&rig.fs, &folder);

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("second saved words"));
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("saved words of another verse")
    );
    assert!(
        handle.notices().iter().any(|notice| matches!(
            &notice.notice,
            Notice::QuarantinedPending { file, salvaged, .. }
                if file.as_ref() == damaged_rel && *salvaged == lines.len() as u32
        )),
        "the original damaged file is reported: {:?}",
        handle.notices()
    );
    assert!(rig.fs.peek(&folder.join(&damaged_rel)).is_none());
    assert!(
        rig.fs
            .files_under(&folder.join(".tc/quarantine"))
            .values()
            .any(|bytes| *bytes == damaged),
        "the original damaged bytes are kept in quarantine"
    );
    if damaged_sorts_first {
        assert!(
            rig.fs.peek(&folder.join(&existing_rel)).is_none(),
            "the replacement salvage was relocated into its pack before removal"
        );
    }
    handle.close().unwrap();
    assert!(pending_files(&rig.fs, &folder).is_empty());
    assert_valid_burrito(&rig.fs, &folder);

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    assert!(pending_files(&rig.fs, &folder).is_empty());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn salvaging_an_earlier_path_keeps_values_when_the_replaced_pending_is_removed() {
    recover_over_an_existing_salvage("000-damaged.batch", true);
}

#[test]
fn salvaging_a_later_path_keeps_values_after_the_existing_pending_was_removed() {
    recover_over_an_existing_salvage("zzz-damaged.batch", false);
}

/// Capture saves before any seal, then stop the original handle separately from the
/// independent crashed image. The only journal copies of these exact new ids are pending.
fn unsealed_salvage_probe_fixture() -> (
    FaultFs,
    std::path::PathBuf,
    tc_journal::DeviceId,
    Vec<Verified>,
) {
    let (rig, folder) = titus();
    let project = project_of(&rig.fs, &folder);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let device = handle.facts().device;
    let mut saved_ids = Vec::new();
    saved_ids.extend(
        save_verse(&handle, 1, 1, "first same-path salvage save")
            .unwrap()
            .changes,
    );
    saved_ids.extend(
        save_verse(&handle, 1, 1, "second same-path salvage save")
            .unwrap()
            .changes,
    );
    saved_ids.extend(
        save_verse(&handle, 1, 2, "another same-path salvage save")
            .unwrap()
            .changes,
    );
    let mut lines: Vec<Verified> = pending_files(&rig.fs, &folder)
        .into_iter()
        .flat_map(|rel| {
            let bytes = rig.fs.peek(&folder.join(rel)).unwrap();
            let read = read_pending(&bytes, project);
            assert_eq!(read.status, PendingStatus::Valid);
            read.entries
        })
        .collect();
    lines.sort_by_key(Verified::sort_key);
    assert_eq!(lines.len(), saved_ids.len());
    assert!(lines.iter().all(|line| saved_ids.contains(&line.id())));
    assert_saved_history(&handle, &lines);
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let fs = rig.fs.crash(false);
    handle.close().unwrap();
    drop(rig);
    for rel in packs(&fs, &folder) {
        let bytes = fs.peek(&folder.join(&rel)).unwrap();
        let read = tc_journal::pack::read_pack(&rel, &bytes, project);
        assert!(
            read.entries
                .iter()
                .all(|line| !saved_ids.contains(&line.id()))
        );
    }
    for rel in pending_files(&fs, &folder) {
        fs.delete(&folder.join(rel)).unwrap();
    }
    assert_valid_burrito(&fs, &folder);
    (fs, folder, device, lines)
}

fn damaged_salvage_probe(
    project: tc_journal::ProjectId,
    device: tc_journal::DeviceId,
    lines: &[Verified],
    purpose: Purpose,
    tail: &[u8],
) -> (String, Vec<u8>) {
    let encoded = encode_pending(
        &PendingHeader {
            device,
            project,
            purpose,
            reconciles: None,
        },
        lines,
    )
    .unwrap();
    let mut damaged = encoded.bytes;
    damaged.extend_from_slice(tail);
    damaged.push(b'\n');
    let read = read_pending(&damaged, project);
    assert!(matches!(read.status, PendingStatus::Damaged(_)));
    assert_eq!(read.entries, lines);
    (encoded.name, damaged)
}

fn assert_quarantine_has_exact_probe_bytes(fs: &FaultFs, folder: &std::path::Path, bytes: &[u8]) {
    assert!(
        fs.files_under(&folder.join(".tc/quarantine"))
            .values()
            .any(|kept| kept.as_slice() == bytes),
        "quarantine lost or silently rewrote the original damaged bytes, including their tail"
    );
}

#[test]
fn salvaging_its_own_pending_path_keeps_every_saved_raw_value_readable() {
    let (fs, folder, device, lines) = unsealed_salvage_probe_fixture();
    let project = project_of(&fs, &folder);
    let (name, damaged) = damaged_salvage_probe(
        project,
        device,
        &lines,
        Purpose::Salvage,
        b"the first damaged tail must also remain intact",
    );
    let rel = format!(".tc/pending/{name}");
    fs.put_unseen(&folder.join(&rel), &damaged, None);
    assert_valid_burrito(&fs, &folder);
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        handle.facts().device,
        device,
        "the header's device is retained"
    );
    // Check exact public History before a retry or checkpoint can rescue a projection
    // under different ids.
    assert_saved_history(&handle, &lines);
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &damaged);
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &damaged);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_saved_history(&handle, &lines);
    handle.wait_idle();
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &damaged);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn repeated_consolidation_keeps_loaded_saved_values_readable_without_reopening() {
    let fs = FaultFs::new();
    let (settings, clock) = config(&fs, APP, "host-1");
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    let rig = Rig { fs, store };
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let saved = save_verse(
        &handle,
        1,
        1,
        "the original loaded value must remain readable",
    )
    .unwrap();
    assert_eq!(saved.changes.len(), 1);
    let id = saved.changes[0];
    let original = handle.snapshot().fold().raw(&id).unwrap();
    let line = tc_journal::parse_change(id, original);
    assert!(line.as_change().is_some());
    let lines = vec![line];
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    // Reopening makes the witness use the loader's FileSource rather than a value this
    // session retains in memory. All following consolidations run in this one session.
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    let project = project_of(&rig.fs, &folder);
    let mut consolidations = 0;
    for round in 0..12 {
        // Two packs suffice for a completed month. Unlike hundreds of current-month
        // small packs, this exercises the same production relocation with few saves.
        save_verse(
            &handle,
            1,
            2,
            &format!("another saved value in month {round}"),
        )
        .unwrap();
        handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .unwrap();
        handle.wait_idle();
        assert_saved_history(&handle, &lines);
        let before: std::collections::BTreeSet<String> =
            packs(&rig.fs, &folder).into_iter().collect();
        clock.advance(31 * 24 * 60 * 60 * 1000);
        let result = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
        // Read the exact original id before accepting the checkpoint result, so a
        // missing historical body is the primary failure even if tree building notices it.
        assert_saved_history(&handle, &lines);
        result.unwrap();
        handle.wait_idle();
        assert_saved_history(&handle, &lines);
        let after: std::collections::BTreeSet<String> =
            packs(&rig.fs, &folder).into_iter().collect();
        let rewritten = after
            .difference(&before)
            .filter(|rel| {
                let bytes = rig.fs.peek(&folder.join(rel)).unwrap();
                let read = tc_journal::pack::read_pack(rel, &bytes, project);
                read.is_valid()
                    && read.entries.iter().any(|line| line.id() == id)
                    && read.header.as_ref().is_some_and(|h| !h.replaces.is_empty())
            })
            .count();
        assert_eq!(
            rewritten, 1,
            "round {round} must relocate the witness into a new pack"
        );
        assert!(
            before.iter().any(|rel| !after.contains(rel)),
            "old packs must actually go"
        );
        consolidations += rewritten;
        assert_valid_burrito(&rig.fs, &folder);
    }
    assert_eq!(consolidations, 12);
    handle.close().unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_saved_history(&handle, &lines);
    handle.wait_idle();
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn quarantine_notices_keep_their_exact_routes_across_reopen_and_dismiss_only_one_image() {
    let (fs, folder, device, lines) = unsealed_salvage_probe_fixture();
    let project = project_of(&fs, &folder);
    let (_, first) =
        damaged_salvage_probe(project, device, &lines, Purpose::Save, b"first exact image");
    let (_, second) = damaged_salvage_probe(
        project,
        device,
        &lines,
        Purpose::Save,
        b"second exact image",
    );
    let rel = ".tc/pending/reused%2F~2.batch";
    fs.put_unseen(&folder.join(rel), &first, None);
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &first);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    // A cloud client can restore this name with another damaged version before the next
    // open, after the first was quarantined.
    rig.fs.put_unseen(&folder.join(rel), &second, None);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_saved_history(&handle, &lines);
    handle.wait_idle();
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &first);
    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &second);
    handle.close().unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let copies: Vec<_> = handle
        .notices()
        .into_iter()
        .filter(|n| {
            matches!(
                &n.notice, Notice::QuarantinedPending { file, .. } if file.as_ref() == rel
            )
        })
        .collect();
    assert_eq!(
        copies.len(),
        2,
        "both distinct damaged images stay discoverable after reopening"
    );
    let first_notice = copies
        .iter()
        .find(|n| {
            rig.fs
                .peek(&folder.join(n.copy.as_ref().unwrap()))
                .as_deref()
                == Some(first.as_slice())
        })
        .unwrap();
    let second_notice = copies
        .iter()
        .find(|n| {
            rig.fs
                .peek(&folder.join(n.copy.as_ref().unwrap()))
                .as_deref()
                == Some(second.as_slice())
        })
        .unwrap();
    handle.dismiss_notice(first_notice.id).wait().unwrap();
    assert!(
        rig.fs
            .peek(&folder.join(first_notice.copy.as_ref().unwrap()))
            .is_none()
    );
    assert_eq!(
        rig.fs
            .peek(&folder.join(second_notice.copy.as_ref().unwrap())),
        Some(second)
    );
    assert!(handle.notices().iter().any(|n| n.id == second_notice.id));
    assert_saved_history(&handle, &lines);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn every_salvage_open_crash_keeps_exact_history_and_the_complete_damaged_image() {
    use tc_store::fault::PowerLoss;
    let (base, folder, device, lines) = unsealed_salvage_probe_fixture();
    let project = project_of(&base, &folder);
    let (name, damaged) = damaged_salvage_probe(
        project,
        device,
        &lines,
        Purpose::Salvage,
        b"original damaged tail survives every write point",
    );
    let rel = format!(".tc/pending/{name}");
    base.put(&folder.join(&rel), &damaged, None);
    let mut tested = 0;
    for model in [PowerLoss::KeepSyncedNames, PowerLoss::Strict] {
        for torn in [false, true] {
            for point in 0..1000 {
                let fs = base.copy();
                fs.set_power_loss(model);
                let rig = rig_as(fs.clone(), APP, "host-1");
                fs.kill_before(point, torn);
                let opened = rig.store.open(&folder, &OpenOptions::default());
                if let Ok(handle) = &opened {
                    handle.wait_idle();
                }
                let dead = fs.is_dead();
                drop(opened);
                drop(rig);
                let cuts: &[bool] = if model == PowerLoss::Strict {
                    &[true]
                } else {
                    &[false, true]
                };
                for &power_cut in cuts {
                    let after = fs.crash(power_cut);
                    assert!(
                        after
                            .files_under(&folder.join(".tc"))
                            .values()
                            .any(|bytes| bytes == &damaged),
                        "lost original image before recovery: {model:?}, torn={torn}, cut={power_cut}, point={point}"
                    );
                    let rig = rig_as(after, APP, "host-1");
                    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
                    assert_saved_history(&handle, &lines);
                    handle.wait_idle();
                    assert_saved_history(&handle, &lines);
                    assert_quarantine_has_exact_probe_bytes(&rig.fs, &folder, &damaged);
                    assert!(
                        handle.notices().iter().any(|notice| {
                            notice.copy.as_ref().is_some_and(|copy| {
                                rig.fs.peek(&folder.join(copy)).as_deref()
                                    == Some(damaged.as_slice())
                            })
                        }),
                        "the recovered damaged image needs a reachable notice"
                    );
                    handle.close().unwrap();
                    assert_valid_burrito(&rig.fs, &folder);
                    assert_commits_valid(&rig.fs, &folder);
                }
                tested += 1;
                if !dead {
                    break;
                }
                assert!(point < 999, "the open never finished");
            }
        }
    }
    assert!(tested > 4, "the test must reach real recovery write points");
    eprintln!(
        "salvage-open crash matrix: {tested} write-point rows, both filesystem models and torn modes"
    );
}

mod quarantine_last_look_probe {
    use super::*;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tc_store::Vfs;
    use tc_store::vfs::{
        DirEntry, FileMeta, LockHandle, ReadHandle, WatchGuard, WatchSink, WriteHandle,
    };

    const REPLACEMENT: &[u8] = b"an outside program replaced the retained evidence";

    #[derive(Debug, Clone, Copy)]
    enum OutsideCopyChange {
        Delete,
        Replace,
    }

    #[derive(Debug)]
    struct InterruptedPreservation {
        source: PathBuf,
        quarantine: PathBuf,
        original: Vec<u8>,
        change: OutsideCopyChange,
    }

    #[derive(Debug)]
    struct ChangeQuarantineWhileSourceIsHeld {
        fs: FaultFs,
        plan: Mutex<Option<InterruptedPreservation>>,
        quarantine: PathBuf,
        changed_copy: Mutex<Option<PathBuf>>,
        fired: AtomicBool,
        deny_new_copies: AtomicBool,
    }

    impl Vfs for ChangeQuarantineWhileSourceIsHeld {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.fs.read(path)
        }
        fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
            self.fs.open(path)
        }
        fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
            self.fs.metadata(path)
        }
        fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
            self.fs.read_dir(path)
        }
        fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
            if self.deny_new_copies.load(Ordering::SeqCst) && path.starts_with(&self.quarantine) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "the quarantine is temporarily unavailable for a replacement copy",
                ));
            }
            self.fs.create_new(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.fs.rename(from, to)
        }
        fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.fs.rename_file_noreplace(from, to)
        }
        fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.fs.rename_dir_noreplace(from, to)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.fs.remove_file(path)
        }
        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            self.fs.remove_dir_all(path)
        }
        fn create_dir(&self, path: &Path) -> io::Result<()> {
            self.fs.create_dir(path)
        }
        fn sync_dir(&self, path: &Path) -> io::Result<()> {
            self.fs.sync_dir(path)
        }
        fn sync_file(&self, path: &Path) -> io::Result<()> {
            self.fs.sync_file(path)
        }
        fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
            self.fs.lock(path)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.fs.canonicalize(path)
        }
        fn secure_dir_chain(&self, path: &Path, made: &[PathBuf]) -> io::Result<()> {
            self.fs.secure_dir_chain(path, made)
        }
        fn outside_dir(&self, path: &Path) -> io::Result<PathBuf> {
            self.fs.outside_dir(path)
        }
        fn is_held(&self, error: &io::Error) -> bool {
            self.fs.is_held(error)
        }
        fn watch(
            &self,
            root: &Path,
            recursive: bool,
            sink: WatchSink,
        ) -> io::Result<Option<WatchGuard>> {
            self.fs.watch(root, recursive, sink)
        }
        fn wait_held(&self, delay: Duration) {
            self.fs.wait_held(delay);
            let Some(plan) = self.plan.lock().unwrap().take() else {
                return;
            };
            assert_eq!(self.fs.peek(&plan.source), Some(plan.original.clone()));
            let copies: Vec<PathBuf> = self
                .fs
                .files_under(&plan.quarantine)
                .into_iter()
                .filter_map(|(path, bytes)| (bytes == plan.original).then_some(path))
                .collect();
            assert_eq!(
                copies.len(),
                1,
                "the first removal attempt has one exact backup"
            );
            let copy = copies[0].clone();
            match plan.change {
                OutsideCopyChange::Delete => self.fs.delete(&copy).unwrap(),
                OutsideCopyChange::Replace => self.fs.put_unseen(&copy, REPLACEMENT, None),
            }
            assert_ne!(self.fs.peek(&copy), Some(plan.original));
            *self.changed_copy.lock().unwrap() = Some(copy);
            // This makes backup unavailability persist through this attempt. A fix
            // may re-preserve after release; it cannot assume the deleted copy exists.
            self.deny_new_copies.store(true, Ordering::SeqCst);
            self.fs.set_busy(&plan.source, false);
            self.fired.store(true, Ordering::SeqCst);
        }
    }

    fn damaged_source_is_still_discoverable(
        rig: &Rig,
        handle: &ProjectHandle,
        folder: &Path,
        rel: &str,
        original: &[u8],
    ) {
        let notices = handle.notices();
        assert!(
            notices.iter().any(|notice| match &notice.notice {
                // A safe deferral can report the source that actually still holds it.
                Notice::FileUnreadable { path, .. } if path.as_ref() == rel => true,
                Notice::QuarantinedPending { file, .. } if file.as_ref() == rel => {
                    notice.copy.as_ref().is_some_and(|copy| {
                        rig.fs.peek(&folder.join(copy)).as_deref() == Some(original)
                    })
                }
                _ => false,
            }),
            "no notice routes to the retained source or an actual exact original copy: {notices:?}"
        );
        assert!(
            notices
                .iter()
                .filter(|notice| matches!(
                    &notice.notice,
                    Notice::QuarantinedPending { file, .. } if file.as_ref() == rel
                ))
                .all(|notice| notice.copy.as_ref().is_some_and(|copy| {
                    rig.fs.peek(&folder.join(copy)).as_deref() == Some(original)
                })),
            "a quarantine notice must not point to the deleted or replaced copy: {notices:?}"
        );
    }

    fn source_retirement_rechecks_its_backup(change: OutsideCopyChange) {
        let (fs, folder, device, lines) = unsealed_salvage_probe_fixture();
        let project = project_of(&fs, &folder);
        let (_, original) = damaged_salvage_probe(
            project,
            device,
            &lines,
            Purpose::Save,
            b"original damaged tail whose last retained copy must survive this removal",
        );
        let rel = ".tc/pending/held-original-evidence.batch";
        let source = folder.join(rel);
        fs.put_unseen(&source, &original, None);
        fs.set_busy(&source, true);
        let quarantine = folder.join(".tc/quarantine");
        let wrapped = Arc::new(ChangeQuarantineWhileSourceIsHeld {
            fs: fs.clone(),
            plan: Mutex::new(Some(InterruptedPreservation {
                source: source.clone(),
                quarantine: quarantine.clone(),
                original: original.clone(),
                change,
            })),
            quarantine,
            changed_copy: Mutex::new(None),
            fired: AtomicBool::new(false),
            deny_new_copies: AtomicBool::new(false),
        });
        let (mut settings, _) = config(&fs, APP, "host-1");
        settings.vfs = wrapped.clone();
        let store = Store::start(settings).unwrap();
        store.wait_cleanup();
        watch_burrito(&store, &fs);
        let rig = Rig { fs, store };
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        rig.fs.set_busy(&source, false);
        assert!(
            wrapped.fired.load(Ordering::SeqCst),
            "the backup must change during the source removal's sharing wait"
        );
        // This is an external deletion/replacement, not a user dismissal. Once the
        // backup is unavailable, the app must keep its still-matching original source.
        assert_eq!(
            rig.fs.peek(&source),
            Some(original.clone()),
            "source retirement destroyed the last exact original after its backup vanished"
        );
        assert_saved_history(&handle, &lines);
        damaged_source_is_still_discoverable(&rig, &handle, &folder, rel, &original);
        let changed_copy = wrapped.changed_copy.lock().unwrap().clone().unwrap();
        if matches!(change, OutsideCopyChange::Replace) {
            assert_eq!(rig.fs.peek(&changed_copy).as_deref(), Some(REPLACEMENT));
        }
        // The open sealed the salvage its O7 wrote; its materialization lists the pack.
        handle.wait_idle();
        assert_valid_burrito(&rig.fs, &folder);
        wrapped.deny_new_copies.store(false, Ordering::SeqCst);
        handle.close().unwrap();

        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        assert_saved_history(&handle, &lines);
        handle.wait_idle();
        let notices = handle.notices();
        let notice = notices
            .iter()
            .find(|notice| {
                matches!(
                    &notice.notice,
                    Notice::QuarantinedPending { file, .. } if file.as_ref() == rel
                )
            })
            .expect("the actual kept original is linked after access returns");
        let copy = notice
            .copy
            .as_ref()
            .expect("the notice names its actual byte copy");
        assert_eq!(rig.fs.peek(&folder.join(copy)), Some(original));
        if matches!(change, OutsideCopyChange::Replace) {
            assert_eq!(
                rig.fs.peek(&changed_copy).as_deref(),
                Some(REPLACEMENT),
                "the outside replacement of the old copy is also retained unchanged"
            );
        }
        handle.close().unwrap();
        assert_valid_burrito(&rig.fs, &folder);
        assert_commits_valid(&rig.fs, &folder);
    }

    #[test]
    fn a_quarantine_backup_deleted_during_a_sharing_wait_does_not_destroy_its_source() {
        source_retirement_rechecks_its_backup(OutsideCopyChange::Delete);
    }

    #[test]
    fn a_quarantine_backup_replaced_during_a_sharing_wait_does_not_destroy_its_source() {
        source_retirement_rechecks_its_backup(OutsideCopyChange::Replace);
    }
}

#[test]
fn sealing_duplicate_verified_lines_keeps_the_first_loaded_history_span_readable() {
    let (fs, folder, device, lines) = unsealed_salvage_probe_fixture();
    let project = project_of(&fs, &folder);
    let duplicates: Vec<Verified> = lines
        .iter()
        .flat_map(|line| [line.clone(), line.clone()])
        .collect();
    let encoded = encode_pending(
        &PendingHeader {
            device,
            project,
            purpose: Purpose::Save,
            reconciles: None,
        },
        &duplicates,
    )
    .unwrap();
    let read = read_pending(&encoded.bytes, project);
    assert_eq!(read.status, PendingStatus::Valid);
    assert_eq!(read.entries.len(), 2 * lines.len());
    fs.put(
        &folder.join(format!(".tc/pending/{}", encoded.name)),
        &encoded.bytes,
        None,
    );
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    let checkpoint = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
    assert_saved_history(&handle, &lines);
    checkpoint.unwrap();
    handle.wait_idle();
    assert_saved_history(&handle, &lines);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_saved_history(&handle, &lines);
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

fn changed_quarantine_copy_is_reported(valid_replacement: bool) {
    let (fs, folder, device, lines) = unsealed_salvage_probe_fixture();
    let project = project_of(&fs, &folder);
    let (_, damaged) = damaged_salvage_probe(
        project,
        device,
        &lines,
        Purpose::Save,
        b"original complete evidence",
    );
    let rel = ".tc/pending/changed-evidence.batch";
    fs.put_unseen(&folder.join(rel), &damaged, None);
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let copy = handle
        .notices()
        .into_iter()
        .find_map(|n| {
            n.copy.filter(|copy| {
                rig.fs.peek(&folder.join(copy)).as_deref() == Some(damaged.as_slice())
            })
        })
        .unwrap();
    handle.close().unwrap();
    let replacement = if valid_replacement {
        encode_pending(
            &PendingHeader {
                device,
                project,
                purpose: Purpose::Save,
                reconciles: None,
            },
            &lines,
        )
        .unwrap()
        .bytes
    } else {
        damaged_salvage_probe(
            project,
            device,
            &lines,
            Purpose::Save,
            b"different damaged evidence supplied outside",
        )
        .1
    };
    assert_ne!(replacement, damaged);
    rig.fs.put_unseen(&folder.join(&copy), &replacement, None);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_saved_history(&handle, &lines);
    handle.wait_idle();
    assert!(
        handle.notices().iter().any(|n| {
            matches!(&n.notice, Notice::FileUnreadable { path, .. } if path.as_ref() == copy)
                && n.copy.as_deref() == Some(copy.as_str())
        }),
        "a changed preserved image must stay visible through its actual path, including a valid replacement: {:?}",
        handle.notices()
    );
    assert!(
        !handle.notices().iter().any(|n| {
            matches!(&n.notice, Notice::QuarantinedPending { file, .. } if file.as_ref() == rel)
                && n.copy.as_deref() == Some(copy.as_str())
        }),
        "changed bytes cannot be presented as the original damaged image"
    );
    assert_eq!(rig.fs.peek(&folder.join(copy)), Some(replacement));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

#[test]
fn replacing_a_quarantine_image_with_other_damage_is_reported_without_reattribution() {
    changed_quarantine_copy_is_reported(false);
}

#[test]
fn replacing_a_quarantine_image_with_valid_bytes_does_not_hide_the_change() {
    changed_quarantine_copy_is_reported(true);
}
