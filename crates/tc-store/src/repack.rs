//! Repack (`docs/STORAGE.md` §9.8). libgit2 has no `gc`, so a project that commits for years
//! would keep every object as a loose file. When the loose objects pass 2,000 or 64 MiB, the
//! ones the branches, remotes and `refs/tc/` reach are written into one new pack, and the
//! loose files that a pack now holds are removed. Objects nothing reaches are removed only
//! when they are older than 14 days.
//!
//! Only the loose objects are packed, not the whole history again: a repack that wrote
//! every reachable object each time would add the history's size to the disk at each run.
//! Nothing is removed before the pack and its index are on disk (libgit2 flushes both and
//! the folder when `core.fsyncObjectFiles` is on, `pack-objects.c` `git_packbuilder_write`),
//! and the index is read back to confirm that it names every object about to lose its loose
//! file. A repack that stops anywhere in between leaves objects twice, never fewer (§6.4 R1);
//! the next one finds them in a pack and removes the loose copies.
//!
//! Callers must not repack during a sync (§9.8).

use crate::counters::StoreCounters;
use crate::git::{Git, GitError, at};
use crate::hooks::{Hooks, Point};
use crate::writer::Writer;
use git2::{ErrorCode, ObjectType, Oid, Repository, Sort};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A loose object nothing reaches is removed once it is this old.
pub const PRUNE_AFTER: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// A repack is due when the loose objects are more than this many.
pub const REPACK_OBJECTS: u64 = 2_000;
/// Or when they take more than this many bytes.
pub const REPACK_BYTES: u64 = 64 * 1024 * 1024;

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

impl Writer {
    /// §9.8: after a checkpoint moved the branch, and once after the open, the repository is
    /// repacked when it holds more than 2,000 loose objects or 64 MiB of them. A repack that
    /// fails loses nothing and is tried again at the next checkpoint. Never call it during
    /// a sync.
    pub(crate) fn maybe_repack(&mut self) {
        if self.read_only().is_some() {
            return;
        }
        let Some(git) = self.git.take() else {
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
            if due(counted.0, counted.1, after) {
                if git.repack(SystemTime::now(), &self.shared.hooks).is_ok() {
                    counted = git.loose_objects();
                } else {
                    // Retried at the next checkpoint.
                    self.loose = None;
                    self.git = Some(git);
                    return;
                }
            }
        }
        self.loose = Some(LooseCount {
            objects: counted.0,
            bytes: counted.1,
            at: now,
        });
        self.git = Some(git);
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
    /// are the operating system's.
    pub fn repack(&self, now: SystemTime, hooks: &Hooks) -> Result<Repacked, GitError> {
        let repo = self.repo();
        let objects = repo.path().join("objects");
        let loose = list_loose(&objects)?;
        let mut done = Repacked::default();
        if loose.is_empty() {
            return Ok(done);
        }
        let wanted: HashSet<Oid> = loose.iter().map(|l| l.oid).collect();
        // A walk that fails removes nothing: an object is pruned only when every ref was
        // read and none reached it.
        let reach = reachable(repo, &wanted)?;
        let reached: HashSet<Oid> = reach.set.intersection(&wanted).copied().collect();
        let packed = packed_among(&objects.join("pack"), &reached)?;
        let plan = classify(&loose, &reach.set, &packed, now);

        if !plan.to_pack.is_empty() {
            done.pack = Some(self.write_pack(&reach, &plan.to_pack, &objects.join("pack"))?);
            done.packed = plan.to_pack.len() as u64;
            hooks.wait(Point::AfterPackWritten);
            hooks
                .check(Point::AfterPackWritten)
                .map_err(|_| GitError::Other("stopped after the pack was written".into()))?;
        }

        let by_oid: HashMap<Oid, &Loose> = loose.iter().map(|l| (l.oid, l)).collect();
        let packed_now = plan.to_pack.iter().chain(plan.duplicates.iter());
        for oid in packed_now {
            if remove_loose(&by_oid[oid].path) {
                done.removed += 1;
            }
        }
        for oid in &plan.prune {
            if remove_loose(&by_oid[oid].path) {
                done.pruned += 1;
            }
        }
        remove_empty_folders(&objects);

        let counters = self.counters();
        counters.add(&counters.git_repacks, 1);
        counters.add(&counters.git_objects_packed, done.packed);
        counters.add(&counters.git_loose_removed, done.removed + done.pruned);
        Ok(done)
    }

    /// Writes `to_pack` as one pack in `pack_dir` and checks its index names them all.
    fn write_pack(
        &self,
        reach: &Reach,
        to_pack: &HashSet<Oid>,
        pack_dir: &Path,
    ) -> Result<String, GitError> {
        let repo = self.repo();
        let mut builder = at("start a pack", repo.packbuilder())?;
        for oid in reach.ordered.iter().filter(|oid| to_pack.contains(oid)) {
            let name = reach.names.get(oid).map(String::as_str);
            at("add to a pack", builder.insert_object(*oid, name))?;
        }
        at("write a pack", builder.write(pack_dir, 0o644))?;
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
        Ok(name)
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
fn reachable(repo: &Repository, wanted: &HashSet<Oid>) -> Result<Reach, GitError> {
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
