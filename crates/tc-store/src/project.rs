//! The state the project actor owns (`docs/STORAGE.md` §5.0: one writer per project) and
//! `commit_batch`, the durability point of every save.

use crate::appdata::AppData;
use crate::atomic::{SyncDir, WriteError, atomic_create_dirs, atomic_write};
use crate::clock::Clock;
use crate::commands::{Ctx, Plan};
use crate::counters::Counters;
use crate::handle::{Shared, StoreEvent};
use crate::layout::{Folder, PENDING};
use crate::reply::{CommandError, Refusal, StoreReply, Stored};
use crate::source::FileSource;
use crate::vfs::Vfs;
use crate::writer::{Job, PackFacts};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use tc_journal::command::PlannedChange;
use tc_journal::fold::{Delta, DerivedFile, FoldInput, FoldState, ProjectKind, Resolved};
use tc_journal::index::Section;
use tc_journal::pending::{PendingHeader, Purpose, encode_pending};
use tc_journal::{
    BatchId, BatchRef, BookId, Change, ChangeBody, ChangeId, DeviceId, DeviceSeq, FORMAT_MINOR,
    PackId, ProjectId, RecordKey, TokenKey, Verified,
};

/// A pack this session knows.
#[derive(Debug, Clone)]
pub(crate) struct PackInfo {
    pub id: PackId,
    pub size: u64,
    pub fingerprint: u64,
    /// The loader's number, when the fold's values may lie in it.
    pub number: Option<u32>,
    pub valid: bool,
    pub sealer: Option<DeviceId>,
    /// The names its header says it replaces (§4.7).
    pub replaces: Vec<tc_journal::PackName>,
    /// Its change ids while the open's rules need them (§6.1 O7): only for a pack that
    /// replaces others; empty otherwise.
    pub ids: Vec<ChangeId>,
    /// The git blob that holds the pack's bytes, when this session stored it (§5.11 K3).
    pub blob: Option<tc_journal::GitOid>,
    /// The size and modification time the pack had when this session last looked at it, so a
    /// checkpoint reads again only a pack that changed (§4.4 "The expected pack set").
    pub seen: Option<(u64, i64)>,
    /// The pack failed P2 to P4 when it was read (damaged, as opposed to newer or foreign).
    pub damaged: bool,
}

/// A pending file this session knows.
#[derive(Debug, Clone)]
pub(crate) struct PendingInfo {
    pub ids: Vec<ChangeId>,
    /// The outside edit this file records, by path and md5 (§11.1 X4), so the same bytes are
    /// never recorded twice.
    pub reconciles: Option<(String, tc_journal::Md5)>,
    /// The loader's number, when the fold's values may lie in it (a file read at open).
    pub number: Option<u32>,
    pub status: tc_journal::pending::PendingStatus,
}

/// What the actor knows of the journal's files.
#[derive(Debug, Default)]
pub(crate) struct Journal {
    pub packs: BTreeMap<String, PackInfo>,
    pub pending: BTreeMap<String, PendingInfo>,
    /// Files that could not be read: never removed, never treated as damaged (§4.4).
    pub unreadable: BTreeSet<String>,
    /// Expected packs (§4.4) missing from the working tree with no copy to restore.
    pub missing: BTreeSet<String>,
    /// Journal files this session made redundant (a sealed pending file, a replaced pack) whose
    /// removal another program refused: they stay for the next open (O7), and the session that
    /// left them never takes them for a second writer's (§5.0).
    pub left: BTreeSet<String>,
}

impl Journal {
    /// A pending file holds saves a seal can take: one of a newer format or of another
    /// project, or one another program holds, waits for no seal (§5.11 K1).
    pub fn has_unsealed(&self) -> bool {
        self.pending.iter().any(|(rel, p)| {
            p.status == tc_journal::pending::PendingStatus::Valid
                && !p.ids.is_empty()
                && !self.unreadable.contains(rel)
        })
    }

    /// While an expected pack is missing or a journal file unreadable, nothing derived is
    /// removed (§5.10 M0).
    pub fn complete(&self) -> bool {
        self.unreadable.is_empty() && self.missing.is_empty()
    }

    /// The files that keep it from being complete.
    pub fn blocked_packs(&self) -> Vec<String> {
        self.missing
            .iter()
            .chain(self.unreadable.iter())
            .cloned()
            .collect()
    }
}

/// The fold cache's sections, kept up to date as files are read and written (§3.6).
#[derive(Debug, Default)]
pub(crate) struct IndexCache {
    pub sections: HashMap<String, Section>,
    pub changed: bool,
}

/// What to write again (§5.0 B5): the actor marks books and files, and the writer expands
/// them from its snapshot, off the actor.
#[derive(Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per kind of file to write again; any mix can be dirty at once"
)]
pub(crate) struct Dirty {
    /// Every file of these books is reconciled with the projection: rendered, written when it
    /// differs, removed when the book no longer projects it.
    pub books: BTreeSet<BookId>,
    /// These files only (a file of no book, or one file of a book).
    pub files: BTreeSet<DerivedFile>,
    /// A `DeviceProfile` changed: the authors' names show in every comments file and in
    /// `conflicts.json`.
    pub comments_all: bool,
    /// A story's text changed: a decision on it may be flagged, so every tool's story
    /// decisions file shows it (as a book's files do for a verse).
    pub story_decisions_all: bool,
    /// A `CarriedFile` record changed: carried files are placed or removed (§11.4).
    pub carried: bool,
    /// Every derived file may have changed (a rebuilt fold, a pin group).
    pub everything: bool,
    /// `metadata.json` lists something that changed (a new pack, a removed file).
    pub metadata: bool,
    /// The work is a book added, removed, restored or rescoped, a pin change or a resource
    /// upgrade: it shows a progress indicator when it lasts (PERFORMANCE.md rule 4).
    pub progress: bool,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
            && self.files.is_empty()
            && !self.comments_all
            && !self.story_decisions_all
            && !self.carried
            && !self.everything
            && !self.metadata
    }

    /// Adds what `other` marks.
    pub fn absorb(&mut self, other: Dirty) {
        self.books.extend(other.books);
        self.files.extend(other.files);
        self.comments_all |= other.comments_all;
        self.story_decisions_all |= other.story_decisions_all;
        self.carried |= other.carried;
        self.everything |= other.everything;
        self.metadata |= other.metadata;
        self.progress |= other.progress;
    }

    /// Marks one derived file: its book, when it is one of a book's files, so the book's
    /// records are decoded once for all its files.
    pub fn add_file(&mut self, file: DerivedFile) {
        match file.book() {
            Some(book) => {
                self.books.insert(book);
            }
            None => {
                self.files.insert(file);
            }
        }
    }

    /// Marks what a fold step changed. A book change marks that book only (B5).
    /// `conflicted`: the step listed or unlisted a conflict, or changed a record that is in
    /// one, so `conflicts.json` shows something different.
    pub fn mark(&mut self, kind: ProjectKind, delta: &Delta, conflicted: bool) {
        self.metadata = true;
        if conflicted {
            self.files.insert(DerivedFile::Conflicts);
        }
        if delta.everything || delta.pins {
            // A pin group changes every book's decisions and alignments and `resources.json`.
            self.everything = true;
            self.progress = true;
            return;
        }
        if delta.paths {
            self.progress = true;
        }
        if delta.carried {
            self.carried = true;
        }
        self.books.extend(delta.books.iter().copied());
        self.books
            .extend(delta.chapters.iter().map(|(book, _)| *book));
        for key in &delta.keys {
            match key {
                RecordKey::Setting(_) => {
                    self.files.insert(DerivedFile::Settings);
                }
                RecordKey::Versification => {
                    self.files.insert(DerivedFile::Versification);
                }
                RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => {
                    self.files.insert(DerivedFile::Resources);
                }
                RecordKey::StoryBase(s) | RecordKey::Frame(s, _) | RecordKey::StoryRef(s) => {
                    self.files.insert(DerivedFile::Story(*s));
                    self.story_decisions_all = true;
                }
                RecordKey::DeviceProfile(_) => self.comments_all = true,
                RecordKey::Comment(_) => {
                    if kind == ProjectKind::Stories {
                        self.files.insert(DerivedFile::StoryComments);
                    }
                    if let Some(book) = key.book() {
                        self.books.insert(book);
                    }
                }
                RecordKey::Check(check) => match key.book() {
                    Some(book) => {
                        self.books.insert(book);
                    }
                    None => {
                        self.files.insert(DerivedFile::StoryDecisions(check.tool));
                    }
                },
                other => {
                    if let Some(book) = other.book() {
                        self.books.insert(book);
                    }
                }
            }
        }
    }
}

/// Whether a fold step changed the conflicts the project lists: one was listed or unlisted,
/// or a record that is in one changed (its candidates are in `conflicts.json`).
pub(crate) fn conflicts_touched(fold: &FoldState, delta: &Delta) -> bool {
    delta.conflicts_changed
        || !delta.new_conflicts.is_empty()
        || delta.keys.iter().any(|key| {
            fold.key_state(key)
                .is_some_and(|state| matches!(state.state, Resolved::Conflict(_)))
        })
}

/// See the module documentation.
pub(crate) struct Project {
    pub vfs: Arc<dyn Vfs>,
    pub counters: Arc<Counters>,
    pub folder: Folder,
    pub id: ProjectId,
    pub kind: ProjectKind,
    pub device: DeviceId,
    pub next_seq: u64,
    pub signing: Arc<TokenKey>,
    pub fold: FoldState,
    pub source: Arc<FileSource>,
    pub clock: Arc<dyn Clock>,
    pub app: Arc<AppData>,
    pub journal: Journal,
    pub index: IndexCache,
    /// What changed since the last job sent to the writer (§5.0 B5).
    pub dirty: Dirty,
    pub shared: Arc<Shared>,
    /// The writer's queue (§5.0 "One writer per project").
    pub writer: Option<Sender<Job>>,
    /// The actor's handle on the project's repository: it moves the branch and reads the
    /// objects a command asks about. `None` when the session has no history (§6.1 O3).
    pub git: Option<crate::git::Git>,
}

impl Project {
    /// What the writer needs of the packs the actor knows.
    pub fn pack_facts(&self) -> BTreeMap<String, PackFacts> {
        self.journal
            .packs
            .iter()
            .map(|(rel, p)| {
                (
                    rel.clone(),
                    PackFacts {
                        size: p.size,
                        fingerprint: p.fingerprint,
                        valid: p.valid,
                        replaces: p.replaces.clone(),
                        blob: p.blob,
                    },
                )
            })
            .collect()
    }

    /// The job that hands the dirty paths to the writer with the newest snapshot.
    pub fn materialize_job(
        &mut self,
        origin: crate::writer::Origin,
        reply: Option<crate::handle::Resolver<Result<(), crate::error::StoreError>>>,
        checkpoint: Option<crate::writer::CheckpointJob>,
    ) -> crate::writer::MaterializeJob {
        crate::writer::MaterializeJob {
            snapshot: self.shared.snapshot(),
            dirty: std::mem::take(&mut self.dirty),
            packs: self.pack_facts(),
            complete: self.journal.complete(),
            missing: self.journal.blocked_packs(),
            origin,
            reply,
            checkpoint,
        }
    }

    /// The checkpoint's job: the snapshot its packs stand for, with the marks made before it
    /// (the new packs are listed in `metadata.json` as well).
    pub fn checkpoint_job(
        &mut self,
        snapshot: crate::handle::Snapshot,
        mut dirty: Dirty,
        checkpoint: crate::writer::CheckpointJob,
    ) -> crate::writer::MaterializeJob {
        dirty.metadata = true;
        crate::writer::MaterializeJob {
            snapshot,
            dirty,
            packs: self.pack_facts(),
            complete: self.journal.complete(),
            missing: self.journal.blocked_packs(),
            origin: crate::writer::Origin::Checkpoint,
            reply: None,
            checkpoint: Some(checkpoint),
        }
    }

    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            fold: &self.fold,
            signing: &self.signing,
            kind: self.kind,
            device: self.device,
            git: self.git.as_ref(),
        }
    }

    /// Publishes the fold as the snapshot readers take (R5), and returns its sequence
    /// number.
    pub fn publish(&self) -> u64 {
        self.shared.publish(self.fold.clone())
    }

    pub fn notice(&self, notice: tc_journal::fold::Notice, copy: Option<String>) {
        let identity =
            crate::notices::CopyIdentity::capture(&*self.vfs, &self.folder, copy.as_deref());
        let notice = self.shared.add_notice(notice, copy, identity);
        self.shared.emit(StoreEvent::Notice(notice));
    }

    /// B2 to B6 of `commit_batch` (§5.0) for a planned batch: assigns sequence numbers, the
    /// time and the batch, writes one pending file durably, folds the changes, marks the
    /// derived files dirty and replies with the new tokens. Nothing is written when a change
    /// breaks a rule of §4.5.
    pub fn commit(&mut self, plan: Plan) -> Result<StoreReply, CommandError> {
        self.commit_reconciling(plan, None)
    }

    /// [`Project::commit`] for the batch that records an outside edit: its pending file
    /// names the file and the md5 it reconciles (§11.1 X4).
    pub fn commit_reconciling(
        &mut self,
        plan: Plan,
        reconciles: Option<tc_journal::pending::Reconciles>,
    ) -> Result<StoreReply, CommandError> {
        if plan.changes.is_empty() {
            return Ok(StoreReply {
                tokens: Vec::new(),
                new_conflicts: Vec::new(),
                stored: Stored::Unchanged,
                seq: self.shared.seq(),
                changes: Vec::new(),
                skipped: plan.skipped,
                kept: plan.kept,
                dropped_links: plan.dropped_links,
            });
        }
        let changes = self.build(&plan.changes)?;
        let stored = self.write_pending(&changes, Purpose::Save, reconciles)?;
        self.next_seq += changes.len() as u64;
        let ids: Vec<ChangeId> = changes.iter().map(Change::id).collect();
        let keys: Vec<RecordKey> = changes.iter().map(|c| c.body().record.key()).collect();
        let delta = self.fold.add(changes.into_iter().map(FoldInput::from));
        let conflicted = conflicts_touched(&self.fold, &delta);
        self.dirty.mark(self.kind, &delta, conflicted);
        let seq = self.publish();
        let mut tokens = Vec::with_capacity(keys.len());
        for (key, id) in keys.iter().zip(&ids) {
            if let Ok(token) = self.fold.token(&self.signing, key, Some(*id)) {
                tokens.push((key.clone(), token));
            }
        }
        if let Stored::WrittenNotDurable(reason) = &stored {
            self.notice(
                tc_journal::fold::Notice::SavedWithDiskError {
                    reason: reason.as_str().into(),
                },
                None,
            );
        }
        self.shared.emit(StoreEvent::Committed {
            seq,
            delta: delta.clone(),
            changes: ids.clone(),
        });
        Ok(StoreReply {
            tokens,
            new_conflicts: delta.new_conflicts,
            stored,
            seq,
            changes: ids,
            skipped: plan.skipped,
            kept: plan.kept,
            dropped_links: plan.dropped_links,
        })
    }

    /// B2: one change per planned change, canonical and checked as every reader will.
    fn build(&self, planned: &[PlannedChange]) -> Result<Vec<Change>, CommandError> {
        let mut keys = BTreeSet::new();
        for p in planned {
            if !keys.insert(p.record.key()) {
                return Err(CommandError::Refused(Refusal::TwoChangesOfOneRecord));
            }
        }
        let n = u32::try_from(planned.len())
            .map_err(|_| CommandError::Refused(Refusal::Rule("the batch is too large".into())))?;
        let batch = BatchRef::new(BatchId::from_bytes(crate::random::bytes::<16>()), n).ok_or(
            CommandError::Refused(Refusal::Rule("an empty batch".into())),
        )?;
        let at = self.clock.now();
        planned
            .iter()
            .enumerate()
            .map(|(i, p)| {
                Change::new(ChangeBody {
                    v: FORMAT_MINOR,
                    record: p.record.clone(),
                    parents: p.parents.clone(),
                    device: self.device,
                    seq: DeviceSeq(self.next_seq + i as u64),
                    at,
                    batch,
                    cause: p.cause.clone(),
                })
                .map_err(|e| CommandError::Refused(Refusal::Rule(e.to_string())))
            })
            .collect()
    }

    /// B3: one atomic pending file, flushed with its rename before anything is shown.
    pub fn write_pending(
        &mut self,
        changes: &[Change],
        purpose: Purpose,
        reconciles: Option<tc_journal::pending::Reconciles>,
    ) -> Result<Stored, CommandError> {
        let header = PendingHeader {
            device: self.device,
            project: self.id,
            purpose,
            reconciles: reconciles.clone(),
        };
        let lines: Vec<Verified> = changes.iter().cloned().map(Verified::from).collect();
        let encoded = encode_pending(&header, &lines)
            .map_err(|e| CommandError::Refused(Refusal::Rule(e.to_string())))?;
        self.shared
            .hooks
            .wait(crate::hooks::Point::BeforeDurablePending);
        // The writer's flushes wait behind this one (PERFORMANCE.md).
        let _in_flight = self.shared.gate.enter();
        let dir = self.folder.path(PENDING);
        if let Err(e) = atomic_create_dirs(&*self.vfs, &dir) {
            return Err(CommandError::NotWritten(e.to_string()));
        }
        let stored = match atomic_write(
            &*self.vfs,
            &dir,
            &encoded.name,
            &encoded.bytes,
            SyncDir::Now,
        ) {
            Ok(()) => Stored::Durable,
            Err(WriteError::NotWritten(e)) => return Err(CommandError::NotWritten(e.to_string())),
            Err(WriteError::WrittenNotDurable(e)) => Stored::WrittenNotDurable(e.to_string()),
        };
        self.journal.pending.insert(
            format!("{PENDING}/{}", encoded.name),
            PendingInfo {
                ids: changes.iter().map(Change::id).collect(),
                reconciles: reconciles.map(|r| (r.path.as_str().to_owned(), r.seen_md5)),
                number: None,
                status: tc_journal::pending::PendingStatus::Valid,
            },
        );
        Ok(stored)
    }
}

impl Project {
    /// Commits the plan of an outside edit as one batch (§11.1 X4). A record a pending batch
    /// that reconciles the same bytes already changed is not recorded twice.
    pub(crate) fn record_outside(
        &mut self,
        rel: &str,
        seen: tc_journal::Md5,
        plan: tc_journal::outside::OutsidePlan,
        derived: u32,
    ) -> crate::writer::Recorded {
        use crate::writer::Recorded;
        let (unknown_base, whitespace_only) = (plan.unknown_base, plan.whitespace_only);
        let kept = u32::try_from(plan.kept_removals.len()).unwrap_or(u32::MAX);
        let Ok(path) = tc_journal::RepoPath::parse_source(rel) else {
            return Recorded::Refused("its name is not a name translationCore can record".into());
        };
        let done = |batch, changes, heads| Recorded::Done {
            batch,
            changes,
            unknown_base,
            whitespace_only,
            kept,
            derived,
            heads,
        };
        let already = (rel.to_owned(), seen);
        // Recorded before (a crash between the batch and the file's rewrite, or a look that
        // found the same bytes again): the changes of those batches are the base of the bytes
        // in the folder all the same, and only a record they did not change is recorded now
        // (the fold may have lost since a change the bytes show, §6.2).
        let earlier: Vec<(tc_journal::RecordKey, tc_journal::ChangeId)> = self
            .journal
            .pending
            .values()
            .filter(|p| p.reconciles.as_ref() == Some(&already))
            .flat_map(|p| p.ids.iter())
            .filter_map(|id| {
                let view = self.fold.change(id)?;
                // A surviving member of an incomplete batch has not recorded this edit.
                // Its bytes must also remain readable before they can replace a new record.
                if !view.folded || self.fold.raw(id).is_none() {
                    return None;
                }
                // A change the project wrote over since no longer stands for the bytes: the
                // same bytes found after that are a new edit, a revert (§11.1 X3).
                if !self.fold.heads(&view.key).contains(id) {
                    return None;
                }
                Some((view.key, *id))
            })
            .collect();
        let mut planned = plan.changes;
        if !earlier.is_empty() {
            let keys: std::collections::BTreeSet<&tc_journal::RecordKey> =
                earlier.iter().map(|(key, _)| key).collect();
            planned.retain(|c| !keys.contains(&c.record.key()));
            if planned.is_empty() {
                return done(None, 0, earlier);
            }
        }
        let changes = u32::try_from(planned.len()).unwrap_or(u32::MAX);
        let plan = Plan {
            changes: planned,
            skipped: Vec::new(),
            kept: Vec::new(),
            dropped_links: Vec::new(),
        };
        let reconciles = tc_journal::pending::Reconciles {
            path,
            seen_md5: seen,
        };
        match self.commit_reconciling(plan, Some(reconciles)) {
            Ok(reply) => {
                let batch = reply
                    .changes
                    .first()
                    .and_then(|id| self.fold.change(id))
                    .map(|view| view.batch);
                let mut heads = earlier;
                heads.extend(
                    reply
                        .changes
                        .iter()
                        .filter_map(|id| Some((self.fold.change(id)?.key, *id))),
                );
                done(batch, changes, heads)
            }
            Err(CommandError::NotWritten(_)) => Recorded::NotWritten,
            Err(e) => Recorded::Refused(e.to_string()),
        }
    }
}
