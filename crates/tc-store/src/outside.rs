//! Edits made outside the app, found and recorded (`docs/STORAGE.md` §11.1, §6.2).
//!
//! The writer finds a derived file whose bytes are none the app wrote, keeps a byte copy of
//! them (X1), decodes them and compares them with what the heads file says the app wrote (X2,
//! X3, in `tc-journal`'s pure planner), and hands the plan to the actor, which commits it as
//! one batch that names the file and its md5 (X4). When the batch is durable the writer takes
//! the bytes as the ones the app saw there (D7) and writes the file again from the project
//! (X6): a compare-and-swap that finds bytes the batch recorded may overwrite them, and a
//! second examination of the same bytes records nothing (the crash rows X1 and X2).
//!
//! A file that is clean by stat but whose bytes differ is storage damage, not an edit (§6.2):
//! it is held, never recorded until the person takes it in.

use crate::error::StoreError;
use crate::handle::Msg;
use crate::layout;
use crate::manifest::{Entry, HeadsFile};
use crate::materialize::is_text_path;
use crate::writer::{Recorded, Writer};
use tc_journal::change::ExternalSource;
use tc_journal::fold::{DerivedFile, Notice};
use tc_journal::outside::{Base, BaseRecord, Decoded, OutsideFile, OutsidePlan, fields_not_shown};
use tc_journal::{ChangeId, FrameNo, Md5, Record, RecordKey, RepoPath, StoryNo};

/// The path of `conflicts.json`, which only the app writes.
const CONFLICTS: &str = "ingredients/tc/conflicts.json";

/// What examining a changed file decided.
pub(crate) enum Examined {
    /// The file shows what the project shows (or differs in line ends and blanks only):
    /// nothing is recorded, and the file is written again from the project.
    Nothing,
    /// The bytes cannot be read as the file they are the name of: a copy was kept, nothing is
    /// recorded, and the file is written again from the project.
    Undecodable { reason: String },
    /// Changes to record.
    Plan(Box<OutsidePlan>, u32),
}

/// A path found changed during the open's classification, to be recorded before the open
/// ends (§6.1 O9).
pub(crate) struct Found {
    pub rel: String,
    pub bytes: Vec<u8>,
    pub force_unknown: bool,
}

impl Writer {
    /// The heads file of `rel`, read as the base of an outside edit; unknown when there is
    /// none or it does not read (§6.2).
    fn read_base(&self, rel: &str) -> Base {
        self.counters.add(&self.counters.heads_read, 1);
        let Some(file) = self.heads_of(rel) else {
            // With no manifest entry either, `.tc/` is gone (a folder copied away): what the
            // last commit of this app holds says what the file was made from (§6.2).
            if !self.manifest.paths.contains_key(rel) {
                return self.head_base(rel);
            }
            return Base::Unknown;
        };
        let mut records = Vec::with_capacity(file.records.len());
        for record in file.records {
            let key = tc_journal::json::parse(record.key.as_bytes())
                .ok()
                .and_then(|j| tc_journal::canon::decode_key(&j).ok());
            let Some(key) = key else {
                return Base::Unknown;
            };
            records.push(BaseRecord {
                key,
                heads: record.heads.into_iter().map(ChangeId::from_bytes).collect(),
                shown: record.shown,
            });
        }
        Base::Heads(records)
    }

    /// The base of an edit of a file whose heads file and manifest entry are lost (§6.2): when
    /// HEAD was made by this app, the heads of each record among the changes of the packs HEAD
    /// holds (a change another pack holds, or only a pending file, is newer than the file the
    /// person started from, so the edit conflicts with it and replaces nothing). A file HEAD
    /// does not have was made since: every record is new. No usable HEAD: unknown.
    fn head_base(&self, rel: &str) -> Base {
        let Some(git) = &self.git else {
            return Base::Unknown;
        };
        let Ok(Some(head)) = git.head() else {
            return Base::Unknown;
        };
        if !head.app_commit {
            return Base::Unknown;
        }
        match git.read_path(head.commit, rel) {
            Ok(Some(_)) => {}
            Ok(None) => return Base::Heads(Vec::new()),
            Err(_) => return Base::Unknown,
        }
        let Ok(packs) = git.subtree_entries(head.tree, tc_journal::pack::PACKS_DIR) else {
            return Base::Unknown;
        };
        let names: std::collections::BTreeSet<[u8; 16]> = packs
            .keys()
            .filter_map(|path| tc_journal::pack::PackPath::parse(path))
            .map(|path| *path.name.as_bytes())
            .collect();
        let keys: Vec<RecordKey> = if rel == layout::METADATA {
            self.fold
                .metadata_heads()
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        } else if let Some(file) = DerivedFile::from_path(rel) {
            self.fold
                .file_heads(&file)
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        } else {
            return Base::Unknown;
        };
        let mut records = Vec::with_capacity(keys.len());
        for key in keys {
            let held: Vec<tc_journal::fold::ChangeView> = self
                .fold
                .history(&key)
                .iter()
                .filter_map(|id| self.fold.change(id))
                .filter(|v| {
                    v.packs.iter().any(|p| {
                        let mut name = [0u8; 16];
                        name.copy_from_slice(&p.as_bytes()[..16]);
                        names.contains(&name)
                    })
                })
                .collect();
            let replaced: std::collections::BTreeSet<ChangeId> = held
                .iter()
                .flat_map(|v| v.parents.iter().copied())
                .collect();
            let heads: Vec<ChangeId> = held
                .iter()
                .map(|v| v.id)
                .filter(|id| !replaced.contains(id))
                .collect();
            records.push(BaseRecord {
                key,
                heads,
                shown: None,
            });
        }
        Base::Heads(records)
    }

    /// Decodes a story file: the records its parts stand for, with the parts that equal what
    /// the project shows replaced by the project's own record, so only a part someone changed
    /// is a change.
    fn decode_story(&self, story: StoryNo, text: &str) -> Result<Decoded, String> {
        let decoded = tc_obs::story::decode_story(text).map_err(|e| e.to_string())?;
        let written = self.written_story(story).and_then(|r| r.as_decoded().ok());
        let mut out = Decoded {
            story_reformatted: u32::try_from(decoded.reformatted.len()).unwrap_or(u32::MAX),
            ..Decoded::default()
        };
        let records = decoded.records;
        let mut parts: Vec<(RecordKey, Record, bool)> = Vec::new();
        let same_base = written.as_ref().is_some_and(|w| w.base == records.base);
        parts.push((
            RecordKey::StoryBase(story),
            Record::StoryBase {
                story,
                value: records.base.clone(),
            },
            same_base,
        ));
        if let Some(title) = &records.title {
            let same = written.as_ref().and_then(|w| w.title.as_ref()) == Some(title);
            parts.push((
                RecordKey::Frame(story, FrameNo(0)),
                Record::Frame {
                    story,
                    frame: FrameNo(0),
                    value: title.clone(),
                },
                same,
            ));
        }
        for (n, text) in (1u16..).zip(&records.frames) {
            let Some(text) = text else { continue };
            let same = written
                .as_ref()
                .and_then(|w| w.frames.get(usize::from(n) - 1))
                .and_then(Option::as_ref)
                == Some(text);
            parts.push((
                RecordKey::Frame(story, FrameNo(n)),
                Record::Frame {
                    story,
                    frame: FrameNo(n),
                    value: text.clone(),
                },
                same,
            ));
        }
        if let Some(reference) = &records.reference {
            let same = written.as_ref().and_then(|w| w.reference.as_ref()) == Some(reference);
            parts.push((
                RecordKey::StoryRef(story),
                Record::StoryRef {
                    story,
                    value: Some(reference.clone()),
                },
                same,
            ));
        }
        for (key, record, same) in parts {
            if same {
                // What the project shows, if it holds a record; no record, nothing to compare.
                if let Some(shown) = self.fold.projected(&key) {
                    out.records.push(shown);
                }
            } else {
                out.records.push(record);
            }
        }
        Ok(out)
    }

    /// The records of a story as the project holds them, shaped as a story file's.
    fn written_story(&self, story: StoryNo) -> Option<tc_obs::story::StoryRecords> {
        let base = match self.fold.projected(&RecordKey::StoryBase(story))? {
            Record::StoryBase { value, .. } => value,
            _ => return None,
        };
        let count = tc_obs::story::decode_story(&base)
            .ok()?
            .records
            .frames
            .len();
        let frame = |n: u16| match self.fold.projected(&RecordKey::Frame(story, FrameNo(n))) {
            Some(Record::Frame { value, .. }) => Some(value),
            _ => None,
        };
        let reference = match self.fold.projected(&RecordKey::StoryRef(story)) {
            Some(Record::StoryRef { value, .. }) => value,
            _ => None,
        };
        Some(tc_obs::story::StoryRecords {
            story: tc_obs::ids::StoryNo::new(story.get())?,
            base,
            title: frame(0),
            frames: (1..=u16::try_from(count).unwrap_or(u16::MAX))
                .map(frame)
                .collect(),
            reference,
        })
    }

    /// Decodes `metadata.json`: the fields the records hold, the ones the app derives that
    /// the file shows changed, and the defaults the builder fills in dropped.
    fn decode_metadata_edit(&mut self, bytes: &[u8]) -> Result<Decoded, String> {
        let found = tc_journal::fold::decode_metadata(bytes).map_err(|e| e.to_string())?;
        let expected = self
            .build_metadata(&std::collections::BTreeMap::new(), &[])
            .map_err(|e| e.to_string())?;
        let shown = tc_journal::fold::decode_metadata(&expected).map_err(|e| e.to_string())?;
        let records = |fields: Vec<(tc_journal::MetaField, tc_journal::Json)>| -> Vec<Record> {
            fields
                .into_iter()
                .map(|(field, value)| Record::ProjectMeta {
                    field,
                    value: Some(value),
                })
                .collect()
        };
        let edited = found
            .derived
            .iter()
            .filter(|(path, value)| shown.derived.get(*path) != Some(*value))
            .count();
        Ok(Decoded {
            records: fields_not_shown(records(found.fields), &records(shown.fields), |key| {
                self.fold.projected(key).is_some()
            }),
            derived_fields: u32::try_from(edited).unwrap_or(u32::MAX),
            ..Decoded::default()
        })
    }

    /// X1 to X3 for a changed file: the byte copy, the decoding and the comparison with what
    /// the app wrote. `force_unknown`: record every change without parents (the person took
    /// the file in, or it changed again while held, §6.2). Returns the copy's path too.
    pub(crate) fn examine(
        &mut self,
        rel: &str,
        bytes: &[u8],
        force_unknown: bool,
    ) -> Result<(Examined, String), StoreError> {
        let copy = self.copy_external(rel, bytes)?;
        let examined = self.examine_copied(rel, bytes, force_unknown);
        Ok((examined, copy))
    }

    /// [`Writer::examine`] for bytes whose copy is kept already.
    pub(crate) fn examine_copied(
        &mut self,
        rel: &str,
        bytes: &[u8],
        force_unknown: bool,
    ) -> Examined {
        let undecodable = |reason: &str| Examined::Undecodable {
            reason: reason.to_owned(),
        };
        let Ok(path) = RepoPath::parse_source(rel) else {
            return undecodable("its name is not a name translationCore can record");
        };
        let seen = Md5::of(bytes);
        let outside = if rel == layout::METADATA {
            OutsideFile::Metadata
        } else if let Some(file) = DerivedFile::from_path(rel) {
            OutsideFile::Derived(file)
        } else {
            return undecodable("it is no file translationCore derives");
        };
        let decoded = match &outside {
            OutsideFile::Metadata => self.decode_metadata_edit(bytes),
            OutsideFile::Derived(DerivedFile::Story(story)) => match std::str::from_utf8(bytes) {
                Ok(text) => self.decode_story(*story, text),
                Err(_) => Err("it is not UTF-8 text".into()),
            },
            OutsideFile::Derived(_) => tc_journal::outside::decode_file(&outside, bytes, &path),
        };
        let mut decoded = match decoded {
            Ok(decoded) => decoded,
            Err(reason) => return Examined::Undecodable { reason },
        };
        let base = if force_unknown {
            Base::Unknown
        } else {
            self.read_base(rel)
        };
        self.counters.add(&self.counters.outside_decodes, 1);
        let mut plan = self.fold.plan_outside_edit(
            &outside,
            &decoded,
            &base,
            ExternalSource::LocalFile {
                path,
                seen_md5: seen,
            },
            force_unknown,
        );
        self.counters.add(
            &self.counters.outside_keys_examined,
            u64::from(plan.keys_examined),
        );
        if plan.changes.is_empty() {
            // The digests of a book's records compare after line ends and blanks at line ends
            // are normalized, so a file that differs from the app's bytes in nothing else has
            // no key looked at and none counted: it is one rewrite of the whole file (X3).
            if plan.whitespace_only == 0
                && plan.keys_examined == 0
                && matches!(outside, OutsideFile::Derived(DerivedFile::Usfm(_)))
            {
                plan.whitespace_only = u32::try_from(decoded.digests.len()).unwrap_or(u32::MAX);
            }
            self.notices_of_plan(rel, &plan, &decoded);
            return Examined::Nothing;
        }
        self.shown_by_path
            .insert(rel.to_owned(), std::mem::take(&mut decoded.digests));
        Examined::Plan(Box::new(plan), decoded.derived_fields)
    }

    /// The notices an examination that records nothing calls for.
    fn notices_of_plan(&self, rel: &str, plan: &OutsidePlan, decoded: &Decoded) {
        let Ok(path) = RepoPath::parse_source(rel) else {
            return;
        };
        if plan.whitespace_only > 0 || decoded.story_reformatted > 0 {
            self.notice(
                Notice::WhitespaceOnlyEdit {
                    path: path.clone(),
                    keys: plan.whitespace_only.max(decoded.story_reformatted),
                },
                None,
            );
        }
        self.extra_notices(&path, plan, decoded.derived_fields);
    }

    /// Removals a file cannot make, flags it cannot set and derived fields it cannot change,
    /// told in words.
    fn extra_notices(&self, path: &RepoPath, plan: &OutsidePlan, derived_fields: u32) {
        if !plan.kept_removals.is_empty() {
            self.notice(
                Notice::OutsideRemovalKept {
                    path: path.clone(),
                    records: u32::try_from(plan.kept_removals.len()).unwrap_or(u32::MAX),
                },
                None,
            );
        }
        if derived_fields > 0 {
            self.notice(
                Notice::DerivedFieldsEdited {
                    path: path.clone(),
                    fields: derived_fields,
                },
                None,
            );
        }
    }

    /// Takes the bytes found at `rel` as the ones the app saw there (D7): its entry has their
    /// md5 and no scope, no hold and no blob, so the next materialization compares them as
    /// the app's own and writes the file again from the project (X6).
    pub(crate) fn accept(&mut self, rel: &str, md5: Md5, bytes: &[u8]) -> Result<(), StoreError> {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let entry = self
            .manifest
            .paths
            .entry(rel.to_owned())
            .or_insert_with(|| Entry::of(String::new(), 0, 0, false));
        entry.md5 = md5.to_hex();
        entry.size = meta.map_or(bytes.len() as u64, |m| m.len);
        entry.mtime_ns = meta.map_or(0, |m| m.mtime_ns);
        entry.settled = false;
        entry.scope = None;
        entry.held = None;
        entry.blob = None;
        entry.nfc = is_text_path(rel)
            .then(|| unicode_normalization::is_nfc(&String::from_utf8_lossy(bytes)));
        self.manifest_changed = true;
        self.shared.remove_notices_of(rel);
        if let Some(file) = DerivedFile::from_path(rel) {
            self.dirty.add_file(file);
        } else if rel == layout::METADATA {
            self.dirty.metadata = true;
        }
        self.write_manifest()
    }

    /// A derived file whose bytes are none the app wrote and that is not clean by stat (§6.2
    /// table): examined, and either recorded through the actor or accepted at once.
    /// While the open classifies (`Writer::collecting`), the path is only noted: the open
    /// records it before it ends.
    pub(crate) fn found_outside(
        &mut self,
        rel: &str,
        bytes: &[u8],
        force_unknown: bool,
    ) -> Result<(), StoreError> {
        if rel == CONFLICTS {
            return self.restore_fixed(rel, bytes);
        }
        if DerivedFile::from_path(rel).is_none() && rel != layout::METADATA {
            // A carried file is no record of the project; its edits wait for the person (§11.4).
            return self.hold(rel, bytes, false);
        }
        if self.collecting {
            self.found.push(Found {
                rel: rel.to_owned(),
                bytes: bytes.to_vec(),
                force_unknown,
            });
            return Ok(());
        }
        let md5 = Md5::of(bytes);
        let (examined, copy) = self.examine(rel, bytes, force_unknown)?;
        match examined {
            Examined::Plan(plan, derived) => {
                self.shared
                    .hooks
                    .wait(crate::hooks::Point::BeforeRecordOutside);
                self.pending_record
                    .insert(rel.to_owned(), (md5, bytes.to_vec()));
                let sent = self.actor.send(Msg::RecordOutside {
                    rel: rel.to_owned(),
                    seen: md5,
                    plan,
                    derived,
                    copy,
                });
                if sent.is_err() {
                    self.pending_record.remove(rel);
                }
                Ok(())
            }
            Examined::Nothing => self.accept(rel, md5, bytes),
            Examined::Undecodable { reason } => {
                self.undecodable(rel, &copy, &reason);
                self.accept(rel, md5, bytes)
            }
        }
    }

    /// [`Writer::found_outside`] for a held file the person takes in (§6.2): no base, and the
    /// asker is answered when the batch is durable, or at once when there is nothing to
    /// record.
    pub(crate) fn found_outside_answering(
        &mut self,
        rel: &str,
        bytes: &[u8],
        reply: crate::handle::Resolver<Result<crate::reply::TakenIn, StoreError>>,
    ) -> Result<(), StoreError> {
        let md5 = Md5::of(bytes);
        let (examined, copy) = self.examine(rel, bytes, true)?;
        match examined {
            Examined::Plan(plan, derived) => {
                self.pending_record
                    .insert(rel.to_owned(), (md5, bytes.to_vec()));
                self.waiting.insert(rel.to_owned(), reply);
                let sent = self.actor.send(Msg::RecordOutside {
                    rel: rel.to_owned(),
                    seen: md5,
                    plan,
                    derived,
                    copy,
                });
                if sent.is_err() {
                    self.pending_record.remove(rel);
                    if let Some(reply) = self.waiting.remove(rel) {
                        reply.resolve(Err(StoreError::Other("the project is closed".into())));
                    }
                }
                Ok(())
            }
            Examined::Nothing => {
                self.accept(rel, md5, bytes)?;
                reply.resolve(Ok(crate::reply::TakenIn::default()));
                self.finish_looking()
            }
            Examined::Undecodable { reason } => {
                self.undecodable(rel, &copy, &reason);
                self.accept(rel, md5, bytes)?;
                reply.resolve(Ok(crate::reply::TakenIn::default()));
                self.finish_looking()
            }
        }
    }

    /// The person is told a file could not be read, where the copy of it is, and that the
    /// project shows the text from before.
    pub(crate) fn undecodable(&self, rel: &str, copy: &str, reason: &str) {
        if let Ok(path) = RepoPath::parse_source(rel) {
            self.notice(
                Notice::OutsideEditUnreadable {
                    path,
                    copy: copy.into(),
                    reason: reason.into(),
                },
                Some(copy.to_owned()),
            );
        }
    }

    /// `project.json`, `format.json` and `conflicts.json` changed outside: never recorded. A
    /// copy is kept, the file is written again, and the person is told (§3.5). `project.json`
    /// and `format.json` come back from HEAD's tree, else from what this app writes;
    /// `conflicts.json` is derived, so the next materialization writes it from the project.
    pub(crate) fn restore_fixed(&mut self, rel: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let copy = self.copy_external(rel, bytes)?;
        if let Ok(path) = RepoPath::parse_source(rel) {
            self.notice(
                Notice::FixedFileRestored {
                    path,
                    copy: copy.as_str().into(),
                },
                Some(copy),
            );
        }
        if rel == CONFLICTS {
            return self.accept(rel, Md5::of(bytes), bytes);
        }
        let good = self.fixed_bytes(rel);
        let (dir, name) = self.folder.dir_and_name(rel);
        crate::atomic::atomic_write(&*self.vfs, &dir, name, &good, crate::atomic::SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        self.manifest.paths.insert(
            rel.to_owned(),
            Entry::of(
                Md5::of(&good).to_hex(),
                good.len() as u64,
                meta.map_or(0, |m| m.mtime_ns),
                false,
            ),
        );
        self.manifest_changed = true;
        Ok(())
    }

    /// What a fixed file holds: HEAD's copy when it reads, else what this app would write.
    fn fixed_bytes(&self, rel: &str) -> Vec<u8> {
        let from_head = self.git.as_ref().and_then(|git| {
            let head = git.head().ok()??;
            git.read_path(head.commit, rel).ok()?
        });
        let reads = |bytes: &[u8]| match rel {
            layout::PROJECT_JSON => crate::open::ProjectJson::reads(bytes),
            _ => tc_journal::format::FormatDoc::parse(bytes).is_ok(),
        };
        if let Some(bytes) = from_head.filter(|b| reads(b)) {
            return bytes;
        }
        match rel {
            layout::PROJECT_JSON => {
                let facts = self.shared.facts();
                crate::open::ProjectJson::new(facts.id, facts.kind, self.clock.now()).to_bytes()
            }
            _ => tc_journal::format::FormatDoc::current().to_bytes(),
        }
    }

    /// X2 for `metadata.json`: the fields of an edit that are records are sent to the actor
    /// to be recorded. The file is written over at once, so nothing is accepted for it.
    pub(crate) fn record_metadata_edit(&mut self, bytes: &[u8], copy: String) {
        let md5 = Md5::of(bytes);
        match self.examine_copied(layout::METADATA, bytes, false) {
            Examined::Plan(plan, derived) => {
                let _ = self.actor.send(Msg::RecordOutside {
                    rel: layout::METADATA.to_owned(),
                    seen: md5,
                    plan,
                    derived,
                    copy,
                });
            }
            Examined::Nothing => {}
            Examined::Undecodable { reason } => self.undecodable(layout::METADATA, &copy, &reason),
        }
    }

    /// D7: the heads file of `rel` after an outside edit was recorded. The bytes in the folder
    /// are the person's, and may stay there for as long as a program keeps the file open
    /// (Paratext on Windows) while that program saves them again; its next save starts from
    /// these bytes, so the base it is compared with names the changes recorded for them, not
    /// the version the app wrote. A record the batch left alone keeps the base it had; the
    /// digests are those of the bytes found. Best effort: a heads file that cannot be written
    /// leaves the next edit to conflict, never to replace.
    fn write_edited_heads(
        &mut self,
        rel: &str,
        changed: &[(RecordKey, ChangeId)],
        shown: Vec<(RecordKey, tc_journal::outside::ShownDigest)>,
    ) {
        use crate::manifest::HeadsRecord;
        let Some(file) = DerivedFile::from_path(rel) else {
            return;
        };
        let outside = OutsideFile::Derived(file.clone());
        let key_text = |key: &RecordKey| {
            tc_journal::canon::encode_key(key)
                .map(|j| j.to_canonical_string())
                .unwrap_or_default()
        };
        let mut records: std::collections::BTreeMap<String, HeadsRecord> = self
            .heads_of(rel)
            .unwrap_or_else(|| HeadsFile::of(&self.fold.file_heads(&file), &[]))
            .records
            .into_iter()
            .map(|r| (r.key.clone(), r))
            .collect();
        for (key, id) in changed.iter().filter(|(key, _)| outside.shows(key)) {
            let text = key_text(key);
            records
                .entry(text.clone())
                .or_insert_with(|| HeadsRecord {
                    key: text,
                    heads: Vec::new(),
                    shown: None,
                })
                .heads = vec![*id.as_bytes()];
        }
        for (key, digest) in shown {
            let text = key_text(&key);
            records
                .entry(text.clone())
                .or_insert_with(|| HeadsRecord {
                    key: text,
                    heads: Vec::new(),
                    shown: None,
                })
                .shown = Some(digest);
        }
        let heads = HeadsFile {
            records: records.into_values().collect(),
        };
        let dir = self.folder.path(layout::HEADS);
        if crate::atomic::atomic_create_dirs(&*self.vfs, &dir).is_ok() {
            let _ = self.put_heads(rel, heads, crate::atomic::SyncDir::Now);
        } else {
            self.heads_unwritten.insert(rel.to_owned(), heads);
        }
    }

    /// The writer's handling of what the actor answered to a plan (D7). `bytes` are the ones
    /// the plan was made of.
    pub(crate) fn recorded(
        &mut self,
        rel: String,
        seen: Md5,
        copy: String,
        outcome: Recorded,
        bytes: &[u8],
    ) {
        self.pending_record.remove(&rel);
        let carried = self.pending_carried.remove(&rel);
        let shown = self.shown_by_path.remove(&rel).unwrap_or_default();
        self.answer_taken_in(&rel, &outcome);
        match outcome {
            Recorded::Done {
                batch,
                changes,
                unknown_base,
                whitespace_only,
                kept,
                derived,
                heads,
            } => {
                if carried.is_none() {
                    self.write_edited_heads(&rel, &heads, shown);
                }
                if let Ok(path) = RepoPath::parse_source(&rel) {
                    if let Some(batch) = batch {
                        self.notice(
                            Notice::OutsideEditRecorded {
                                path: path.clone(),
                                changes,
                                batch,
                                unknown_base,
                            },
                            // The notice of `metadata.json` written over already keeps the
                            // copy, and a copy goes with one notice only.
                            (rel != layout::METADATA && !copy.is_empty()).then_some(copy),
                        );
                    }
                    if whitespace_only > 0 {
                        self.notice(
                            Notice::WhitespaceOnlyEdit {
                                path: path.clone(),
                                keys: whitespace_only,
                            },
                            None,
                        );
                    }
                    if kept > 0 {
                        self.notice(
                            Notice::OutsideRemovalKept {
                                path: path.clone(),
                                records: kept,
                            },
                            None,
                        );
                    }
                    if derived > 0 {
                        self.notice(
                            Notice::DerivedFieldsEdited {
                                path,
                                fields: derived,
                            },
                            None,
                        );
                    }
                }
            }
            Recorded::Refused(reason) => self.undecodable(&rel, &copy, &reason),
            Recorded::NotWritten => {
                // The disk refused the batch: nothing is recorded and nothing is accepted;
                // the copy is kept, and the path stays dirty, so the writer's own retry
                // (200 ms to 3 s) reads it and examines it again.
                self.dirty.metadata = true;
                self.dirty.carried |= carried.is_some();
                if let Some(file) = DerivedFile::from_path(&rel) {
                    self.dirty.add_file(file);
                }
                return;
            }
        }
        if rel == layout::METADATA {
            // The file was written over; its records are written again now.
            self.dirty.metadata = true;
            return;
        }
        if let Some((oid, size)) = carried {
            // A carried file's bytes are its blob; the entry names both (§11.4).
            self.accept_carried(&rel, seen, size, oid);
            return;
        }
        let _ = self.accept(&rel, seen, bytes);
    }
}
