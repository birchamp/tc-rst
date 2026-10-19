//! Repack (`docs/STORAGE.md` §9.8). libgit2 has no `gc`, so a project that commits for years
//! would keep every object as a loose file. When the loose objects pass 2,000 or 64 MiB, the
//! ones the branches, remotes and `refs/tc/` reach are written into one new pack, and the
//! loose files that a pack now holds are removed. Objects nothing reaches are removed only
//! when they are older than 14 days.
//!
//! Only the loose objects are packed, not the whole history again: a repack that wrote
//! every reachable object each time would add the history's size to the disk at each run.
//! A run packs at most [`MAX_RAW`] bytes (before compression) of them, and the next check
//! finds the rest still due: the search for deltas between the versions of a derived file
//! (they are what keeps the history of a whole Bible in a few hundred MiB) took 26 s for the
//! loose objects of 250 checkpoints. It runs on a thread of its own, never on the writer
//! (a materialization waits for it otherwise), and it stops when the project closes: libgit2
//! asks the progress callback twice a second whether to go on, and a stopped run removes
//! nothing. Nothing is removed before the pack and its index are on disk (libgit2 flushes
//! both and the folder when `core.fsyncObjectFiles` is on, `pack-objects.c`
//! `git_packbuilder_write`), and the index is read back to confirm that it names every object
//! about to lose its loose file. A repack that stops anywhere in between leaves objects
//! twice, never fewer (§6.4 R1); the next one finds them in a pack and removes the loose
//! copies, and a half-written `objects/pack/tmp_*` is removed by the next open (§6.1 O2).
//!
//! Callers must not repack during a sync (§9.8).

use crate::counters::StoreCounters;
#[cfg(test)]
use crate::git::to_oid;
use crate::git::{Git, GitError, at};
use crate::hooks::{Hooks, Point};
use crate::writer::Writer;
use git2::{ErrorCode, ObjectType, Oid, Repository, Sort};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// A loose object nothing reaches is removed once it is this old.
pub const PRUNE_AFTER: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// A repack is due when the loose objects are more than this many.
pub const REPACK_OBJECTS: u64 = 2_000;
/// Or when they take more than this many bytes.
pub const REPACK_BYTES: u64 = 64 * 1024 * 1024;
/// One run packs the loose objects that take this many bytes before compression, at most
/// (one object that is larger runs alone). The rest waits for the next run.
pub const MAX_RAW: u64 = 96 * 1024 * 1024;
/// The unreachable objects removed while holding off a commit's start: it waits for one batch
/// at most (§9.8).
const PRUNE_BATCH: usize = 128;

/// What a repack did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Repacked {
    /// Objects written into the new pack.
    pub packed: u64,
    /// Loose files removed because a pack holds the object.
    pub removed: u64,
    /// Unreachable loose files removed for their age.
    pub pruned: u64,
    /// The new pack's name (`pack-<hash>`), when one was written.
    pub pack: Option<String>,
}

/// What the writer has written to the object database, by the store's counters.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Written {
    objects: u64,
    bytes: u64,
}

fn written(counters: &StoreCounters) -> Written {
    Written {
        objects: counters.git_blobs_written
            + counters.git_trees_written
            + counters.git_commits_written,
        bytes: counters.git_blob_bytes,
    }
}

/// How many loose objects the repository held when they were counted, and what the writer
/// had written by then. What it writes after is added, so the folders are listed once, not
/// at every checkpoint (§9.8).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LooseCount {
    objects: u64,
    bytes: u64,
    at: Written,
}

/// A repack that runs on a thread of its own.
pub(crate) struct Run {
    done: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Writer {
    /// §9.8: after a checkpoint moved the branch, and once after the open, the repository is
    /// repacked when it holds more than 2,000 loose objects or 64 MiB of them. The repack runs
    /// on a thread of its own (the writer goes on with its jobs), never starts while the
    /// project closes or while `refs/tc/carried` lacks a version the fold names, and loses
    /// nothing if it fails: the next check tries again. Never call it during a sync.
    pub(crate) fn maybe_repack(&mut self) {
        if let Some(run) = self.repack.take() {
            if !run.done.load(Ordering::Acquire) {
                self.repack = Some(run);
                return;
            }
            let _ = run.thread.join();
            // The run changed the folders: they are counted again.
            self.loose = None;
        }
        if self.read_only().is_some() || self.shared.is_closing() {
            return;
        }
        let Some(git) = &self.git else {
            return;
        };
        let now = written(&self.counters.read());
        let after = self.shared.hooks.repack_threshold();
        let (objects, bytes) = match self.loose {
            Some(l) => (
                l.objects + now.objects.saturating_sub(l.at.objects),
                l.bytes + now.bytes.saturating_sub(l.at.bytes),
            ),
            None => git.loose_objects(),
        };
        let mut counted = (objects, bytes);
        // The estimate counts objects the database already had; the folders are the truth.
        if due(objects, bytes, after) {
            counted = git.loose_objects();
            // A version recorded since the last checkpoint (in this session, or in one that
            // ended before its first) is reachable only once `refs/tc/carried` names it.
            let versions = self.shared.snapshot().fold().carried_versions();
            if due(counted.0, counted.1, after) && self.keep_carried_versions(self.device, versions)
            {
                self.start_repack();
            }
        }
        self.loose = Some(LooseCount {
            objects: counted.0,
            bytes: counted.1,
            at: now,
        });
    }

    fn start_repack(&mut self) {
        let Some(Ok(git)) = self.git.as_ref().map(Git::another) else {
            return;
        };
        let done = Arc::new(AtomicBool::new(false));
        let (flag, shared, hooks) = (done.clone(), self.shared.clone(), self.shared.hooks.clone());
        shared.job_started();
        let spawned = std::thread::Builder::new()
            .name("tc-store repack".into())
            .spawn(move || {
                // A failed or stopped run loses nothing and is tried again at the next check.
                let _ = git.repack(SystemTime::now(), &hooks, &shared.stop_flag(), MAX_RAW);
                flag.store(true, Ordering::Release);
                shared.job_done();
            });
        match spawned {
            Ok(thread) => self.repack = Some(Run { done, thread }),
            Err(_) => self.shared.job_done(),
        }
    }

    /// The project closes: a repack that runs is stopped (its progress callback sees the
    /// flag) and waited for, which takes the time of one step of libgit2's, never the time of
    /// the repack.
    pub(crate) fn stop_repack(&mut self) {
        self.shared.begin_closing();
        if let Some(run) = self.repack.take() {
            let _ = run.thread.join();
        }
    }
}

/// A loose object file.
#[derive(Debug, Clone)]
pub(crate) struct Loose {
    pub oid: Oid,
    pub path: PathBuf,
    pub modified: SystemTime,
}

/// What to do with each loose object (pure: [`classify`]).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Reachable and in no pack: go into the new pack.
    pub to_pack: HashSet<Oid>,
    /// Reachable and already in a pack: only the loose copy goes.
    pub duplicates: Vec<Oid>,
    /// Unreachable and old.
    pub prune: Vec<Oid>,
}

/// Which loose objects to pack, drop and prune. `reachable` holds what the refs reach and
/// `packed` the loose objects a pack already has.
pub(crate) fn classify(
    loose: &[Loose],
    reachable: &HashSet<Oid>,
    packed: &HashSet<Oid>,
    now: SystemTime,
) -> Plan {
    let mut plan = Plan::default();
    for l in loose {
        if reachable.contains(&l.oid) {
            if packed.contains(&l.oid) {
                plan.duplicates.push(l.oid);
            } else {
                plan.to_pack.insert(l.oid);
            }
        } else if now
            .duration_since(l.modified)
            .is_ok_and(|age| age > PRUNE_AFTER)
        {
            plan.prune.push(l.oid);
        }
    }
    plan
}

/// The object ids a version 2 pack index lists. `None` for anything else.
pub(crate) fn index_oids(bytes: &[u8]) -> Option<impl Iterator<Item = [u8; 20]> + '_> {
    const HEADER: [u8; 8] = [0xff, b't', b'O', b'c', 0, 0, 0, 2];
    const FANOUT: usize = 256 * 4;
    if bytes.len() < 8 + FANOUT || bytes[..8] != HEADER {
        return None;
    }
    let last = &bytes[8 + FANOUT - 4..8 + FANOUT];
    let count = u32::from_be_bytes([last[0], last[1], last[2], last[3]]) as usize;
    let names = bytes.get(8 + FANOUT..8 + FANOUT + count.checked_mul(20)?)?;
    Some(names.as_chunks::<20>().0.iter().copied())
}

/// What the refs reach, newest first: commits, then trees, then blobs (the order a pack
/// compresses best in).
struct Reach {
    ordered: Vec<Oid>,
    set: HashSet<Oid>,
    /// Where a wanted object was first met, for the packbuilder's delta search.
    names: HashMap<Oid, String>,
}

impl Git {
    /// The repack (§9.8). `now` is the time the 14 days are counted to; the files' own times
    /// are the operating system's. `stop` is raised when the project closes: the run ends at
    /// the next step, and removes nothing it has not packed. One run packs at most `max_raw`
    /// bytes of loose objects (before compression).
    pub fn repack(
        &self,
        now: SystemTime,
        hooks: &Hooks,
        stop: &Arc<AtomicBool>,
        max_raw: u64,
    ) -> Result<Repacked, GitError> {
        let repo = self.repo();
        let objects = repo.path().join("objects");
        let marks = self.commit_marks();
        let loose = list_loose(&objects)?;
        let mut done = Repacked::default();
        if loose.is_empty() {
            return Ok(done);
        }
        let wanted: HashSet<Oid> = loose.iter().map(|l| l.oid).collect();
        // A walk that fails removes nothing: an object is pruned only when every ref was
        // read and none reached it.
        let reach = match reachable(repo, &wanted, stop) {
            Ok(reach) => reach,
            // A stopped walk is no failure: nothing was packed or removed.
            Err(_) if stop.load(Ordering::Acquire) => return Ok(done),
            Err(e) => return Err(e),
        };
        let reached: HashSet<Oid> = reach.set.intersection(&wanted).copied().collect();
        let packed = packed_among(&objects.join("pack"), &reached)?;
        let plan = classify(&loose, &reach.set, &packed, now);
        if stop.load(Ordering::Acquire) {
            return Ok(done);
        }

        let chosen = self.within(&reach, &plan.to_pack, max_raw)?;
        if !chosen.is_empty() {
            let Some(name) = self.write_pack(&reach, &chosen, &objects.join("pack"), stop)? else {
                return Ok(done);
            };
            done.pack = Some(name);
            done.packed = chosen.len() as u64;
            hooks.wait(Point::AfterPackWritten);
            hooks
                .check(Point::AfterPackWritten)
                .map_err(|_| GitError::Other("stopped after the pack was written".into()))?;
            if stop.load(Ordering::Acquire) {
                // The objects are in the pack and still loose: the next repack removes the
                // loose copies (§6.4 R1).
                return Ok(done);
            }
        }

        let by_oid: HashMap<Oid, &Loose> = loose.iter().map(|l| (l.oid, l)).collect();
        let packed_now = chosen.iter().chain(plan.duplicates.iter());
        for oid in packed_now {
            if remove_loose(&by_oid[oid].path) {
                done.removed += 1;
            }
        }
        // The walk found nothing reaching these; a commit made since may name them. Nothing
        // unreachable goes while one is being made or after one was begun during the run (the
        // next run has them again), and a commit cannot begin while a batch is removed: it
        // waits for the batch, and the batches after it find it.
        for batch in plan.prune.chunks(PRUNE_BATCH) {
            hooks.prune_batch_reached(done.pruned);
            if stop.load(Ordering::Acquire) {
                break;
            }
            let removed = self.while_no_commit(marks, || {
                batch
                    .iter()
                    .filter(|oid| prune_if_unchanged(by_oid[*oid]))
                    .count()
            });
            let Some(removed) = removed else {
                break;
            };
            done.pruned += removed as u64;
        }
        remove_empty_folders(&objects);

        let counters = self.counters();
        counters.add(&counters.git_repacks, 1);
        counters.add(&counters.git_objects_packed, done.packed);
        counters.add(&counters.git_loose_removed, done.removed + done.pruned);
        Ok(done)
    }

    /// Of `to_pack`, the first objects (in the order of a good pack: commits, trees, blobs)
    /// whose sizes add up to `max_raw`; at least one.
    fn within(
        &self,
        reach: &Reach,
        to_pack: &HashSet<Oid>,
        max_raw: u64,
    ) -> Result<HashSet<Oid>, GitError> {
        let odb = at("open its objects", self.repo().odb())?;
        let mut chosen = HashSet::new();
        let mut total = 0u64;
        for oid in reach.ordered.iter().filter(|oid| to_pack.contains(oid)) {
            let size = odb.read_header(*oid).map_or(0, |(size, _)| size as u64);
            if !chosen.is_empty() && total + size > max_raw {
                break;
            }
            total += size;
            chosen.insert(*oid);
        }
        Ok(chosen)
    }

    /// Writes `to_pack` as one pack in `pack_dir` and checks its index names them all. `None`
    /// when `stop` was raised first (libgit2 asks twice a second while it looks for deltas).
    fn write_pack(
        &self,
        reach: &Reach,
        to_pack: &HashSet<Oid>,
        pack_dir: &Path,
        stop: &Arc<AtomicBool>,
    ) -> Result<Option<String>, GitError> {
        let repo = self.repo();
        let mut builder = at("start a pack", repo.packbuilder())?;
        // Two threads look for deltas: the run is background work beside a person typing.
        builder.set_threads(2);
        let flag = stop.clone();
        at(
            "watch a pack",
            builder.set_progress_callback(move |_, _, _| !flag.load(Ordering::Acquire)),
        )?;
        for oid in reach.ordered.iter().filter(|oid| to_pack.contains(oid)) {
            let name = reach.names.get(oid).map(String::as_str);
            at("add to a pack", builder.insert_object(*oid, name))?;
        }
        if stop.load(Ordering::Acquire) {
            return Ok(None);
        }
        if let Err(e) = builder.write(pack_dir, 0o644) {
            // A stopped run is no failure; the half-written pack is removed by the next open.
            if stop.load(Ordering::Acquire) {
                return Ok(None);
            }
            return Err(GitError::Git {
                what: "write a pack",
                source: e,
            });
        }
        let name = match at("name a pack", builder.name())? {
            Some(hash) => format!("pack-{hash}"),
            None => return Err(GitError::Other("the pack has no name".into())),
        };
        // The pack is trusted only as far as its index says it holds every object.
        let index = std::fs::read(pack_dir.join(format!("{name}.idx")))
            .map_err(|e| GitError::Other(format!("the pack's index cannot be read: {e}")))?;
        let Some(listed) = index_oids(&index) else {
            return Err(GitError::Other("the pack's index is not readable".into()));
        };
        let listed: HashSet<[u8; 20]> = listed.collect();
        for oid in to_pack {
            let mut bytes = [0u8; 20];
            bytes.copy_from_slice(oid.as_bytes());
            if !listed.contains(&bytes) {
                return Err(GitError::Other(format!(
                    "the pack lacks {oid}; no loose object was removed"
                )));
            }
        }
        // libgit2 lists its packs once; the new one is made known to it.
        at(
            "refresh its objects",
            repo.odb().and_then(|odb| odb.refresh()),
        )?;
        Ok(Some(name))
    }
}

/// Every loose object file under `objects/`.
fn list_loose(objects: &Path) -> Result<Vec<Loose>, GitError> {
    let mut out = Vec::new();
    let folders = match std::fs::read_dir(objects) {
        Ok(folders) => folders,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(GitError::Other(format!(
                "the objects cannot be listed: {e}"
            )));
        }
    };
    for folder in folders.flatten() {
        let prefix = folder.file_name().to_string_lossy().into_owned();
        if prefix.len() != 2 || !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(folder.path()) else {
            continue;
        };
        for file in files.flatten() {
            let rest = file.file_name().to_string_lossy().into_owned();
            let Ok(oid) = Oid::from_str(&format!("{prefix}{rest}")) else {
                continue;
            };
            let modified = file
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::now());
            out.push(Loose {
                oid,
                path: file.path(),
                modified,
            });
        }
    }
    Ok(out)
}

/// Everything the branches, remotes and `refs/tc/` reach. A reference that cannot be read,
/// or a commit or tree that is missing, fails the walk.
fn reachable(
    repo: &Repository,
    wanted: &HashSet<Oid>,
    stop: &AtomicBool,
) -> Result<Reach, GitError> {
    let mut walk = at("walk the history", repo.revwalk())?;
    at("walk the history", walk.set_sorting(Sort::TIME))?;
    let refs = at("list references", repo.references())?;
    for reference in refs {
        let reference = at("read a reference", reference)?;
        let name = reference.name_bytes();
        if [&b"refs/heads/"[..], b"refs/remotes/", b"refs/tc/"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            let commit = at("read a reference", reference.peel_to_commit())?;
            at("walk the history", walk.push(commit.id()))?;
        }
    }
    // A detached HEAD keeps its history too; an unborn one has none.
    if let Ok(head) = repo.head()
        && let Ok(commit) = head.peel_to_commit()
    {
        at("walk the history", walk.push(commit.id()))?;
    }

    let mut commits = Vec::new();
    let mut trees = Vec::new();
    let mut blobs = Vec::new();
    let mut set: HashSet<Oid> = HashSet::new();
    let mut names: HashMap<Oid, String> = HashMap::new();
    let mut stack: Vec<(Oid, String)> = Vec::new();
    for oid in walk {
        if stop.load(Ordering::Acquire) {
            return Err(GitError::Other("stopped while walking the history".into()));
        }
        let oid = at("walk the history", oid)?;
        set.insert(oid);
        commits.push(oid);
        let commit = at("read a commit", repo.find_commit(oid))?;
        stack.push((commit.tree_id(), String::new()));
        while let Some((tree_id, path)) = stack.pop() {
            if !set.insert(tree_id) {
                continue;
            }
            trees.push(tree_id);
            if wanted.contains(&tree_id) {
                names.insert(tree_id, path.clone());
            }
            let tree = match repo.find_tree(tree_id) {
                Ok(tree) => tree,
                Err(e) if e.code() == ErrorCode::NotFound => {
                    return Err(GitError::Other(format!("the tree {tree_id} is missing")));
                }
                Err(source) => {
                    return Err(GitError::Git {
                        what: "read a tree",
                        source,
                    });
                }
            };
            for entry in tree.iter() {
                let entry_path = format!("{path}{}", String::from_utf8_lossy(entry.name_bytes()));
                match entry.kind() {
                    Some(ObjectType::Tree) => stack.push((entry.id(), format!("{entry_path}/"))),
                    Some(ObjectType::Blob) if set.insert(entry.id()) => {
                        blobs.push(entry.id());
                        if wanted.contains(&entry.id()) {
                            names.insert(entry.id(), entry_path);
                        }
                    }
                    // A submodule's commit lives in another repository.
                    _ => {}
                }
            }
        }
    }
    let mut ordered = commits;
    ordered.extend(trees);
    ordered.extend(blobs);
    Ok(Reach {
        ordered,
        set,
        names,
    })
}

/// Of `candidates`, the objects some pack index of `pack_dir` lists.
fn packed_among(pack_dir: &Path, candidates: &HashSet<Oid>) -> Result<HashSet<Oid>, GitError> {
    let mut found = HashSet::new();
    if candidates.is_empty() {
        return Ok(found);
    }
    let by_bytes: HashMap<[u8; 20], Oid> = candidates
        .iter()
        .filter_map(|oid| <[u8; 20]>::try_from(oid.as_bytes()).ok().map(|b| (b, *oid)))
        .collect();
    let files = match std::fs::read_dir(pack_dir) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(GitError::Other(format!("the packs cannot be listed: {e}"))),
    };
    for file in files.flatten() {
        let name = file.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("pack-") && name.ends_with(".idx")) {
            continue;
        }
        // An index this code cannot read proves nothing: its objects are written again.
        let Ok(bytes) = std::fs::read(file.path()) else {
            continue;
        };
        let Some(listed) = index_oids(&bytes) else {
            continue;
        };
        for oid in listed {
            if let Some(oid) = by_bytes.get(&oid) {
                found.insert(*oid);
            }
        }
    }
    Ok(found)
}

/// Removes an unreachable loose object unless its file was written again since the repack
/// listed it (something freshened it, as git does for an object it is given again).
fn prune_if_unchanged(listed: &Loose) -> bool {
    let unchanged = std::fs::metadata(&listed.path)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified == listed.modified);
    unchanged && remove_loose(&listed.path)
}

/// Removes a loose object file. `false` when it could not be removed now; a later repack
/// tries again.
fn remove_loose(path: &Path) -> bool {
    if std::fs::remove_file(path).is_ok() {
        return true;
    }
    retry_writable(path)
}

/// libgit2 writes loose objects read-only, which Windows refuses to remove.
#[cfg(windows)]
fn retry_writable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let mut permissions = meta.permissions();
    permissions.set_readonly(false);
    let _ = std::fs::set_permissions(path, permissions);
    std::fs::remove_file(path).is_ok()
}

#[cfg(not(windows))]
fn retry_writable(_path: &Path) -> bool {
    false
}

/// Removes the two-letter folders a repack emptied (`rmdir` refuses a folder with files).
fn remove_empty_folders(objects: &Path) {
    let Ok(folders) = std::fs::read_dir(objects) else {
        return;
    };
    for folder in folders.flatten() {
        let name = folder.file_name().to_string_lossy().into_owned();
        if name.len() == 2 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            let _ = std::fs::remove_dir(folder.path());
        }
    }
}

/// Whether the loose objects are enough to repack.
pub(crate) fn due(objects: u64, bytes: u64, after: (u64, u64)) -> bool {
    objects > after.0 || bytes > after.1
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn loose(n: u8, age_days: u64, now: SystemTime) -> Loose {
        Loose {
            oid: Oid::from_bytes(&[n; 20]).unwrap(),
            path: PathBuf::from(format!("x/{n}")),
            modified: now - Duration::from_secs(age_days * 24 * 3600),
        }
    }

    #[test]
    fn a_reachable_object_is_packed_and_an_unreachable_one_waits_for_its_age() {
        let now = SystemTime::now();
        let files = [loose(1, 0, now), loose(2, 20, now), loose(3, 3, now)];
        let reachable = HashSet::from([files[0].oid]);
        let plan = classify(&files, &reachable, &HashSet::new(), now);
        assert_eq!(plan.to_pack, HashSet::from([files[0].oid]));
        assert_eq!(plan.prune, vec![files[1].oid]);
        assert!(plan.duplicates.is_empty());
    }

    #[test]
    fn a_reachable_object_a_pack_already_holds_is_not_packed_again() {
        let now = SystemTime::now();
        let files = [loose(1, 0, now)];
        let reachable = HashSet::from([files[0].oid]);
        let packed = HashSet::from([files[0].oid]);
        let plan = classify(&files, &reachable, &packed, now);
        assert!(plan.to_pack.is_empty());
        assert_eq!(plan.duplicates, vec![files[0].oid]);
    }

    #[test]
    fn a_stopped_repack_packs_and_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::init(dir.path(), Arc::default()).unwrap();
        let blob = git.write_blob(b"loose, reachable").unwrap();
        let tree = git
            .update_tree(None, &[("f".into(), blob, crate::git::MODE_BLOB)], &[])
            .unwrap();
        let author = crate::git::Author {
            name: "a".into(),
            time_ms: 1_000,
        };
        let commit = git.write_commit(tree, &[], &author, "m").unwrap();
        git.move_branch(None, commit).unwrap();
        let stop = Arc::new(AtomicBool::new(true));
        let done = git
            .repack(SystemTime::now(), &Hooks::default(), &stop, MAX_RAW)
            .unwrap();
        assert_eq!(done, Repacked::default());
        assert!(git.has(blob));
        assert!(
            !list_loose(&git.git_dir().join("objects"))
                .unwrap()
                .is_empty()
        );
        // The same repository repacks when nothing stops it, and keeps the object.
        stop.store(false, Ordering::Release);
        let done = git
            .repack(SystemTime::now(), &Hooks::default(), &stop, MAX_RAW)
            .unwrap();
        assert!(done.packed >= 3 && done.removed >= 3, "{done:?}");
        assert!(git.has(blob));
        // What a branch reaches is no longer loose (the empty tree `update_tree` starts from
        // is unreachable and young, so it stays).
        let left = list_loose(&git.git_dir().join("objects")).unwrap();
        for oid in [
            to_oid(commit).unwrap(),
            to_oid(tree).unwrap(),
            to_oid(blob).unwrap(),
        ] {
            assert!(left.iter().all(|l| l.oid != oid), "{oid} is still loose");
        }
    }

    /// One run packs the objects that fit in `max_raw`, in the order of a good pack, and the
    /// next run takes the rest.
    #[test]
    fn a_run_packs_no_more_than_its_share() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::init(dir.path(), Arc::default()).unwrap();
        let mut entries = Vec::new();
        for n in 0..6u8 {
            let bytes = vec![n; 1_000];
            let blob = git.write_blob(&bytes).unwrap();
            entries.push((format!("f{n}"), blob, crate::git::MODE_BLOB));
        }
        let tree = git.update_tree(None, &entries, &[]).unwrap();
        let author = crate::git::Author {
            name: "a".into(),
            time_ms: 1_000,
        };
        let commit = git.write_commit(tree, &[], &author, "m").unwrap();
        git.move_branch(None, commit).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let first = git
            .repack(SystemTime::now(), &Hooks::default(), &stop, 2_500)
            .unwrap();
        // The commit and the tree come first, then blobs of 1,000 bytes: 2,500 bytes hold the
        // commit, the tree and two blobs.
        assert!(
            first.packed >= 2 && first.packed < 8,
            "a share of the 8 objects: {first:?}"
        );
        let mut runs = 1;
        loop {
            let more = git
                .repack(SystemTime::now(), &Hooks::default(), &stop, 2_500)
                .unwrap();
            runs += 1;
            if more.packed == 0 {
                break;
            }
            assert!(runs < 10, "the runs end");
        }
        let left = list_loose(&git.git_dir().join("objects")).unwrap();
        for (_, blob, _) in &entries {
            assert!(git.has(*blob));
            assert!(left.iter().all(|l| l.oid != to_oid(*blob).unwrap()));
        }
    }

    #[test]
    fn a_pack_index_of_another_version_lists_nothing() {
        assert!(index_oids(b"short").is_none());
        let mut bytes = vec![0xff, b't', b'O', b'c', 0, 0, 0, 3];
        bytes.extend(vec![0u8; 1024]);
        assert!(index_oids(&bytes).is_none());
    }

    #[test]
    fn the_thresholds_are_two_thousand_objects_or_sixty_four_mebibytes() {
        let after = (REPACK_OBJECTS, REPACK_BYTES);
        assert!(!due(2_000, 1, after));
        assert!(due(2_001, 1, after));
        assert!(!due(1, 64 * 1024 * 1024, after));
        assert!(due(1, 64 * 1024 * 1024 + 1, after));
    }
}
