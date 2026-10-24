//! The writer's half of a checkpoint (`docs/STORAGE.md` §5.13 C3 to C5, §9.4; APP-DESIGN R21).
//!
//! The actor sealed the pending files and handed the writer a snapshot; the writer has just
//! materialized it. Now it builds the commit tree in memory, from the manifest's blobs for
//! every file the last materialization made equal to the snapshot's projection and from the
//! snapshot for the rest, checks it as a burrito, and writes the commit object. The branch is
//! moved by the actor, the one step of a checkpoint that is not the writer's.
//!
//! Only the directories on the paths that changed are written again: the tree is the head's
//! with the changed entries set, and a file that is the same as in the head is never read.

use crate::error::StoreError;
use crate::git::{
    Author, CARRIED_REF, Git, Head, MODE_BLOB, TRAILER_DEVICE, TRAILER_FORMAT, TRAILER_FRONTIER,
    TreeEntry,
};
use crate::handle::{CheckpointReport, CheckpointSkip, Msg};
use crate::layout::{
    FORMAT_JSON, GITATTRIBUTES, GITATTRIBUTES_BYTES, GITIGNORE, GITIGNORE_BYTES, METADATA,
    PROJECT_JSON,
};
use crate::materialize::{Disk, book_ranges, is_text_path};
use crate::writer::{CheckpointJob, Writer};
use std::collections::{BTreeMap, BTreeSet};
use tc_burrito::{FileFacts, FileState, IngredientEntry, IngredientPath, ProjectFiles};
use tc_journal::fold::{Blocked, DerivedFile, Notice};
use tc_journal::{
    BookId, DeviceId, FORMAT_MAJOR, FORMAT_MINOR, GitOid, Md5, PackName, Record, RecordKey,
    RepoPath,
};
use unicode_normalization::is_nfc;

/// One entry of the tree being built.
#[derive(Debug, Clone)]
struct Desired {
    oid: GitOid,
    mode: u32,
    md5: Md5,
    size: u64,
    /// For a text file: whether its bytes are in NFC (`metadata.json` says so, §3.4).
    nfc: Option<bool>,
}

/// What a commit's message says it is for.
pub(crate) enum Subject {
    /// A checkpoint (§5.13).
    Checkpoint(crate::handle::CheckpointReason),
    /// A new repository's first commit after the damaged one was set aside (§6.4 G4).
    Recovered,
}

/// What the checkpoint's last stage decided.
enum Outcome {
    Skip(CheckpointSkip),
    /// A commit object is written; the actor moves the branch to it.
    Commit {
        commit: GitOid,
        expected: Option<GitOid>,
        tree: GitOid,
    },
}

/// A tree built, or the problem that stops it.
struct Built {
    tree: GitOid,
    /// It is HEAD's tree: no commit (C4).
    same: bool,
    books: BTreeSet<BookId>,
    problem: Option<String>,
}

/// The files of the tree being built, as the burrito validator sees them.
struct TreeFiles<'a>(&'a BTreeMap<String, Desired>);

impl ProjectFiles for TreeFiles<'_> {
    fn inspect(&self, path: &IngredientPath) -> FileState {
        match self.0.get(path.as_str()) {
            Some(d) => match tc_burrito::Md5::parse(&d.md5.to_hex()) {
                Some(md5) => FileState::Present(FileFacts { md5, size: d.size }),
                None => FileState::Unreadable("an md5 that does not parse".into()),
            },
            None => FileState::Missing,
        }
    }

    fn ingredient_dir_files(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .keys()
            .filter(|path| {
                path.strip_prefix("ingredients/").is_some_and(|rest| {
                    !rest
                        .split('/')
                        .any(|part| part.starts_with('.') || part.ends_with(".bak"))
                })
            })
            .cloned()
            .collect())
    }
}

impl Writer {
    /// Ends a checkpoint job: answers through the actor, either with the commit to publish or
    /// with why there is none. The seal's error comes before the files', and theirs before
    /// the commit's (a failed seal leaves pending files the packs lack, so nothing is
    /// committed).
    pub(crate) fn finish_checkpoint(
        &mut self,
        job: CheckpointJob,
        materialized: Result<(), StoreError>,
    ) {
        let CheckpointJob {
            reason,
            intake,
            sealed,
            device,
            reply,
        } = job;
        let versions = self.fold.carried_versions();
        self.keep_carried_versions(device, versions);
        let outcome = sealed.and_then(|sealed| {
            materialized?;
            let mut report = CheckpointReport {
                sealed: sealed.packs,
                intake,
                ..CheckpointReport::default()
            };
            if !sealed.unsealed.is_empty() {
                // The snapshot shows changes of a file the seal could not take, and no pack
                // holds them (C3).
                report.skipped = Some(CheckpointSkip::SealFailed);
                return Ok((report, None));
            }
            match self.commit_stage(&Subject::Checkpoint(reason), device)? {
                Outcome::Skip(why) => {
                    report.skipped = Some(why);
                    Ok((report, None))
                }
                Outcome::Commit {
                    commit,
                    expected,
                    tree,
                } => Ok((report, Some((commit, expected, tree)))),
            }
        });
        let message = match outcome {
            Err(result) => Msg::CheckpointEnded {
                result: Err(result),
                reply,
            },
            Ok((report, None)) => Msg::CheckpointEnded {
                result: Ok(report),
                reply,
            },
            Ok((report, Some((commit, expected, tree)))) => Msg::MoveHead {
                commit,
                expected,
                tree,
                report,
                reply,
            },
        };
        let _ = self.actor.send(message);
    }

    /// §9.8, §11.4: `refs/tc/carried` is a commit with no parent whose tree names, by blob id,
    /// every version of every carried file the history holds and the object database has.
    /// Superseded versions and ones no checkpoint committed are reachable through it, so a
    /// repack never prunes a version a person may restore (L73). The set only grows. It is
    /// written at every checkpoint and before every repack, so a version recorded in a session
    /// that ended before its first checkpoint is kept by the next session's first repack.
    /// `versions`: the versions the fold names. `true` when the reference keeps every one of
    /// them the object database has; a reference that cannot be written is tried again later,
    /// and no repack runs until it is (the bytes are in the object database either way).
    pub(crate) fn keep_carried_versions(
        &mut self,
        device: DeviceId,
        versions: Vec<GitOid>,
    ) -> bool {
        if self.read_only().is_some() {
            return false;
        }
        let Some(git) = self.git.take() else {
            return false;
        };
        git.commit_begin();
        let kept = self.keep_carried_versions_with(&git, device, versions);
        git.commit_end();
        self.git = Some(git);
        kept.is_ok()
    }

    fn keep_carried_versions_with(
        &mut self,
        git: &Git,
        device: DeviceId,
        versions: Vec<GitOid>,
    ) -> Result<(), StoreError> {
        let mut wanted: BTreeSet<GitOid> =
            versions.into_iter().filter(|oid| git.has(*oid)).collect();
        if self.carried_kept.is_none() {
            let kept: BTreeSet<GitOid> = match git.find_ref(CARRIED_REF)? {
                Some(commit) => git
                    .tree_entries(git.tree_of(commit)?)?
                    .keys()
                    .filter_map(|name| GitOid::parse(name))
                    .collect(),
                None => BTreeSet::new(),
            };
            self.carried_kept = Some(kept);
        }
        let kept = self.carried_kept.clone().unwrap_or_default();
        if wanted.is_subset(&kept) {
            return Ok(());
        }
        wanted.extend(kept);
        let entries: Vec<(String, GitOid, u32)> = wanted
            .iter()
            .map(|oid| (oid.to_hex(), *oid, MODE_BLOB))
            .collect();
        let tree = git.update_tree(None, &entries, &[])?;
        let author = Author {
            name: self.author_name(device),
            time_ms: self.clock.now().0,
        };
        let commit = git.write_commit(tree, &[], &author, "Carried files (tC)")?;
        git.set_ref(CARRIED_REF, commit)?;
        self.carried_kept = Some(wanted);
        Ok(())
    }

    /// The first commit of a repository that replaced a damaged one (§6.4 G4): the tree the
    /// project makes now, with no parent. The actor moves the branch as for a checkpoint.
    pub(crate) fn recover_history(&mut self) {
        let Some(broken) = self.recover.take() else {
            return;
        };
        self.adopt_old_objects(&broken);
        let device = self.device;
        let (_unused, reply) = crate::handle::pending(|| {
            Err(StoreError::Other(
                "no one waits for the recovery commit".into(),
            ))
        });
        let message = match self.commit_stage(&Subject::Recovered, device) {
            Ok(Outcome::Commit {
                commit,
                expected,
                tree,
            }) => Msg::MoveHead {
                commit,
                expected,
                tree,
                report: CheckpointReport::default(),
                reply,
            },
            Ok(Outcome::Skip(_)) | Err(_) => return,
        };
        let _ = self.actor.send(message);
    }

    /// §6.4 G4: the blobs the old repository held that the project still needs go into the new
    /// one: every version of every carried file the history names and every pack's blob, copied
    /// from the old repository's objects; then the carried files in the working tree whose
    /// bytes are a recorded version are stored as blobs. A blob that is nowhere is left out of
    /// the commit with `CarriedBytesMissing`, never a reason to refuse it (§9.4).
    fn adopt_old_objects(&mut self, broken: &std::path::Path) {
        let Some(git) = self.git.take() else {
            return;
        };
        let mut wanted: BTreeSet<GitOid> = self.fold.carried_versions().into_iter().collect();
        wanted.extend(
            self.manifest
                .packs
                .values()
                .filter_map(|p| p.blob.as_deref().and_then(GitOid::parse)),
        );
        wanted.extend(self.packs.values().filter_map(|p| p.blob));
        let wanted: Vec<GitOid> = wanted.into_iter().collect();
        // A damaged object database may have lost any of them: what is missing is looked for
        // in the working tree next.
        let _ = git.adopt_blobs(&broken.join("objects"), &wanted);
        for placement in self.fold.carried_files() {
            self.store_placed_bytes(&git, &placement);
        }
        self.git = Some(git);
    }

    /// A placed carried file whose blob the object database lacks (lost with a damaged
    /// repository, or pruned) is stored again from the folder when the file there holds the
    /// placement's bytes. `true` when the object database has the blob after it.
    fn store_placed_bytes(
        &self,
        git: &Git,
        placement: &tc_journal::fold::CarriedPlacement,
    ) -> bool {
        if git.has(placement.oid) {
            return true;
        }
        let Ok((md5, size)) = self.hash_file(&placement.path) else {
            return false;
        };
        md5 == placement.md5
            && size == placement.size
            && self
                .store_file_as_blob(git, &placement.path, placement.oid)
                .unwrap_or(false)
    }

    /// C3 to C5 up to the commit object.
    fn commit_stage(&mut self, subject: &Subject, device: DeviceId) -> Result<Outcome, StoreError> {
        if self.read_only().is_some() {
            return Ok(Outcome::Skip(CheckpointSkip::ReadOnly));
        }
        if !self.complete {
            let paths: Vec<RepoPath> = self
                .packs_missing
                .iter()
                .filter_map(|p| RepoPath::parse(p).ok())
                .collect();
            if !self
                .shared
                .has_blocked_notice(&paths, &Blocked::PackMissing)
            {
                self.notice(
                    Notice::MaterializeBlocked {
                        paths,
                        reason: Blocked::PackMissing,
                    },
                    None,
                );
            }
            return Ok(Outcome::Skip(CheckpointSkip::PackMissing));
        }
        let Some(git) = self.git.take() else {
            return Ok(Outcome::Skip(CheckpointSkip::NoHistory));
        };
        // A repack removes no unreachable object until the branch has moved to the commit
        // (or the commit was given up): the tree may name objects nothing references yet.
        git.commit_begin();
        let result = self.commit_with(&git, subject, device);
        if !matches!(result, Ok(Outcome::Commit { .. })) {
            git.commit_end();
        }
        self.git = Some(git);
        result
    }

    fn commit_with(
        &mut self,
        git: &Git,
        subject: &Subject,
        device: DeviceId,
    ) -> Result<Outcome, StoreError> {
        let head = git.head()?;
        // A sync's merge: Door43's commit is the second parent, and its tree's entries the
        // project does not make are kept (§9.4).
        let remote = match subject {
            Subject::Checkpoint(crate::handle::CheckpointReason::SyncMerge { remote }) => {
                Some(*remote)
            }
            _ => None,
        }
        // Already in this history (an earlier merge took it): an ordinary checkpoint.
        .filter(|remote| match &head {
            None => true,
            Some(h) => h.commit != *remote && !git.descends_from(h.commit, *remote),
        });
        let remote_tree = remote.map(|r| git.tree_of(r)).transpose()?;
        let built = self.build_tree(git, head.as_ref(), remote_tree)?;
        if self.manifest_changed {
            // The blobs just stored: the next checkpoint finds them in the manifest.
            self.write_manifest()?;
        }
        if let Some(problem) = built.problem {
            self.notice(
                Notice::MaterializeBlocked {
                    paths: Vec::new(),
                    reason: Blocked::TreeInvalid {
                        problem: problem.into(),
                    },
                },
                None,
            );
            return Ok(Outcome::Skip(CheckpointSkip::TreeInvalid));
        }
        if built.same && remote.is_none() {
            return Ok(Outcome::Skip(CheckpointSkip::NoChanges));
        }
        let mut message = match subject {
            Subject::Checkpoint(crate::handle::CheckpointReason::SyncMerge { .. }) => {
                "Sync with Door43".to_owned()
            }
            Subject::Checkpoint(crate::handle::CheckpointReason::Received { from }) => {
                format!("Received work from {from}")
            }
            Subject::Checkpoint(reason) => format!("Checkpoint, {}", reason.words()),
            Subject::Recovered => "Recovered history".to_owned(),
        };
        if !built.books.is_empty() && matches!(subject, Subject::Checkpoint(_)) && remote.is_none()
        {
            let codes: Vec<&str> = built.books.iter().map(|b| b.code()).collect();
            message.push_str(&format!(": {}", codes.join(", ")));
        }
        message.push_str(&format!(
            " (tC)\n\n{TRAILER_DEVICE}: {device}\n{TRAILER_FORMAT}: {FORMAT_MAJOR}.{FORMAT_MINOR}\n{TRAILER_FRONTIER}: {}\n",
            self.fold.frontier().to_hex()
        ));
        let author = Author {
            name: self.author_name(device),
            time_ms: self.clock.now().0,
        };
        let parents: Vec<GitOid> = head.iter().map(|h| h.commit).chain(remote).collect();
        let commit = git.write_commit(built.tree, &parents, &author, &message)?;
        self.shared
            .hooks
            .wait(crate::hooks::Point::BeforeHeadUpdate);
        if self
            .shared
            .hooks
            .check(crate::hooks::Point::BeforeHeadUpdate)
            .is_err()
        {
            return Err(StoreError::Other(
                "stopped after the commit object was written, before the branch moved".into(),
            ));
        }
        Ok(Outcome::Commit {
            commit,
            expected: head.map(|h| h.commit),
            tree: built.tree,
        })
    }

    /// The Door43 username the device's profile holds (written only after consent, D98), else
    /// "translationCore user".
    fn author_name(&self, device: DeviceId) -> String {
        match self.fold.projected(&RecordKey::DeviceProfile(device)) {
            Some(Record::DeviceProfile { value, .. }) => value
                .door43_username
                .map(|u| u.as_str().to_owned())
                .unwrap_or_else(|| "translationCore user".to_owned()),
            _ => "translationCore user".to_owned(),
        }
    }

    /// The derived paths whose bytes on disk are not the snapshot's projection: held, refused
    /// by another program, left dirty by the last job, waiting to be recorded as an outside
    /// edit, or written from a newer snapshot. A tree takes these from the snapshot, never
    /// from the disk (§9.4).
    fn stale_paths(&self) -> BTreeSet<String> {
        let mut stale: BTreeSet<String> = self.busy.keys().cloned().collect();
        // Bytes found there wait to be recorded, and nothing was written over them (§11.1 D7).
        stale.extend(self.pending_record.keys().cloned());
        // A look at the folder that ran on a newer snapshot than the checkpoint's wrote
        // saves the checkpoint's packs do not hold.
        stale.extend(
            self.written_seq
                .iter()
                .filter(|(_, written)| **written > self.seq)
                .map(|(path, _)| path.clone()),
        );
        for book in &self.dirty.books {
            stale.extend(self.fold.book_files(*book).iter().map(DerivedFile::path));
        }
        stale.extend(self.dirty.files.iter().map(DerivedFile::path));
        if self.dirty.everything {
            stale.extend(self.fold.derived_files().iter().map(DerivedFile::path));
        }
        stale
    }

    /// Whether `path` of HEAD's tree is the app's to drop when the projection no longer
    /// holds it (§9.4): everything else is a foreign entry and stays.
    fn app_owned(&self, path: &str) -> bool {
        if matches!(
            path,
            GITIGNORE | GITATTRIBUTES | METADATA | PROJECT_JSON | FORMAT_JSON
        ) || path.starts_with("ingredients/tc/changes/")
            || path.starts_with("ingredients/tc4-sync/")
            || path.starts_with("ingredients/tc4-open/")
            || DerivedFile::from_path(path).is_some()
        {
            return true;
        }
        // A carried file the project has a record of, or the other version of one.
        let original = conflict_original(path);
        [Some(path.to_owned()), original]
            .into_iter()
            .flatten()
            .filter_map(|p| RepoPath::parse(&p).ok())
            .any(|p| self.fold.key_state(&RecordKey::CarriedFile(p)).is_some())
    }

    /// C3 and §9.4: the commit tree in memory.
    fn build_tree(
        &mut self,
        git: &Git,
        head: Option<&Head>,
        remote_tree: Option<GitOid>,
    ) -> Result<Built, StoreError> {
        let head_entries: BTreeMap<String, TreeEntry> = match head {
            Some(h) => git.tree_entries(h.tree)?,
            None => BTreeMap::new(),
        };
        let mut desired: BTreeMap<String, Desired> = BTreeMap::new();
        let nfc_of = |rel: &str, bytes: &[u8]| {
            is_text_path(rel).then(|| is_nfc(&String::from_utf8_lossy(bytes)))
        };

        // The app's own files.
        for (rel, bytes) in [
            (GITIGNORE, GITIGNORE_BYTES),
            (GITATTRIBUTES, GITATTRIBUTES_BYTES),
        ] {
            desired.insert(
                rel.to_owned(),
                Desired {
                    oid: git.ensure_blob(bytes)?,
                    mode: MODE_BLOB,
                    md5: Md5::of(bytes),
                    size: bytes.len() as u64,
                    nfc: None,
                },
            );
        }
        // The files written once.
        for rel in crate::materialize::FIXED {
            if let Disk::Bytes(bytes) = self.read_disk(rel) {
                desired.insert(
                    rel.to_owned(),
                    Desired {
                        oid: git.ensure_blob(&bytes)?,
                        mode: MODE_BLOB,
                        md5: Md5::of(&bytes),
                        size: bytes.len() as u64,
                        nfc: None,
                    },
                );
            }
        }

        // Derived files: the manifest's blob when the last materialization made the file
        // equal to the projection, else the projection itself.
        let stale = self.stale_paths();
        let mut from_snapshot: BTreeSet<String> = stale.clone();
        let entries: Vec<(String, crate::manifest::Entry)> = self
            .manifest
            .paths
            .iter()
            .filter(|(p, e)| {
                p.as_str() != METADATA
                    && !crate::materialize::FIXED.contains(&p.as_str())
                    && e.oid.is_none()
                    && DerivedFile::from_path(p).is_some()
            })
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        for (path, entry) in entries {
            // Bytes the ledger knows placed there; a hold, a write under way or a path a
            // newer snapshot wrote comes from the snapshot.
            let placed = entry
                .placement()
                .filter(|_| !stale.contains(&path))
                .cloned();
            let Some((md5, size)) = placed.and_then(|p| Md5::parse(&p.md5).map(|m| (m, p.size)))
            else {
                from_snapshot.insert(path);
                continue;
            };
            let known = entry.blob.as_deref().and_then(GitOid::parse).filter(|oid| {
                head_entries.get(&path).is_some_and(|e| e.oid == *oid) || git.has(*oid)
            });
            if let Some(oid) = known {
                desired.insert(
                    path,
                    Desired {
                        oid,
                        mode: MODE_BLOB,
                        md5,
                        size,
                        nfc: entry.nfc,
                    },
                );
                continue;
            }
            match self.read_disk(&path) {
                Disk::Bytes(bytes) if Md5::of(&bytes) == md5 => {
                    let oid = git.ensure_blob(&bytes)?;
                    if let Some(e) = self.manifest.paths.get_mut(&path) {
                        e.blob = Some(oid.to_hex());
                        self.manifest_changed = true;
                    }
                    desired.insert(
                        path,
                        Desired {
                            oid,
                            mode: MODE_BLOB,
                            md5,
                            size: bytes.len() as u64,
                            nfc: entry.nfc,
                        },
                    );
                }
                _ => {
                    from_snapshot.insert(path);
                }
            }
        }
        for path in from_snapshot {
            let Some(file) = DerivedFile::from_path(&path) else {
                continue;
            };
            let Some(bytes) = self.fold.render(&file) else {
                continue;
            };
            self.counters.add(&self.counters.projected, 1);
            desired.insert(
                path.clone(),
                Desired {
                    oid: git.ensure_blob(&bytes)?,
                    mode: MODE_BLOB,
                    md5: Md5::of(&bytes),
                    size: bytes.len() as u64,
                    nfc: nfc_of(&path, &bytes),
                },
            );
        }

        // Packs: every pack of the working tree, but those a valid pack replaced (§4.7).
        let replaced: BTreeSet<PackName> = self
            .packs
            .values()
            .filter(|p| p.valid)
            .flat_map(|p| p.replaces.iter().copied())
            .collect();
        let pack_rels: Vec<String> = self.packs.keys().cloned().collect();
        for rel in pack_rels {
            let redundant =
                tc_journal::pack::PackPath::parse(&rel).is_some_and(|p| replaced.contains(&p.name));
            if redundant {
                continue;
            }
            let Some(entry) = self.pack_entry_for_tree(&rel)? else {
                return Ok(self.unreadable_pack(&rel));
            };
            let facts = self.packs.get(&rel).cloned();
            let candidates = [
                facts.and_then(|f| f.blob),
                entry.blob.as_deref().and_then(GitOid::parse),
                head_entries.get(&rel).map(|e| e.oid),
            ];
            let oid = match candidates.into_iter().flatten().find(|oid| git.has(*oid)) {
                Some(oid) => oid,
                None => match self.read_disk(&rel) {
                    Disk::Bytes(bytes) => git.ensure_blob(&bytes)?,
                    _ => return Ok(self.unreadable_pack(&rel)),
                },
            };
            if entry.blob.as_deref() != Some(oid.to_hex().as_str())
                && let Some(e) = self.manifest.packs.get_mut(&rel)
            {
                e.blob = Some(oid.to_hex());
                self.manifest_changed = true;
            }
            desired.insert(
                rel,
                Desired {
                    oid,
                    mode: MODE_BLOB,
                    md5: Md5::parse(&entry.md5).unwrap_or_else(|| Md5::of(b"")),
                    size: entry.size,
                    nfc: None,
                },
            );
        }

        // Carried files: the bytes are blobs the object database holds (§11.4). A blob it
        // lacks is stored again when the folder holds its bytes; else the head's blob stays at
        // that path, or the file is left out; the commit is never refused for it (§9.4).
        for placement in self.fold.carried_files() {
            self.store_placed_bytes(git, &placement);
            if let Some(d) =
                self.carried_entry(git, &placement, head_entries.get(&placement.path))?
            {
                desired.insert(placement.path.clone(), d);
            } else if let Ok(path) = RepoPath::parse(&placement.path) {
                self.notice(
                    Notice::CarriedBytesMissing {
                        path,
                        oid: placement.oid,
                    },
                    None,
                );
            }
        }

        // Foreign entries stay as they are (§9.4).
        let mut moved: Vec<(String, String)> = Vec::new();
        for (path, entry) in &head_entries {
            if desired.contains_key(path) || self.app_owned(path) {
                continue;
            }
            let ingredient_like = path.starts_with("ingredients/")
                && !path.split('/').any(|part| part.starts_with('.'));
            if ingredient_like && IngredientPath::parse(path).is_err() {
                // A name that cannot be an ingredient moves to a dot path burrito readers skip.
                moved.push((path.clone(), format!(".tc-moved/{path}")));
                continue;
            }
            let (md5, size) = if ingredient_like {
                match git.read_blob(entry.oid)? {
                    Some(bytes) => (Md5::of(&bytes), bytes.len() as u64),
                    None => continue,
                }
            } else {
                (Md5::of(b""), 0)
            };
            desired.insert(
                path.clone(),
                Desired {
                    oid: entry.oid,
                    mode: entry.mode,
                    md5,
                    size,
                    nfc: None,
                },
            );
        }
        // A sync's merge keeps what Door43's tree holds that the project does not make: its
        // foreign entries, with Door43's blob, and its packs this project has not taken (a
        // damaged copy, a newer format), so no pack of R ever leaves the tip (§9.2 N3, §9.4
        // "No entry of R is dropped silently"). Derived files come from the fold.
        if let Some(remote_tree) = remote_tree {
            for (path, entry) in git.tree_entries(remote_tree)? {
                let pack = path.starts_with("ingredients/tc/changes/");
                let foreign = !self.app_owned(&path);
                let redundant = tc_journal::pack::PackPath::parse(&path)
                    .is_some_and(|p| replaced.contains(&p.name));
                if desired.contains_key(&path) || redundant || !(pack || foreign) {
                    continue;
                }
                let ingredient = path.starts_with("ingredients/")
                    && !path.split('/').any(|part| part.starts_with('.'));
                if ingredient && IngredientPath::parse(&path).is_err() {
                    desired.insert(
                        format!(".tc-moved/{path}"),
                        Desired {
                            oid: entry.oid,
                            mode: entry.mode,
                            md5: Md5::of(b""),
                            size: 0,
                            nfc: None,
                        },
                    );
                    continue;
                }
                let (md5, size) = if ingredient {
                    match git.read_blob(entry.oid)? {
                        Some(bytes) => (Md5::of(&bytes), bytes.len() as u64),
                        None => continue,
                    }
                } else {
                    (Md5::of(b""), 0)
                };
                desired.insert(
                    path,
                    Desired {
                        oid: entry.oid,
                        mode: entry.mode,
                        md5,
                        size,
                        nfc: None,
                    },
                );
            }
        }
        for (from, to) in &moved {
            if let Some(entry) = head_entries.get(from) {
                desired.insert(
                    to.clone(),
                    Desired {
                        oid: entry.oid,
                        mode: entry.mode,
                        md5: Md5::of(b""),
                        size: 0,
                        nfc: None,
                    },
                );
            }
        }

        // metadata.json lists exactly the ingredients of the tree.
        let metadata = match self.tree_metadata(&desired) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Ok(Built {
                    tree: GitOid::from_bytes([0; 20]),
                    same: false,
                    books: BTreeSet::new(),
                    problem: Some(e.to_string()),
                });
            }
        };
        desired.insert(
            METADATA.to_owned(),
            Desired {
                oid: git.ensure_blob(&metadata)?,
                mode: MODE_BLOB,
                md5: Md5::of(&metadata),
                size: metadata.len() as u64,
                nfc: None,
            },
        );
        let problems = tc_burrito::validate_project(&metadata, &TreeFiles(&desired));
        if let Some(first) = problems.first() {
            return Ok(Built {
                tree: GitOid::from_bytes([0; 20]),
                same: false,
                books: BTreeSet::new(),
                problem: Some(format!("{first:?}")),
            });
        }

        // The tree: HEAD's with what differs set.
        let mut upserts: Vec<(String, GitOid, u32)> = Vec::new();
        let mut removes: Vec<String> = Vec::new();
        let mut books: BTreeSet<BookId> = BTreeSet::new();
        let note_book = |path: &str, books: &mut BTreeSet<BookId>| {
            if let Some(book) = DerivedFile::from_path(path).and_then(|f| f.book()) {
                books.insert(book);
            }
        };
        for (path, d) in &desired {
            let same = head_entries
                .get(path)
                .is_some_and(|e| e.oid == d.oid && e.mode == d.mode);
            if !same {
                note_book(path, &mut books);
                upserts.push((path.clone(), d.oid, d.mode));
            }
        }
        for path in head_entries.keys() {
            if !desired.contains_key(path) {
                note_book(path, &mut books);
                removes.push(path.clone());
            }
        }
        let base = head.map(|h| h.tree);
        let tree = if upserts.is_empty() && removes.is_empty() {
            match base {
                Some(tree) => tree,
                None => git.update_tree(None, &[], &[])?,
            }
        } else {
            git.update_tree(base, &upserts, &removes)?
        };
        Ok(Built {
            same: Some(tree) == base,
            tree,
            books,
            problem: None,
        })
    }

    /// The pack's facts for a tree: the manifest's, read once when it has none.
    fn pack_entry_for_tree(
        &mut self,
        rel: &str,
    ) -> Result<Option<crate::manifest::PackEntry>, StoreError> {
        self.pack_entry(rel)
    }

    fn unreadable_pack(&mut self, rel: &str) -> Built {
        if let Ok(path) = RepoPath::parse(rel) {
            self.notice(
                Notice::MaterializeBlocked {
                    paths: vec![path],
                    reason: Blocked::PackMissing,
                },
                None,
            );
        }
        Built {
            tree: GitOid::from_bytes([0; 20]),
            same: false,
            books: BTreeSet::new(),
            problem: Some(format!("{rel} could not be read")),
        }
    }

    /// A carried file's entry: its blob when the object database has it, else the head's.
    fn carried_entry(
        &self,
        git: &Git,
        placement: &tc_journal::fold::CarriedPlacement,
        head: Option<&TreeEntry>,
    ) -> Result<Option<Desired>, StoreError> {
        // libgit2 refuses a tree a new entry of which names a blob whose header does not
        // read: a damaged blob is as missing as an absent one, and never refuses the commit
        // (§9.4). An entry HEAD has already stays as it is, and nothing reads its blob.
        let in_head = head.is_some_and(|h| h.oid == placement.oid && h.mode == MODE_BLOB);
        if git.has(placement.oid) && (in_head || git.has_readable_blob(placement.oid)) {
            return Ok(Some(Desired {
                oid: placement.oid,
                mode: MODE_BLOB,
                md5: placement.md5,
                size: placement.size,
                nfc: None,
            }));
        }
        let Some(head) = head else {
            return Ok(None);
        };
        Ok(git.read_blob(head.oid)?.map(|bytes| Desired {
            oid: head.oid,
            mode: head.mode,
            md5: Md5::of(&bytes),
            size: bytes.len() as u64,
            nfc: None,
        }))
    }

    /// `metadata.json` for the ingredients of the tree (§3.4): each with the md5 and size of
    /// the bytes in the tree, not of the bytes on disk.
    fn tree_metadata(&self, desired: &BTreeMap<String, Desired>) -> Result<Vec<u8>, StoreError> {
        let probe = self.scope_probe();
        let mut entries: Vec<IngredientEntry> = Vec::new();
        let mut nfc = true;
        for (path, d) in desired {
            let listed = path.starts_with("ingredients/")
                && !path.split('/').any(|part| part.starts_with('.'));
            if !listed {
                continue;
            }
            let Ok(ingredient) = IngredientPath::parse(path) else {
                continue;
            };
            let Some(md5) = tc_burrito::Md5::parse(&d.md5.to_hex()) else {
                continue;
            };
            let (role, scope) = match DerivedFile::from_path(path) {
                Some(file) => {
                    if matches!(file, DerivedFile::Usfm(_) | DerivedFile::Story(_)) {
                        nfc &= d.nfc.unwrap_or(false);
                    }
                    let ranges = file
                        .book()
                        .map(|b| book_ranges(&probe, b))
                        .unwrap_or_default();
                    (file.role().map(str::to_owned), file.scope(&ranges))
                }
                None if path.starts_with("ingredients/tc/changes/") => {
                    (Some("x-tc-changes".to_owned()), None)
                }
                None if path == PROJECT_JSON || path == FORMAT_JSON => {
                    (Some("x-tc-project".to_owned()), None)
                }
                None => (None, None),
            };
            entries.push(IngredientEntry {
                path: ingredient,
                md5,
                size: d.size,
                role,
                scope,
            });
        }
        self.assemble_metadata(&entries, nfc)
    }
}

/// The path a `.conflict-<oid8>` or `.case-<oid8>` placement stands for, if `path` is one.
fn conflict_original(path: &str) -> Option<String> {
    let (stem, ext) = match path.rsplit_once('.') {
        Some((stem, ext)) if !ext.contains('/') => (stem, Some(ext)),
        _ => (path, None),
    };
    let (head, tail) = stem.rsplit_once('.')?;
    let hex = tail
        .strip_prefix("conflict-")
        .or_else(|| tail.strip_prefix("case-"))?;
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(match ext {
        Some(ext) => format!("{head}.{ext}"),
        None => head.to_owned(),
    })
}
