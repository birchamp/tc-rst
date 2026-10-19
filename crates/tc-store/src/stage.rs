//! A large batch is written off the actor (`docs/STORAGE.md` §5.0 "A large batch", §5.8).
//!
//! A resource upgrade that carries every decision of a whole Bible is 150 to 260 MB of
//! changes. Built, written and folded on the actor it holds every save back for seconds and
//! the process for gigabytes (`docs/evidence/storage-scale-2026-10-19.md`). Past
//! [`STAGE_CHANGES`] changes the actor reserves the batch's sequence numbers and hands the
//! plan to a worker thread; saves go on meanwhile. The worker
//!
//! 1. checks the plan against the fold as it was at receipt (every record still has the
//!    parents the plan was made from);
//! 2. builds the changes and writes them as packs of at most 64 MiB into
//!    `.tc/staged/<batch>/`, outside the burrito, in 4 MiB pieces that each end in a flush
//!    (a flush waits for the drive's queue, so no single one may carry more), reading every
//!    pack back;
//! 3. folds the batch into a copy of the fold taken at receipt, with the values left in the
//!    packs (the copy is a persistent structure; the actor's fold goes on meanwhile);
//! 4. tells the actor it is ready. The actor refuses the batch when a save touched a record
//!    of it since receipt (`PlanStale`: "a newer change just arrived, look again") and
//!    otherwise says go;
//! 5. writes `commit.json` (the decision, durable) and moves the packs into
//!    `ingredients/tc/changes/` (a rename each, one flush of each folder), then removes its
//!    folder. A crash after `commit.json` is finished by the next open
//!    ([`recover`]); a crash before it leaves a folder the next open removes, so the batch
//!    is all there or not at all.
//!
//! The actor then adds the batches it committed meanwhile to the worker's fold and swaps it
//! in. The fold is a function of the set of changes, so the result is the one folding them in
//! the order of arrival would give (I1). A save that touched a plan record after the check
//! and before the swap is a sibling of the carried change: a visible conflict, never a
//! replacement (I3).

use crate::atomic::{DirSet, HeldRetry, SyncDir, atomic_create_dirs, atomic_write, rename_held};
use crate::error::StoreError;
use crate::handle::{Msg, Shared, StoreEvent, Work};
use crate::layout::{Folder, STAGED};
use crate::progress::{Phase, start};
use crate::reply::{CommandError, Refusal};
use crate::source::FileSource;
use crate::vfs::{Vfs, is_not_found};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;
use tc_journal::command::UpgradePlan;
use tc_journal::fold::{Delta, FoldInput, FoldState};
use tc_journal::index::{Section, load_pack};
use tc_journal::line::LINE_OVERHEAD;
use tc_journal::pack::{MAX_PACK_BYTES, PackSpec, encode_pack, read_pack};
use tc_journal::{
    BatchId, BatchRef, Change, ChangeBody, ChangeId, DeviceId, DeviceSeq, FORMAT_MINOR, PackId,
    ProjectId, RecordKey, Verified, WallTime,
};

/// A planned batch of more than this many changes is staged. Only a resource upgrade reaches
/// it in practice; an added book is about 2,500 changes and is written as any save.
pub(crate) const STAGE_CHANGES: usize = 2_048;

/// The most the worker writes before it flushes: the drive's flush queue is what a save waits
/// for, so no flush may carry more than this.
const WRITE_PIECE: usize = 4 * 1024 * 1024;

/// The bytes of lines a staged pack holds before the next one starts: well under the 64 MiB
/// limit of a pack, so the header and the rounding never push one over.
const PACK_LINES: usize = MAX_PACK_BYTES / 4 * 3;

/// The file that decides a staged batch: present and durable means finish it, absent means
/// remove everything.
pub(crate) const COMMIT_FILE: &str = "commit.json";

/// A pack the worker wrote and read back.
pub(crate) struct StagedPack {
    /// Where it goes: `ingredients/tc/changes/<sealer>/<yyyy-mm>/<name>`.
    pub rel: String,
    pub id: PackId,
    pub size: u64,
    pub fingerprint: u64,
    pub number: u32,
    pub section: Option<Section>,
}

/// What the actor hands the worker.
pub(crate) struct StageJob {
    pub id: u64,
    pub plan: UpgradePlan,
    /// The fold as it was when the actor received the command.
    pub fold: FoldState,
    pub project: ProjectId,
    pub device: DeviceId,
    pub first_seq: u64,
    pub at: WallTime,
    pub batch: BatchId,
    pub folder: Folder,
    /// The raw filesystem; the worker paces it behind saves itself.
    pub vfs: Arc<dyn Vfs>,
    pub source: Arc<FileSource>,
    pub shared: Arc<Shared>,
    pub actor: Sender<Msg>,
    /// The actor's answer to [`Msg::StagedReady`].
    pub verdict: Receiver<bool>,
}

/// What the worker tells the actor when the batch is written and folded.
pub(crate) struct Ready {
    pub id: u64,
    pub packs: Vec<StagedPack>,
    /// The records the batch changes.
    pub keys: HashSet<RecordKey>,
    pub ids: Vec<ChangeId>,
    pub fold: FoldState,
    pub delta: Delta,
    pub last_seq: u64,
}

/// The packs of a batch, the fold inputs of its changes and their ids.
type Written = (Vec<StagedPack>, Vec<FoldInput>, Vec<ChangeId>);

/// Why a staged batch ended without being written.
pub(crate) enum Failure {
    Command(CommandError),
    Store(StoreError),
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<CommandError> for Failure {
    fn from(e: CommandError) -> Self {
        Self::Command(e)
    }
}

/// The folder of one staged batch.
pub(crate) fn batch_dir(folder: &Folder, batch: BatchId) -> PathBuf {
    folder.path(&format!("{STAGED}/{}", batch.to_hex()))
}

/// Runs a staged batch on the thread it is called on. Everything it sends the actor is a
/// [`Msg`]; the thread ends when the batch has been committed or given up.
pub(crate) fn run(job: StageJob) {
    let (id, actor, shared) = (job.id, job.actor.clone(), job.shared.clone());
    let dir = batch_dir(&job.folder, job.batch);
    let vfs = job.vfs.clone();
    let result = work(job);
    if let Err(failure) = result {
        // Nothing the batch wrote is kept before the decision.
        let decided = crate::stage::decided(&*vfs, &dir);
        if !decided {
            let _ = crate::atomic::remove_dir_all_held(&*vfs, &dir, &HeldRetry::SAVE);
        }
        let _ = actor.send(Msg::StagedFailed {
            id,
            error: match failure {
                Failure::Command(e) => e,
                Failure::Store(e) => CommandError::NotWritten(e.to_string()),
            },
        });
    }
    drop(shared);
}

/// Whether `commit.json` of the batch folder is in place.
fn decided(vfs: &dyn Vfs, dir: &Path) -> bool {
    vfs.metadata(&dir.join(COMMIT_FILE)).is_ok()
}

fn work(job: StageJob) -> Result<(), Failure> {
    let StageJob {
        id,
        plan,
        fold,
        project,
        device,
        first_seq,
        at,
        batch,
        folder,
        vfs,
        source,
        shared,
        actor,
        verdict,
    } = job;
    let paced: Arc<dyn Vfs> = Arc::new(crate::pace::PacedVfs::new(
        vfs,
        shared.gate.clone(),
        shared.counters.clone(),
    ));
    let vfs: &dyn Vfs = &*paced;
    let total = plan.changes.len();

    // 1. The plan against the fold at receipt (§5.8 step 1).
    let mut keys: HashSet<RecordKey> = HashSet::with_capacity(total);
    for planned in &plan.changes {
        let key = planned.record.key();
        let mut want = planned.parents.clone();
        want.sort();
        want.dedup();
        if fold.heads(&key) != want {
            return Err(CommandError::PlanStale.into());
        }
        if !keys.insert(key) {
            return Err(CommandError::Refused(Refusal::TwoChangesOfOneRecord).into());
        }
    }
    let count = u32::try_from(total)
        .map_err(|_| CommandError::Refused(Refusal::Rule("the batch is too large".into())))?;
    let batch_ref = BatchRef::new(batch, count)
        .ok_or_else(|| CommandError::Refused(Refusal::Rule("an empty batch".into())))?;

    // Progress from 300 ms on, off this thread (PERFORMANCE.md rule 4).
    let sink: crate::progress::ProgressSink = {
        let shared = shared.clone();
        Arc::new(move |progress| {
            shared.emit(StoreEvent::Progress {
                work: Work::Batch,
                progress,
            });
        })
    };
    let (tracker, reporter) = start(sink, Instant::now());
    tracker.phase(Phase::Working, total as u64, 0);
    let finished = (|| -> Result<Written, Failure> {
        let dir = batch_dir(&folder, batch);
        atomic_create_dirs(vfs, &dir)?;
        let spec = PackSpec {
            project,
            sealer: device,
            sealed: at,
        };
        let mut packs: Vec<StagedPack> = Vec::new();
        let mut inputs: Vec<FoldInput> = Vec::with_capacity(total);
        let mut ids: Vec<ChangeId> = Vec::with_capacity(total);
        let mut lines: Vec<Verified> = Vec::new();
        let mut bytes = 0usize;
        let mut flush = |lines: &mut Vec<Verified>, bytes: &mut usize| -> Result<(), Failure> {
            if lines.is_empty() {
                return Ok(());
            }
            let n = lines.len() as u64;
            let written = write_pack(vfs, &folder, &dir, &spec, std::mem::take(lines), &source)?;
            tracker.add(n, *bytes as u64);
            *bytes = 0;
            ids.extend(written.ids);
            inputs.extend(written.inputs);
            packs.push(written.pack);
            Ok(())
        };
        for (seq, planned) in (first_seq..).zip(plan.changes) {
            let change = Change::new(ChangeBody {
                v: FORMAT_MINOR,
                record: planned.record,
                parents: planned.parents,
                device,
                seq: DeviceSeq(seq),
                at,
                batch: batch_ref,
                cause: planned.cause,
            })
            .map_err(|e| CommandError::Refused(Refusal::Rule(e.to_string())))?;
            let line = Verified::from(change);
            bytes += line.raw().len() + LINE_OVERHEAD;
            lines.push(line);
            if bytes >= PACK_LINES {
                flush(&mut lines, &mut bytes)?;
            }
        }
        flush(&mut lines, &mut bytes)?;
        Ok((packs, inputs, ids))
    })();
    let (packs, inputs, ids) = match finished {
        Ok(done) => done,
        Err(failure) => {
            reporter.finish(&tracker);
            return Err(failure);
        }
    };

    // 3. The batch folded into the copy: values stay in the packs.
    let mut folded = fold;
    let delta = folded.add(inputs);
    shared.hooks.wait(crate::hooks::Point::BeforeStagedReady);
    let last_seq = first_seq + total as u64 - 1;
    let names: Vec<String> = packs.iter().map(|p| p.rel.clone()).collect();
    let ready = Ready {
        id,
        packs,
        keys,
        ids,
        fold: folded,
        delta,
        last_seq,
    };
    if actor.send(Msg::StagedReady(Box::new(ready))).is_err() {
        reporter.finish(&tracker);
        return Err(StoreError::Other("the project closed".into()).into());
    }
    // 4. The actor's word. A batch it refuses is removed.
    let go = verdict.recv().unwrap_or(false);
    if !go {
        reporter.finish(&tracker);
        let _ =
            crate::atomic::remove_dir_all_held(vfs, &batch_dir(&folder, batch), &HeldRetry::SAVE);
        return Ok(());
    }
    shared.hooks.wait(crate::hooks::Point::BeforeStagedCommit);
    // 5. The decision and the moves.
    let committed = commit(vfs, &folder, batch, &names);
    reporter.finish(&tracker);
    let _ = actor.send(Msg::StagedCommitted {
        id,
        result: committed.map_err(|e| e.to_string()),
    });
    Ok(())
}

/// One pack written into the batch's folder in pieces, flushed, read back and loaded.
struct WrittenPack {
    pack: StagedPack,
    ids: Vec<ChangeId>,
    inputs: Vec<FoldInput>,
}

fn write_pack(
    vfs: &dyn Vfs,
    folder: &Folder,
    dir: &Path,
    spec: &PackSpec,
    lines: Vec<Verified>,
    source: &FileSource,
) -> Result<WrittenPack, Failure> {
    let encoded = encode_pack(spec, &lines, &[])
        .map_err(|e| CommandError::Refused(Refusal::Rule(e.to_string())))?;
    drop(lines);
    let rel = encoded.path.to_string();
    let name = rel.rsplit_once('/').map_or(rel.as_str(), |(_, name)| name);
    let target = dir.join(name);
    let part = dir.join(format!("{name}.part"));
    let mut file = vfs
        .create_new(&part)
        .map_err(|e| StoreError::io("create", &part, e))?;
    for piece in encoded.bytes.chunks(WRITE_PIECE) {
        file.write_all(piece)
            .map_err(|e| StoreError::io("write", &part, e))?;
        file.sync_all()
            .map_err(|e| StoreError::io("flush", &part, e))?;
    }
    file.sync_all()
        .map_err(|e| StoreError::io("flush", &part, e))?;
    drop(file);
    let back = vfs
        .read(&part)
        .map_err(|e| StoreError::io("read back", &part, e))?;
    if back != encoded.bytes {
        return Err(StoreError::Other(format!(
            "the staged pack {rel} did not read back as written"
        ))
        .into());
    }
    drop(back);
    rename_held(vfs, &part, &target, &HeldRetry::SAVE)
        .map_err(|e| StoreError::io("rename", &target, e))?;
    let number = source.add(&folder.path(&rel));
    let loaded = load_pack(&rel, &encoded.bytes, spec.project, number);
    Ok(WrittenPack {
        pack: StagedPack {
            rel,
            id: encoded.id,
            size: encoded.bytes.len() as u64,
            fingerprint: tc_journal::index::fingerprint(&encoded.bytes),
            number,
            section: loaded.section,
        },
        ids: encoded.changes,
        inputs: loaded.inputs,
    })
}

/// The decision and the moves (step 5), and the same again after a crash ([`recover`]).
fn commit(
    vfs: &dyn Vfs,
    folder: &Folder,
    batch: BatchId,
    packs: &[String],
) -> Result<(), StoreError> {
    let dir = batch_dir(folder, batch);
    // The staged files are in their names and their folder is durable.
    vfs.sync_dir(&dir)
        .map_err(|e| StoreError::io("flush the folder", &dir, e))?;
    let manifest = serde_json::json!({ "staged": 1, "batch": batch.to_hex(), "packs": packs });
    let bytes = serde_json::to_vec(&manifest).map_err(|e| StoreError::Other(e.to_string()))?;
    atomic_write(vfs, &dir, COMMIT_FILE, &bytes, SyncDir::Now).map_err(|e| e.into_inner())?;
    roll_forward(vfs, folder, &dir, None)
}

/// Moves the packs `commit.json` lists from `dir` into the project and removes `dir`. A pack
/// already in place stays; one that is in neither place is an error. With `project`, each
/// staged pack is verified first (a recovery trusts nothing it did not just write).
fn roll_forward(
    vfs: &dyn Vfs,
    folder: &Folder,
    dir: &Path,
    project: Option<ProjectId>,
) -> Result<(), StoreError> {
    let bytes = vfs
        .read(&dir.join(COMMIT_FILE))
        .map_err(|e| StoreError::io("read", dir, e))?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| StoreError::Other(format!("a staged batch's commit.json: {e}")))?;
    let listed: Vec<String> = manifest
        .get("packs")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        })
        .ok_or_else(|| StoreError::Other("a staged batch's commit.json lists no packs".into()))?;
    let mut moves = Vec::new();
    for rel in &listed {
        let target = folder.path(rel);
        if vfs.metadata(&target).is_ok() {
            continue;
        }
        let name = rel.rsplit_once('/').map_or(rel.as_str(), |(_, name)| name);
        let staged = dir.join(name);
        if let Some(project) = project {
            let bytes = vfs
                .read(&staged)
                .map_err(|e| StoreError::io("read", &staged, e))?;
            if !read_pack(rel, &bytes, project).is_valid() {
                return Err(StoreError::Other(format!(
                    "the staged pack {rel} does not verify; the batch is kept in {}",
                    dir.display()
                )));
            }
        } else if vfs.metadata(&staged).is_err() {
            return Err(StoreError::Other(format!(
                "the staged pack {rel} is in neither place"
            )));
        }
        moves.push((staged, target));
    }
    let mut dirs = DirSet::new();
    for (staged, target) in moves {
        let parent = target
            .parent()
            .ok_or_else(|| StoreError::Other("a pack with no folder".into()))?;
        atomic_create_dirs(vfs, parent)?;
        rename_held(vfs, &staged, &target, &HeldRetry::SAVE)
            .map_err(|e| StoreError::io("rename", &target, e))?;
        dirs.add_dir(parent);
        // Windows keeps a rename in its log until the renamed file is flushed.
        let _ = vfs.sync_file(&target);
    }
    dirs.sync(vfs)?;
    crate::atomic::remove_dir_all_held(vfs, dir, &HeldRetry::SAVE)
        .map_err(|e| StoreError::io("remove", dir, e))?;
    if let Some(parent) = dir.parent() {
        vfs.sync_dir(parent)
            .map_err(|e| StoreError::io("flush the folder", parent, e))?;
    }
    Ok(())
}

/// O2 for staged batches (§6.1): a batch folder with its `commit.json` is finished; one
/// without it never got its decision and is removed whole. Runs before the journal is read.
pub(crate) fn recover(
    vfs: &dyn Vfs,
    folder: &Folder,
    project: ProjectId,
) -> Result<(), StoreError> {
    let root = folder.path(STAGED);
    let entries = match vfs.read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if is_not_found(&e) => return Ok(()),
        Err(e) => return Err(StoreError::io("list", &root, e)),
    };
    for entry in entries {
        let dir = root.join(&entry.name);
        if decided(vfs, &dir) {
            roll_forward(vfs, folder, &dir, Some(project))?;
        } else {
            let _ = crate::atomic::remove_dir_all_held(vfs, &dir, &HeldRetry::SAVE);
        }
    }
    vfs.sync_dir(&root)
        .map_err(|e| StoreError::io("flush the folder", &root, e))?;
    Ok(())
}
