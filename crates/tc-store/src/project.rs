//! The state the project actor owns (`docs/STORAGE.md` §5.0: one writer per project) and
//! `commit_batch`, the durability point of every save.

use crate::appdata::AppData;
use crate::atomic::{SyncDir, WriteError, atomic_create_dirs, atomic_write};
use crate::clock::Clock;
use crate::commands::{Ctx, Plan};
use crate::counters::Counters;
use crate::handle::{Shared, StoreEvent};
use crate::layout::{Folder, PENDING};
use crate::manifest::Manifest;
use crate::reply::{CommandError, Refusal, StoreReply, Stored};
use crate::source::FileSource;
use crate::vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use tc_journal::command::PlannedChange;
use tc_journal::fold::{Delta, DerivedFile, FoldInput, FoldState, ProjectKind};
use tc_journal::index::Section;
use tc_journal::pending::{PendingHeader, Purpose, encode_pending};
use tc_journal::{
    BatchId, BatchRef, Change, ChangeBody, ChangeId, DeviceId, DeviceSeq, FORMAT_MINOR, PackId,
    ProjectId, RecordKey, TokenKey, Verified,
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
}

/// A pending file this session knows.
#[derive(Debug, Clone)]
pub(crate) struct PendingInfo {
    pub ids: Vec<ChangeId>,
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
}

impl Journal {
    /// While an expected pack is missing or a journal file unreadable, nothing derived is
    /// removed (§5.10 M0).
    pub fn complete(&self) -> bool {
        self.unreadable.is_empty() && self.missing.is_empty()
    }
}

/// The fold cache's sections, kept up to date as files are read and written (§3.6).
#[derive(Debug, Default)]
pub(crate) struct IndexCache {
    pub sections: HashMap<String, Section>,
    pub changed: bool,
}

/// Derived paths to write again (§5.0 B5).
#[derive(Debug, Default)]
pub(crate) struct Dirty {
    pub files: BTreeSet<DerivedFile>,
    pub everything: bool,
    /// `metadata.json` lists something that changed (a new pack, a removed file).
    pub metadata: bool,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && !self.everything && !self.metadata
    }

    /// Marks what a fold step changed.
    pub fn mark(&mut self, fold: &FoldState, kind: ProjectKind, delta: &Delta) {
        self.metadata = true;
        if delta.everything || delta.paths || kind == ProjectKind::Stories {
            self.everything = true;
            return;
        }
        for book in &delta.books {
            self.files.extend(fold.book_files(*book));
            self.files.insert(DerivedFile::Usfm(*book));
        }
        for (book, _) in &delta.chapters {
            self.files.extend(fold.book_files(*book));
        }
        if delta.pins {
            self.files.insert(DerivedFile::Resources);
            self.everything = true;
        }
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
                }
                RecordKey::DeviceProfile(_) => {
                    // Authors' names show in comments and conflicts only.
                    self.files
                        .extend(fold.derived_files().into_iter().filter(|f| {
                            matches!(
                                f,
                                DerivedFile::Comments(_)
                                    | DerivedFile::StoryComments
                                    | DerivedFile::Conflicts
                            )
                        }));
                }
                RecordKey::Comment(_) | RecordKey::Check(_) => {
                    if let Some(book) = key.book() {
                        self.files.extend(fold.book_files(book));
                    }
                }
                other => {
                    if let Some(book) = other.book() {
                        self.files.extend(fold.book_files(book));
                        self.files.insert(DerivedFile::Usfm(book));
                    }
                }
            }
        }
        self.files.insert(DerivedFile::Conflicts);
    }
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
    /// On a network drive no entry is clean by stat (§6.2).
    pub network: bool,
    pub journal: Journal,
    pub index: IndexCache,
    pub manifest: Manifest,
    pub dirty: Dirty,
    /// The newest time stamp (§6.2): `.tc/lock` at O1, `materialized.json` at each M7.
    pub stamp_ns: i64,
    pub shared: Arc<Shared>,
    /// Manifest entries were refreshed or settled since it was last written.
    pub manifest_changed: bool,
    /// Bytes O9 projected to classify a path, kept for the materialization that follows it
    /// with the same fold, so the open projects each file once. Cleared when the fold
    /// changes.
    pub prerendered: BTreeMap<String, Vec<u8>>,
    /// Derived paths another program refused the app (§5.10), by path.
    pub busy: BTreeMap<String, crate::materialize::Busy>,
    /// The md5 of each outside version of `metadata.json` already copied to `.tc/external/`
    /// this session, so a blocked write that is tried again copies it once.
    pub copied_outside: BTreeSet<String>,
}

impl Project {
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            fold: &self.fold,
            signing: &self.signing,
            kind: self.kind,
        }
    }

    /// Publishes the fold as the snapshot readers take (R5), and returns its sequence
    /// number.
    pub fn publish(&self) -> u64 {
        self.shared.publish(self.fold.clone())
    }

    pub fn notice(&self, notice: tc_journal::fold::Notice, copy: Option<String>) {
        let notice = self.shared.add_notice(notice, copy);
        self.shared.emit(StoreEvent::Notice(notice));
    }

    /// B2 to B6 of `commit_batch` (§5.0) for a planned batch: assigns sequence numbers, the
    /// time and the batch, writes one pending file durably, folds the changes, marks the
    /// derived files dirty and replies with the new tokens. Nothing is written when a change
    /// breaks a rule of §4.5.
    pub fn commit(&mut self, plan: Plan) -> Result<StoreReply, CommandError> {
        if plan.changes.is_empty() {
            return Ok(StoreReply {
                tokens: Vec::new(),
                new_conflicts: Vec::new(),
                stored: Stored::Unchanged,
                seq: self.shared.seq(),
                changes: Vec::new(),
                skipped: plan.skipped,
            });
        }
        let changes = self.build(&plan.changes)?;
        let stored = self.write_pending(&changes, Purpose::Save)?;
        self.next_seq += changes.len() as u64;
        let ids: Vec<ChangeId> = changes.iter().map(Change::id).collect();
        let keys: Vec<RecordKey> = changes.iter().map(|c| c.body().record.key()).collect();
        self.prerendered.clear();
        let delta = self.fold.add(changes.into_iter().map(FoldInput::from));
        self.dirty.mark(&self.fold, self.kind, &delta);
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
    ) -> Result<Stored, CommandError> {
        let header = PendingHeader {
            device: self.device,
            project: self.id,
            purpose,
            reconciles: None,
        };
        let lines: Vec<Verified> = changes.iter().cloned().map(Verified::from).collect();
        let encoded = encode_pending(&header, &lines)
            .map_err(|e| CommandError::Refused(Refusal::Rule(e.to_string())))?;
        self.shared
            .hooks
            .wait(crate::hooks::Point::BeforeDurablePending);
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
                number: None,
                status: tc_journal::pending::PendingStatus::Valid,
            },
        );
        Ok(stored)
    }
}
