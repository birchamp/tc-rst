//! O2: what the open does with the temporary files a stopped writer left (`docs/STORAGE.md`
//! §6.1, §5.0 step 5).
//!
//! A temporary file of a pack whose bytes verify is renamed into place (a power cut on
//! Windows can keep the temp and lose the rename after "Saved"). A temporary file that does
//! not verify is removed. A temporary file of a pending file that verifies is not placed
//! at once: it can be a save the person was told had failed (a scanner held the file past the
//! retries, §5.0 B3), and the person may have saved again at the same sequence number. The
//! open therefore decides after it has read the journal ([`settle_temps`]): a temp that
//! shares a `(device, seq)` with a change the project holds is kept as a copy in
//! `.tc/external/`, never folded beside the later save, where it would look like another
//! computer saving as this one.
//!
//! A temporary file that cannot be read is never removed, and a removal or rename that a
//! scanner still refuses leaves the file for the next open. The open never fails for them.

use crate::appdata::utc_stamp;
use crate::atomic::{
    DirSet, HeldRetry, SyncDir, atomic_create_dirs, atomic_remove_if_free, rename_held, temp_target,
};
use crate::clock::Clock;
use crate::error::StoreError;
use crate::layout::{self, EXTERNAL, Folder, PACKS, PENDING};
use crate::project::{Journal, PendingInfo};
use crate::vfs::{EntryKind, Vfs, is_not_found};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tc_journal::fold::{FoldInput, FoldState, Notice};
use tc_journal::pack::read_pack;
use tc_journal::pending::{PendingStatus, encode_pending, read_pending};
use tc_journal::{BatchId, ChangeId, DeviceId, ProjectId, Verified, WallTime};

/// A complete temporary file of a pending file, to be placed or kept once the fold exists.
#[derive(Debug)]
pub(crate) struct DeferredTemp {
    abs: PathBuf,
    /// Where it goes: the pending file its name is for.
    target: PathBuf,
    /// The target relative to the project folder.
    rel: String,
    name: String,
    entries: Vec<Verified>,
    at: WallTime,
    batch: BatchId,
}

/// What O2 found before the journal was read.
#[derive(Debug, Default)]
pub(crate) struct Temps {
    pub deferred: Vec<DeferredTemp>,
    /// A temporary file of a pending file could not be read: it may hold sequence numbers
    /// this device used, so a new save cannot reuse any (§6.3).
    pub pending_unreadable: bool,
}

/// What [`settle_temps`] decided, for O6's identity rule.
#[derive(Debug, Default)]
pub(crate) struct Settled {
    /// A temp that could not be placed or kept holds changes of these devices up to this
    /// seq; the ledger rule counts them as held, and a save never reuses one.
    pub unplaced_seq: BTreeMap<DeviceId, u64>,
    pub pending_unreadable: bool,
}

/// A pending file's name and the bytes' changes, when `bytes` are a complete pending file
/// named `name` (its name is the hash of its header and ids).
fn complete_pending(
    name: &str,
    bytes: &[u8],
    project: ProjectId,
) -> Option<(tc_journal::pending::PendingHeader, Vec<Verified>)> {
    let read = read_pending(bytes, project);
    let header = read.header.clone()?;
    if read.status != PendingStatus::Valid {
        return None;
    }
    encode_pending(&header, &read.entries)
        .ok()
        .filter(|encoded| encoded.name == name && encoded.bytes == bytes)?;
    Some((header, read.entries))
}

/// Whether `bytes` are a complete pack at `rel` (P2 to P4; its name is the hash of its
/// header and ids).
fn complete_pack(rel: &str, bytes: &[u8], project: ProjectId) -> bool {
    rel.starts_with(PACKS) && read_pack(rel, bytes, project).is_valid()
}

/// O2, first half: places the complete pack temps, removes every temp that does not verify
/// and returns the complete pending temps for [`settle_temps`].
pub(crate) fn finish_temps(
    vfs: &dyn Vfs,
    folder: &Folder,
    project: ProjectId,
) -> Result<Temps, StoreError> {
    let mut temps = Temps::default();
    let mut stack: Vec<(PathBuf, String)> = vec![(folder.root().to_owned(), String::new())];
    let pending_dir = format!("{PENDING}/");
    while let Some((dir, rel)) = stack.pop() {
        let entries = match vfs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if is_not_found(&e) => continue,
            Err(e) => return Err(StoreError::io("list", &dir, e)),
        };
        let mut dirs = DirSet::new();
        for entry in entries {
            let name = entry.name.to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if entry.kind == EntryKind::Dir {
                if child != ".git" && !name.starts_with(".staging-") {
                    stack.push((dir.join(&entry.name), child));
                }
                continue;
            }
            let Some(target) = temp_target(&name) else {
                continue;
            };
            let abs = dir.join(&entry.name);
            let target_rel = if rel.is_empty() {
                target.to_owned()
            } else {
                format!("{rel}/{target}")
            };
            let is_pending = target_rel.starts_with(&pending_dir);
            let bytes = match vfs.read(&abs) {
                Ok(bytes) => bytes,
                Err(e) if is_not_found(&e) => continue,
                Err(_) => {
                    // Whatever cannot be read is never removed.
                    temps.pending_unreadable |= is_pending;
                    continue;
                }
            };
            if is_pending {
                if let Some((_, entries)) = complete_pending(target, &bytes, project) {
                    let first = entries.iter().find_map(Verified::as_change);
                    temps.deferred.push(DeferredTemp {
                        abs,
                        target: dir.join(target),
                        rel: target_rel,
                        name: target.to_owned(),
                        at: first.map_or(WallTime(0), |c| c.body().at),
                        batch: first.map_or(BatchId::from_bytes([0; 16]), |c| c.body().batch.id),
                        entries,
                    });
                    continue;
                }
            } else if complete_pack(&target_rel, &bytes, project) {
                if rename_held(vfs, &abs, &dir.join(target), &HeldRetry::SAVE).is_ok() {
                    dirs.add_dir(&dir);
                    // Windows keeps a rename in its log until the renamed file is flushed.
                    let _ = vfs.sync_file(&dir.join(target));
                }
                continue;
            }
            // Not a complete file of anything: removed, unless a scanner still holds it.
            let _ = atomic_remove_if_free(vfs, &abs, SyncDir::Deferred(&mut dirs));
        }
        dirs.sync(vfs)?;
    }
    Ok(temps)
}

/// O2, second half, after O8 has read the journal: each complete pending temp is a
/// duplicate of a file in place (removed), a save that was told it failed and was saved past
/// (kept as a copy in `.tc/external/`), or a save that was written and not renamed (placed
/// and folded).
///
/// The temps are walked newest first, by `(at, batch)`. A temp collides when one of its
/// changes has a `(device, seq)` that the fold maps to another change, or that a temp placed
/// earlier in the walk holds: the later attempt is the person's, and the earlier one is the
/// copy.
pub(crate) fn settle_temps(
    vfs: &dyn Vfs,
    folder: &Folder,
    clock: &dyn Clock,
    fold: &mut FoldState,
    journal: &mut Journal,
    notices: &mut Vec<(Notice, Option<String>)>,
    mut deferred: Vec<DeferredTemp>,
) -> Settled {
    let mut settled = Settled::default();
    deferred.sort_by_key(|t| std::cmp::Reverse((t.at, t.batch)));
    let mut dirs = DirSet::new();
    for temp in deferred {
        let ids: Vec<ChangeId> = temp.entries.iter().map(Verified::id).collect();
        // Step 1: the same file is in place and folded.
        if ids.iter().all(|id| fold.change(id).is_some()) {
            let _ = atomic_remove_if_free(vfs, &temp.abs, SyncDir::Deferred(&mut dirs));
            continue;
        }
        // The pending file of the same name is in place but not folded (it could not be
        // read): this copy is not rolled over it.
        if vfs.metadata(&temp.target).is_ok() {
            settled.pending_unreadable = true;
            continue;
        }
        let collides = temp
            .entries
            .iter()
            .filter_map(Verified::as_change)
            .any(|c| {
                fold.change_at(c.body().device, c.body().seq)
                    .is_some_and(|other| other != c.id())
            });
        if collides {
            match keep_as_copy(vfs, folder, clock, &temp) {
                Ok(copy) => {
                    dirs.add_dir(&folder.path(EXTERNAL));
                    dirs.add_dir(&folder.path(PENDING));
                    notices.push((
                        Notice::UnsavedAttemptKept {
                            copy: copy.as_str().into(),
                            changes: u32::try_from(ids.len()).unwrap_or(u32::MAX),
                            at: temp.at,
                        },
                        // Dismissing the notice never removes the person's text.
                        None,
                    ));
                }
                Err(_) => remember_unplaced(&mut settled, &temp),
            }
            continue;
        }
        if rename_held(vfs, &temp.abs, &temp.target, &HeldRetry::SAVE).is_err() {
            remember_unplaced(&mut settled, &temp);
            continue;
        }
        dirs.add_dir(&folder.path(PENDING));
        // Windows keeps a rename in its log until the renamed file is flushed.
        let _ = vfs.sync_file(&temp.target);
        journal.pending.insert(
            temp.rel.clone(),
            PendingInfo {
                ids,
                reconciles: None,
                number: None,
                status: PendingStatus::Valid,
            },
        );
        fold.add(temp.entries.into_iter().map(FoldInput::from));
    }
    let _ = dirs.sync(vfs);
    settled
}

/// A temp that is neither placed nor kept: its sequence numbers are not free.
fn remember_unplaced(settled: &mut Settled, temp: &DeferredTemp) {
    settled.pending_unreadable = true;
    for change in temp.entries.iter().filter_map(Verified::as_change) {
        let seq = settled
            .unplaced_seq
            .entry(change.body().device)
            .or_default();
        *seq = (*seq).max(change.body().seq.0);
    }
}

/// Moves the temp to `.tc/external/<utc>-not-saved-<name>`; its bytes stay what the person's
/// save attempt wrote.
fn keep_as_copy(
    vfs: &dyn Vfs,
    folder: &Folder,
    clock: &dyn Clock,
    temp: &DeferredTemp,
) -> Result<String, StoreError> {
    let dir = folder.path(EXTERNAL);
    atomic_create_dirs(vfs, &dir)?;
    let base = format!("{}-not-saved-{}", utc_stamp(clock.now()), temp.name);
    let mut name = base.clone();
    for n in 2..1000 {
        match vfs.metadata(&dir.join(&name)) {
            Err(e) if is_not_found(&e) => break,
            _ => name = format!("{base}-{n}"),
        }
    }
    rename_held(vfs, &temp.abs, &dir.join(&name), &HeldRetry::SAVE)
        .map_err(|e| StoreError::io("keep as a copy", &temp.abs, e))?;
    Ok(format!("{}/{name}", layout::EXTERNAL))
}
