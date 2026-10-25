//! A book file or story another program changed (owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.3, §2.4, §2.11): its bytes are copied to
//! `.tc/external/`, the file is held as it is, and the book waits for the person's answer.
//! Nothing in the copy enters the project until the person chooses *Import*; *Not now* writes
//! the file again from the project, and the copy stays listed with *Import* and *Dismiss*
//! until it is imported or dismissed.
//!
//! The order of the writes is what makes a crash lose nothing (delta §2.6 X1, X2): the copy
//! and its folder are flushed, then one manifest write names the hold and the `kept` entry
//! together. Before that write the file is still changed and is found again; after it, the
//! book waits and is asked about at the next open.

use crate::atomic::{HeldRetry, Removed, SyncDir, atomic_remove_checked};
use crate::classify::Next;
use crate::error::StoreError;
use crate::handle::Resolver;
use crate::layout::EXTERNAL;
use crate::ledger::{self, Event, HeldCause, Stat, State};
use crate::materialize::is_text_path;
use crate::outside::Examined;
use crate::vfs::{EntryKind, is_not_found};
use crate::writer::Writer;
use serde::{Deserialize, Serialize};
use tc_journal::fold::{DerivedFile, Notice};
use tc_journal::outside::{Base as PlanBase, OutsideFile, OutsidePlan};
use tc_journal::{BookId, Md5, RepoPath};

/// After the name of a copy in `.tc/external/`: the empty file that says an *Import* took
/// the copy in.
pub(crate) const IMPORTED: &str = ".imported";

/// After the name of a copy in `.tc/external/`: the empty file that says the copy holds work
/// the project's records lost (`why: lost`), so an open that lost the manifest lists it as
/// that again.
const LOST: &str = ".lost";

/// What a kept copy was compared with: the version of the project the outside file was
/// written from (delta §2.3). Written as `placed:<md5>`, `head:<commit>`, `head` or `none`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum KeptBase {
    /// The placement the change was found against; its heads are in the path's heads file
    /// for as long as the entry lives.
    Placed(String),
    /// What HEAD's packs held (`.tc/` was lost, and HEAD is this app's), as of the commit
    /// that was HEAD when the file was found: HEAD moves with the person's later saves. With
    /// no commit (an earlier build's entry) *Import* compares with no base.
    Head(Option<tc_journal::GitOid>),
    /// No base: every difference with text on both sides is one to review.
    None,
}

impl From<KeptBase> for String {
    fn from(base: KeptBase) -> Self {
        match base {
            KeptBase::Placed(md5) => format!("placed:{md5}"),
            KeptBase::Head(Some(commit)) => format!("head:{}", commit.to_hex()),
            KeptBase::Head(None) => "head".into(),
            KeptBase::None => "none".into(),
        }
    }
}

impl TryFrom<String> for KeptBase {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        match text.as_str() {
            "head" => Ok(Self::Head(None)),
            "none" => Ok(Self::None),
            _ => {
                if let Some(md5) = text.strip_prefix("placed:")
                    && Md5::parse(md5).is_some()
                {
                    return Ok(Self::Placed(md5.to_owned()));
                }
                match text
                    .strip_prefix("head:")
                    .and_then(tc_journal::GitOid::parse)
                {
                    Some(commit) => Ok(Self::Head(Some(commit))),
                    None => Err(format!("not a kept base: {text}")),
                }
            }
        }
    }
}

impl KeptBase {
    fn of(base: &ledger::Base, head: Option<tc_journal::GitOid>) -> Self {
        match base {
            ledger::Base::Placed(md5) => Self::Placed(md5.clone()),
            ledger::Base::Head => Self::Head(head),
            ledger::Base::Unknown => Self::None,
        }
    }

    fn ledger(&self) -> ledger::Base {
        match self {
            Self::Placed(md5) => ledger::Base::Placed(md5.clone()),
            Self::Head(_) => ledger::Base::Head,
            Self::None => ledger::Base::Unknown,
        }
    }
}

/// Why a copy was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeptWhy {
    /// Another program changed the file.
    Changed,
    /// The app's own bytes show work the journal lost (row N4, H22 part 1): *Bring it back*
    /// (an *Import* of the copy), and no *Dismiss*: the copy may be the work's only one.
    Lost,
}

/// One kept copy that still has a notice (`.tc/materialized.json` `kept`, delta §2.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    /// The project path the bytes were found at.
    pub path: String,
    /// The copy in `.tc/external/`, relative to the project folder.
    pub copy: String,
    pub md5: String,
    pub size: u64,
    pub base: KeptBase,
    pub why: KeptWhy,
    /// Why the bytes cannot be read as the file they are the name of (a verse number twice):
    /// the question offers only *Not now* (delta §2.11 Q2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    /// A `why: lost` entry the person answered (*Not now*): it is not asked again, and stays
    /// listed with *Bring it back*. An entry of an earlier build has none, so it is asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answered: bool,
}

/// A book file or story that waits for the person's answer: held as it is, with its newest
/// kept copy, the one *Import* takes; or a `why: lost` copy not answered yet, which the
/// question offers to bring back (H22 part 1, `kept.why`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingBook {
    pub path: String,
    pub file: DerivedFile,
    /// The project has no such book or story: *Import* adds it (delta §2.11).
    pub added: bool,
    pub kept: Kept,
}

/// What an *Import* of a kept copy did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Records that came in with no question.
    pub imported: u32,
    /// Records that differ on both sides: conflicts to review.
    pub to_review: u32,
    /// Verses the project keeps: empty in the file, or not in it.
    pub left_as_it_was: u32,
    /// The file added its book to the project.
    pub book_added: bool,
    /// The batch of the changes to review: the conflict review's one row for this import,
    /// with *Accept all from the file* (`Command::ResolveOutsideEdit`, §8.1).
    pub review: Option<tc_journal::BatchId>,
}

/// The lists the handle reads, as the manifest on disk last named them.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeptView {
    pub waiting: Vec<WaitingBook>,
    pub kept: Vec<Kept>,
}

/// What the actor imports.
pub(crate) enum ImportWhat {
    /// "Update a book from USFM" on the copy's bytes, with the copy's base (§13.6).
    Book {
        book: BookId,
        file_name: String,
        base: PlanBase,
    },
    /// A story: the outside-edit planner's plan of the copy against its base.
    Story { plan: Box<OutsidePlan> },
}

/// A book file or story: the files the person is asked about.
pub(crate) fn asks(rel: &str) -> bool {
    matches!(
        DerivedFile::from_path(rel),
        Some(DerivedFile::Usfm(_) | DerivedFile::Story(_))
    )
}

/// The book file or story a copy in `.tc/external/` was made of, from the name
/// `copy_external` gives it: `<utc stamp>-<escaped path>`, with `-2`, `-3`, … after a second
/// copy in the same second.
fn source_of_copy(name: &str) -> Option<String> {
    let (_stamp, escaped) = name.split_once('-')?;
    let unescaped = |s: &str| s.replace("%2F", "/").replace("%25", "%");
    let whole = unescaped(escaped);
    if asks(&whole) {
        return Some(whole);
    }
    let (stem, n) = escaped.rsplit_once('-')?;
    let rel = unescaped(stem);
    (n.parse::<u16>().is_ok() && asks(&rel)).then_some(rel)
}

impl Writer {
    /// Every live entry, and one for a held book file or story a manifest of an earlier build
    /// held with no entry (no base).
    fn kept_entries(&self) -> Vec<Kept> {
        let mut out = self.manifest.kept.clone();
        for (rel, entry) in &self.manifest.paths {
            let Some(held) = entry.held() else { continue };
            if asks(rel) && !out.iter().any(|k| k.path == *rel && k.md5 == held.md5) {
                out.push(Kept {
                    path: rel.clone(),
                    copy: held.copy.clone(),
                    md5: held.md5.clone(),
                    size: held.size,
                    base: KeptBase::None,
                    why: KeptWhy::Changed,
                    unreadable: None,
                    answered: false,
                });
            }
        }
        out
    }

    /// At an open that found no manifest: every copy in `.tc/external/` of a book file or
    /// story that no entry names is listed again with no base, since its entry may have been
    /// in the lost manifest. A copy an *Import* took in has a mark beside it (its name and
    /// [`IMPORTED`]), so it is not one of them. An open that has the manifest never calls
    /// this, so a copy whose entry ended by *Import* or *Dismiss* stays unlisted.
    pub(crate) fn relist_unnamed_copies(&mut self) -> Result<(), StoreError> {
        let dir = self.folder.path(EXTERNAL);
        let mut names: Vec<String> = match self.vfs.read_dir(&dir) {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.kind == EntryKind::File)
                .filter_map(|e| e.name.into_string().ok())
                .collect(),
            Err(e) if is_not_found(&e) => return Ok(()),
            Err(e) => return Err(StoreError::io("list", &dir, e)),
        };
        names.sort();
        let marked: std::collections::BTreeSet<String> = names
            .iter()
            .filter(|n| n.ends_with(IMPORTED) || n.ends_with(LOST))
            .cloned()
            .collect();
        let named = self.kept_entries();
        for name in names {
            let Some(rel) = source_of_copy(&name) else {
                continue;
            };
            if marked.contains(&format!("{name}{IMPORTED}")) {
                continue;
            }
            let copy = format!("{EXTERNAL}/{name}");
            let path = self.folder.path(&copy);
            let bytes = self
                .vfs
                .read(&path)
                .map_err(|e| StoreError::io("read the kept copy", &path, e))?;
            let md5 = Md5::of(&bytes).to_hex();
            // A copy of bytes already listed for its file (a hold the open made again).
            if named
                .iter()
                .chain(&self.manifest.kept)
                .any(|k| k.copy == copy || (k.path == rel && k.md5 == md5))
            {
                continue;
            }
            // Lost work keeps its kind and its pinned base, the placement it was found as,
            // whose heads stay in the heads file while the entry lives.
            let (why, base) = if marked.contains(&format!("{name}{LOST}")) {
                (KeptWhy::Lost, KeptBase::Placed(md5.clone()))
            } else {
                (KeptWhy::Changed, KeptBase::None)
            };
            self.manifest.kept.push(Kept {
                path: rel,
                copy,
                md5,
                size: bytes.len() as u64,
                base,
                why,
                unreadable: None,
                answered: false,
            });
            self.manifest_changed = true;
        }
        Ok(())
    }

    /// Whether `.tc/external/` holds a copy of `rel` with the md5 `md5`.
    fn has_copy_of(&self, rel: &str, md5: &str) -> Result<bool, StoreError> {
        let dir = self.folder.path(EXTERNAL);
        let entries = match self.vfs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if is_not_found(&e) => return Ok(false),
            Err(e) => return Err(StoreError::io("list", &dir, e)),
        };
        for entry in entries {
            let Ok(name) = entry.name.into_string() else {
                continue;
            };
            if entry.kind != EntryKind::File || source_of_copy(&name).as_deref() != Some(rel) {
                continue;
            }
            let path = dir.join(&name);
            let bytes = self
                .vfs
                .read(&path)
                .map_err(|e| StoreError::io("read the kept copy", &path, e))?;
            if Md5::of(&bytes).to_hex() == md5 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The bases the live entries of `rel` name: their heads stay in the path's heads file.
    pub(crate) fn kept_bases(&self, rel: &str) -> Vec<String> {
        self.manifest
            .kept
            .iter()
            .filter(|k| k.path == rel)
            .filter_map(|k| match &k.base {
                KeptBase::Placed(md5) => Some(md5.clone()),
                KeptBase::Head(_) | KeptBase::None => None,
            })
            .collect()
    }

    /// Hands the handle the lists as the manifest names them.
    pub(crate) fn publish_kept(&self) {
        let kept = self.kept_entries();
        let waiting = if self.read_only().is_some() {
            // A read-only open asks nothing (delta §2.11).
            Vec::new()
        } else {
            self.manifest
                .paths
                .iter()
                .filter_map(|(rel, entry)| {
                    let held = entry.held()?;
                    let file = DerivedFile::from_path(rel).filter(|_| asks(rel))?;
                    let newest = kept
                        .iter()
                        .rev()
                        .find(|k| k.path == *rel && k.md5 == held.md5)?
                        .clone();
                    Some(WaitingBook {
                        path: rel.clone(),
                        added: !self.fold.projects_file(&file),
                        file,
                        kept: newest,
                    })
                })
                .chain(
                    kept.iter()
                        .filter(|k| k.why == KeptWhy::Lost && !k.answered)
                        .filter_map(|k| {
                            Some(WaitingBook {
                                path: k.path.clone(),
                                file: DerivedFile::from_path(&k.path)?,
                                added: false,
                                kept: k.clone(),
                            })
                        }),
                )
                .collect()
        };
        self.shared.set_kept(KeptView { waiting, kept });
    }

    /// *Keep and hold* (delta §2.4) for bytes of a book file or story the app did not write,
    /// found against `base`: the copy, then the hold and its `kept` entry in one manifest
    /// write. Bytes that differ from the project in line ends and blanks at line ends only ask
    /// nothing: the file is written again (today's whitespace rule, H22).
    pub(crate) fn keep_and_hold(
        &mut self,
        rel: &str,
        bytes: &[u8],
        base: ledger::Base,
    ) -> Result<Next, StoreError> {
        // A second save of a held file keeps the base of the copy that waits (§2.11).
        let base = match self.state_of(rel) {
            State::Held { .. } => self
                .manifest
                .kept
                .iter()
                .rev()
                .find(|k| k.path == rel)
                .map_or(KeptBase::None, |k| k.base.clone()),
            // These bytes were kept before (an earlier hold whose manifest was lost): HEAD may
            // have moved since with the person's saves, so it is no base for them.
            _ if base == ledger::Base::Head
                && self.has_copy_of(rel, &Md5::of(bytes).to_hex())? =>
            {
                KeptBase::None
            }
            _ => KeptBase::of(
                &base,
                self.git
                    .as_ref()
                    .and_then(|git| git.head().ok().flatten())
                    .map(|head| head.commit),
            ),
        };
        // A second save is compared with the project, not with the waiting copy's base: bytes
        // equal to that older base set back what the person saved since (a revert), and only
        // a difference of line ends from the project asks nothing.
        let compared = match self.state_of(rel) {
            State::Held { .. } => ledger::Base::Unknown,
            _ => base.ledger(),
        };
        let copy = self.copy_external(rel, bytes)?;
        let md5 = Md5::of(bytes);
        let unreadable = match self.examine_copied(rel, bytes, &compared) {
            Examined::Nothing => {
                self.forget_examined(rel);
                self.accept(rel, md5, bytes, None)?;
                return Ok(Next::Recorded);
            }
            Examined::Undecodable { reason } => Some(reason),
            Examined::Plan(..) => None,
        };
        self.forget_examined(rel);
        let stat = self
            .vfs
            .metadata(&self.folder.path(rel))
            .ok()
            .map(|m| Stat {
                size: m.len,
                mtime_ns: m.mtime_ns,
            });
        self.note(
            rel,
            Event::Kept {
                md5: md5.to_hex(),
                cause: HeldCause::Edit,
                copy: copy.clone(),
                stat,
            },
        );
        if let Some(entry) = self.manifest.paths.get_mut(rel) {
            entry.nfc = is_text_path(rel)
                .then(|| unicode_normalization::is_nfc(&String::from_utf8_lossy(bytes)));
        }
        // The same bytes found again (a crash stopped the write that ended their hold) are
        // the entry that lists them already, with its base.
        if !self
            .manifest
            .kept
            .iter()
            .any(|k| k.path == rel && k.md5 == md5.to_hex())
        {
            self.manifest.kept.push(Kept {
                path: rel.to_owned(),
                copy: copy.clone(),
                md5: md5.to_hex(),
                size: bytes.len() as u64,
                base,
                why: KeptWhy::Changed,
                unreadable,
                answered: false,
            });
        }
        self.manifest_changed = true;
        self.write_manifest()?;
        // `metadata.json` lists the held bytes as they are in the folder (M6).
        self.dirty.metadata = true;
        self.shared.remove_notices_of(rel);
        self.hold_notice(rel, HeldCause::Edit, copy);
        Ok(Next::Held)
    }

    /// *Keep and write, lost* (delta §2.4, row N4) for a book file or story the app wrote that
    /// shows work the journal lost: the copy, then its `kept` entry with `why: lost` and the
    /// R4 base in one manifest write; then the file is written from the project. Nothing comes
    /// back until the person chooses *Bring it back* (H22 part 1).
    pub(crate) fn keep_lost(
        &mut self,
        rel: &str,
        bytes: &[u8],
        base: &ledger::Base,
    ) -> Result<Next, StoreError> {
        let copy = self.copy_external(rel, bytes)?;
        // The mark is what keeps the copy undismissable after a manifest loss, so a failed one
        // stops here, before the entry: the open finds the lost save again.
        self.mark(&copy, LOST)?;
        let md5 = Md5::of(bytes).to_hex();
        let same = self
            .manifest
            .kept
            .iter()
            .position(|k| k.path == rel && k.md5 == md5);
        if let Some(i) = same {
            // Another program's bytes equal to the lost save: it is the translator's work, so
            // the entry turns lost and can no longer be dismissed.
            let entry = &mut self.manifest.kept[i];
            if entry.why != KeptWhy::Lost {
                entry.why = KeptWhy::Lost;
                let copy = entry.copy.clone();
                self.mark(&copy, LOST)?;
            }
        } else {
            self.manifest.kept.push(Kept {
                path: rel.to_owned(),
                copy,
                md5: md5.clone(),
                size: bytes.len() as u64,
                base: KeptBase::of(base, None),
                why: KeptWhy::Lost,
                unreadable: None,
                answered: false,
            });
        }
        self.manifest_changed = true;
        self.write_manifest()?;
        self.pin_lost_base(rel, &md5)?;
        Ok(Next::Write)
    }

    /// The base of a `why: lost` entry, fixed now: a record every head of which in the
    /// placement is a change the journal lost stands on the heads the fold holds for it at this
    /// moment, when an earlier placement of the file named those same heads (what the lost save
    /// was made over), with no digest so it is compared. *Bring it back* then restores the save
    /// while the project still holds those heads, and a save made since makes it a conflict. Written after the entry: a crash before this leaves the lost heads,
    /// which compare as no base (a conflict, nothing lost), and the next open pins them.
    fn pin_lost_base(&mut self, rel: &str, md5: &str) -> Result<(), StoreError> {
        let Some(file) = self.heads_of(rel) else {
            return Ok(());
        };
        let Some(placed) = file.placements.iter().find(|p| p.md5 == md5).cloned() else {
            return Ok(());
        };
        let current = self.current_heads(rel);
        let mut changed = false;
        let mut records = placed.records;
        for record in &mut records {
            // Only a record every head of which is lost: a head the fold still holds is a
            // version the save stood beside (a teammate's, a conflict), never one it replaced.
            let all_lost = !record.heads.is_empty()
                && record
                    .heads
                    .iter()
                    .all(|id| !self.shows_held(&tc_journal::ChangeId::from_bytes(*id), &current));
            if !all_lost {
                continue;
            }
            let Some(key) = tc_journal::json::parse(record.key.as_bytes())
                .ok()
                .and_then(|j| tc_journal::canon::decode_key(&j).ok())
            else {
                continue;
            };
            let mut now: Vec<[u8; 32]> = self
                .fold
                .heads(&key)
                .iter()
                .map(|id| *id.as_bytes())
                .collect();
            now.sort();
            // Only onto heads an earlier placement of this file named for the record: what the
            // file showed before the lost save. A head that came since (a teammate's change
            // received) is newer than the save, so the lost heads stay and *Bring it back* is a
            // conflict.
            let earlier = file.placements.iter().filter(|p| p.md5 != md5).any(|p| {
                p.records.iter().any(|r| {
                    let mut heads = r.heads.clone();
                    heads.sort();
                    r.key == record.key && heads == now
                })
            });
            if now.is_empty() || !earlier {
                continue;
            }
            record.heads = now;
            record.shown = None;
            changed = true;
        }
        if !changed {
            return Ok(());
        }
        crate::atomic::atomic_create_dirs(&*self.vfs, &self.folder.path(crate::layout::HEADS))?;
        self.put_heads(rel, md5, records, SyncDir::Now)
    }

    /// At a read-only open, which keeps no copy and asks nothing: a notice for each book file
    /// or story whose bytes are not the ones the app placed (H22 part 4).
    pub(crate) fn notice_changed_while_read_only(&self) {
        for (rel, entry) in &self.manifest.paths {
            let Some(placed) = entry.placement() else {
                continue;
            };
            let Ok(path) = RepoPath::parse(rel) else {
                continue;
            };
            if !asks(rel) {
                continue;
            }
            let file = self.folder.path(rel);
            let same_stat = self.vfs.metadata(&file).is_ok_and(|m| {
                placed.settled && m.len == placed.size && Some(m.mtime_ns) == placed.mtime_ns
            });
            if same_stat {
                continue;
            }
            match self.vfs.read(&file) {
                Ok(bytes) if Md5::of(&bytes).to_hex() != placed.md5 => {
                    self.notice(Notice::ChangedWhileReadOnly { path }, None);
                }
                _ => {}
            }
        }
    }

    /// The examination's state for the recording that a held file never gets.
    fn forget_examined(&mut self, rel: &str) {
        self.shown_by_path.remove(rel);
        self.bases.remove(rel);
    }

    /// The base an *Import* of `entry` compares with (§13.6 U2 a). Heads that are not in the
    /// heads file any more count as no base.
    fn import_base(&mut self, entry: &Kept, file: &DerivedFile, bytes: &[u8]) -> PlanBase {
        match &entry.base {
            KeptBase::Placed(_) => {
                let digests = RepoPath::parse_source(&entry.path)
                    .ok()
                    .and_then(|path| {
                        tc_journal::outside::decode_file(
                            &OutsideFile::Derived(file.clone()),
                            bytes,
                            &path,
                        )
                        .ok()
                    })
                    .map(|d| d.digests)
                    .unwrap_or_default();
                self.read_base(&entry.path, &entry.base.ledger(), &digests)
            }
            KeptBase::Head(Some(commit)) => self.head_base_at(&entry.path, *commit),
            KeptBase::Head(None) | KeptBase::None => PlanBase::Unknown,
        }
    }

    /// *Import* of the kept copy `copy`: its bytes go to the actor with their base, and the
    /// person is answered when the batches are durable ([`Writer::imported`]).
    pub(crate) fn import_kept(
        &mut self,
        copy: &str,
        reply: Resolver<Result<ImportReport, StoreError>>,
    ) {
        match self.prepare_import(copy) {
            Ok(Some((entry, bytes, what))) => {
                let rel = entry.path.clone();
                let sent = self.actor.send(crate::handle::Msg::ImportKept {
                    rel,
                    copy: entry.copy,
                    md5: Md5::of(&bytes),
                    bytes,
                    what,
                    reply,
                });
                if let Err(e) = sent {
                    let crate::handle::Msg::ImportKept { reply, .. } = e.0 else {
                        return;
                    };
                    reply.resolve(Err(StoreError::Other("the project is closed".into())));
                }
            }
            // The copy shows what the project shows already: nothing to import.
            Ok(None) => {
                let result = self.prepare_nothing(copy).map(|()| ImportReport::default());
                reply.resolve(result);
            }
            Err(e) => reply.resolve(Err(e)),
        }
    }

    /// The entry, the copy's bytes and what to import; `None` when the copy differs from the
    /// project in nothing an import takes in.
    #[allow(clippy::type_complexity)]
    fn prepare_import(
        &mut self,
        copy: &str,
    ) -> Result<Option<(Kept, Vec<u8>, ImportWhat)>, StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        self.begin_newest();
        let entry = self
            .kept_entries()
            .into_iter()
            .find(|k| k.copy == copy)
            .ok_or_else(|| StoreError::NotKept {
                copy: copy.to_owned(),
            })?;
        let path = self.folder.path(&entry.copy);
        let bytes = self
            .vfs
            .read(&path)
            .map_err(|e| StoreError::io("read the kept copy", &path, e))?;
        if Md5::of(&bytes).to_hex() != entry.md5 {
            return Err(StoreError::Other(format!(
                "{copy} changed since it was kept; it is left as it is"
            )));
        }
        let file = DerivedFile::from_path(&entry.path)
            .filter(|_| asks(&entry.path))
            .ok_or_else(|| StoreError::Other(format!("{} is no book file", entry.path)))?;
        let what = match &file {
            DerivedFile::Usfm(book) => ImportWhat::Book {
                book: *book,
                file_name: entry
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.path)
                    .to_owned(),
                base: self.import_base(&entry, &file, &bytes),
            },
            // A story's planner reads HEAD as it is now, which may hold the person's later
            // saves: compared with it, the file's older words would come in unasked.
            _ => match self.examine_copied(
                &entry.path,
                &bytes,
                &match entry.base {
                    KeptBase::Head(_) => ledger::Base::Unknown,
                    ref base => base.ledger(),
                },
            ) {
                Examined::Plan(plan, _) => {
                    self.forget_examined(&entry.path);
                    ImportWhat::Story { plan }
                }
                Examined::Nothing => {
                    self.forget_examined(&entry.path);
                    return Ok(None);
                }
                Examined::Undecodable { reason } => {
                    return Err(StoreError::Other(format!(
                        "{} cannot be read: {reason}",
                        entry.path
                    )));
                }
            },
        };
        Ok(Some((entry, bytes, what)))
    }

    /// An import with nothing to take in ends its entry all the same.
    fn prepare_nothing(&mut self, copy: &str) -> Result<(), StoreError> {
        let Some(entry) = self.kept_entries().into_iter().find(|k| k.copy == copy) else {
            return Ok(());
        };
        let path = self.folder.path(&entry.copy);
        let bytes = self
            .vfs
            .read(&path)
            .map_err(|e| StoreError::io("read the kept copy", &path, e))?;
        self.end_import(&entry, &bytes)
    }

    /// The actor's answer to an *Import*: the entry ends, the bytes it was made of become the
    /// app's own when the file still holds them, and the file is written from the project.
    pub(crate) fn imported(
        &mut self,
        copy: &str,
        bytes: &[u8],
        result: Result<ImportReport, StoreError>,
        reply: Resolver<Result<ImportReport, StoreError>>,
    ) {
        let result = result.and_then(|report| {
            self.begin_newest();
            // An earlier *Import* of the same copy ended its entry while this one waited
            // for the actor: this one took nothing in, and the earlier one has the report.
            let entry = self
                .kept_entries()
                .into_iter()
                .find(|k| k.copy == copy)
                .ok_or_else(|| StoreError::NotKept {
                    copy: copy.to_owned(),
                })?;
            self.end_import(&entry, bytes)?;
            Ok(report)
        });
        reply.resolve(result);
    }

    /// Ends `entry` in one manifest write; a held file that still holds its bytes is the
    /// app's own after it (the import holds what they show), so the next job writes it.
    fn end_import(&mut self, entry: &Kept, bytes: &[u8]) -> Result<(), StoreError> {
        let holds_these = matches!(self.state_of(&entry.path),
            State::Held { held } if held.md5 == entry.md5);
        if holds_these {
            // While the entry lives, the heads file keeps its base: a crash before the entry
            // ends then leaves it with the base its Import compares with.
            self.accept(&entry.path, Md5::of(bytes), bytes, None)?;
        }
        self.manifest.kept.retain(|k| k.copy != entry.copy);
        self.manifest_changed = true;
        self.write_manifest()?;
        // A mark that cannot be written only lets an open that lost the manifest list the
        // imported copy again, as an outside change to answer.
        let _ = self.mark(&entry.copy, IMPORTED);
        if let Some(file) =
            DerivedFile::from_path(&entry.path).filter(|f| self.fold.projects_file(f))
        {
            self.dirty.add_file(file);
        }
        self.finish_looking()
    }

    /// Marks a copy with an empty file beside it ([`IMPORTED`], [`LOST`]), which an open that
    /// lost the manifest reads; the copy keeps its name and bytes.
    fn mark(&self, copy: &str, mark: &str) -> Result<(), StoreError> {
        let (dir, name) = self.folder.dir_and_name(copy);
        match crate::atomic::atomic_create(
            &*self.vfs,
            &dir,
            &format!("{name}{mark}"),
            b"",
            SyncDir::Now,
        ) {
            Ok(()) => Ok(()),
            // An earlier keep of the same copy marked it already.
            Err(crate::atomic::WriteError::NotWritten(e))
                if e.kind() == Some(std::io::ErrorKind::AlreadyExists) =>
            {
                Ok(())
            }
            Err(e) => Err(e.into_inner()),
        }
    }

    /// *Not now* for a held book file or story: the file is written again from the project
    /// (a file of a book the project lacks leaves the folder); its copy stays listed.
    pub(crate) fn not_now(&mut self, rel: &str) -> Result<(), StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        self.begin_newest();
        // Lost work of the path is answered: it is not asked again and stays listed.
        let mut answered = false;
        for kept in &mut self.manifest.kept {
            if kept.path == rel && kept.why == KeptWhy::Lost && !kept.answered {
                kept.answered = true;
                answered = true;
            }
        }
        if answered {
            self.manifest_changed = true;
            self.write_manifest()?;
        }
        let State::Held { held } = self.state_of(rel) else {
            return Ok(());
        };
        let Some(file) = DerivedFile::from_path(rel).filter(|_| asks(rel)) else {
            return Err(StoreError::Other(format!("{rel} is no book file")));
        };
        if !self.kept_entries().iter().any(|k| k.path == rel) {
            return Err(StoreError::Other(format!("{rel} has no kept copy")));
        }
        if self.fold.projects_file(&file) {
            self.restore_held(rel)?;
            if self.manifest_changed {
                self.write_manifest()?;
            }
            return Ok(());
        }
        // A file of a book the project lacks: its copy is kept, so it leaves the folder,
        // while it still holds the bytes held.
        let path = self.folder.path(rel);
        let removed = atomic_remove_checked(
            &*self.vfs,
            &path,
            SyncDir::Now,
            &HeldRetry::SAVE,
            &mut || match self.vfs.read(&path) {
                Ok(now) => Ok(Md5::of(&now).to_hex() == held.md5),
                Err(e) if crate::vfs::is_not_found(&e) => Ok(false),
                Err(e) => Err(StoreError::io("read", &path, e)),
            },
        )?;
        if removed == Removed::Refused && self.vfs.metadata(&path).is_ok() {
            return Err(StoreError::Other(format!(
                "{rel} changed again; it is kept and asked about"
            )));
        }
        self.end_hold(rel);
        self.dirty.metadata = true;
        self.write_manifest()?;
        self.finish_looking()
    }

    /// *Dismiss* of a kept copy: its entry ends, and the copy goes while its bytes are still
    /// the ones kept. A copy whose book still waits for an answer is not dismissed.
    pub(crate) fn dismiss_kept(&mut self, copy: &str) -> Result<(), StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        let Some(entry) = self.kept_entries().into_iter().find(|k| k.copy == copy) else {
            return Ok(());
        };
        if entry.why == KeptWhy::Lost {
            return Err(StoreError::LostWorkKept {
                copy: copy.to_owned(),
            });
        }
        if matches!(self.state_of(&entry.path),
            State::Held { held } if held.md5 == entry.md5)
        {
            return Err(StoreError::Other(format!(
                "{} waits for an answer: Import or Not now",
                entry.path
            )));
        }
        self.manifest.kept.retain(|k| k.copy != copy);
        self.manifest_changed = true;
        self.write_manifest()?;
        let named = self.kept_entries().iter().any(|k| k.copy == copy)
            || self
                .manifest
                .paths
                .values()
                .any(|e| e.held().is_some_and(|h| h.copy == copy));
        if named {
            return Ok(());
        }
        let path = self.folder.path(copy);
        atomic_remove_checked(
            &*self.vfs,
            &path,
            SyncDir::Now,
            &HeldRetry::SAVE,
            &mut || match self.vfs.read(&path) {
                Ok(now) => Ok(Md5::of(&now).to_hex() == entry.md5),
                Err(e) if crate::vfs::is_not_found(&e) => Ok(false),
                Err(e) => Err(StoreError::io("read the kept copy", &path, e)),
            },
        )?;
        Ok(())
    }
}

impl crate::project::Project {
    /// The batches of an *Import* (§13.6 U5): each names the file and the md5 it reconciles.
    /// A book's first batch is what comes in with no question, its second the changes to
    /// review, planned again on the project the first left; an import run again after a crash
    /// between them adds nothing the first run wrote.
    pub(crate) fn import_kept(
        &mut self,
        rel: &str,
        md5: Md5,
        bytes: &[u8],
        what: ImportWhat,
    ) -> Result<ImportReport, StoreError> {
        let path = RepoPath::parse_source(rel)
            .map_err(|_| StoreError::Other(format!("{rel} is no name to import from")))?;
        let reconciles = tc_journal::pending::Reconciles {
            path,
            seen_md5: md5,
        };
        let commit = |project: &mut Self, changes| {
            project
                .commit_reconciling(
                    crate::commands::Plan {
                        changes,
                        skipped: Vec::new(),
                        kept: Vec::new(),
                        dropped_links: Vec::new(),
                    },
                    Some(reconciles.clone()),
                )
                .map(|reply| {
                    reply
                        .changes
                        .first()
                        .and_then(|id| project.fold.change(id))
                        .map(|view| view.batch)
                })
                .map_err(|e| StoreError::Other(e.to_string()))
        };
        match what {
            // A book the project never had: "Update a book" compares verse by verse and has
            // no verse to fill, so the file's book is added with its text (`AddBookText`, the
            // import of a new book, §13.6 U1).
            ImportWhat::Book {
                book, file_name, ..
            } if self
                .fold
                .heads(&tc_journal::RecordKey::BookPresence(book))
                .is_empty() =>
            {
                let usfm = std::str::from_utf8(bytes)
                    .map_err(|_| StoreError::Other(format!("{rel} is not UTF-8 text")))?;
                let command = tc_journal::command::Command::AddBookText {
                    book,
                    scope: tc_journal::record::Scope::Whole,
                    usfm: usfm.to_owned(),
                    file_name,
                };
                let plan = crate::commands::plan(&self.ctx(), &command)
                    .map_err(|e| StoreError::Other(e.to_string()))?;
                let verses = plan
                    .changes
                    .iter()
                    .filter(|c| matches!(c.record.key(), tc_journal::RecordKey::Verse(..)))
                    .count();
                commit(self, plan.changes)?;
                Ok(ImportReport {
                    imported: u32::try_from(verses).unwrap_or(u32::MAX),
                    book_added: true,
                    ..ImportReport::default()
                })
            }
            ImportWhat::Book {
                book,
                file_name,
                base,
            } => {
                let plan = |project: &Self| {
                    project
                        .fold
                        .plan_update_book_from(book, bytes, &file_name, &base)
                        .map_err(StoreError::Other)
                };
                let fill = plan(self)?;
                let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
                let mut report = ImportReport {
                    imported: count(fill.imported.len()),
                    to_review: count(fill.to_review.len()),
                    left_as_it_was: count(fill.left_as_it_was.len()),
                    book_added: fill.book_added,
                    review: None,
                };
                commit(self, fill.changes)?;
                let review = plan(self)?;
                report.review = commit(self, review.review)?;
                Ok(report)
            }
            ImportWhat::Story { plan } => match self.record_outside(rel, md5, *plan, 0) {
                crate::writer::Recorded::Done {
                    batch,
                    changes,
                    unknown_base,
                    ..
                } => Ok(ImportReport {
                    imported: if unknown_base { 0 } else { changes },
                    to_review: if unknown_base { changes } else { 0 },
                    review: batch.filter(|_| unknown_base),
                    ..ImportReport::default()
                }),
                crate::writer::Recorded::Refused(reason) => Err(StoreError::Other(reason)),
                crate::writer::Recorded::NotWritten => Err(StoreError::Other(
                    "the disk refused the changes; the copy is kept".into(),
                )),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeptBase, source_of_copy};

    #[test]
    fn a_head_base_keeps_its_commit_and_reads_an_earlier_builds_head() {
        let commit = tc_journal::GitOid::parse("c354b8ae0a8a0e1e47b4c36d0b5e6e8a2c3f4d5e").unwrap();
        let text = String::from(KeptBase::Head(Some(commit)));
        assert_eq!(text, "head:c354b8ae0a8a0e1e47b4c36d0b5e6e8a2c3f4d5e");
        assert_eq!(KeptBase::try_from(text), Ok(KeptBase::Head(Some(commit))));
        assert_eq!(
            KeptBase::try_from("head".to_owned()),
            Ok(KeptBase::Head(None))
        );
        assert!(KeptBase::try_from("head:xyz".to_owned()).is_err());
    }

    #[test]
    fn a_copys_name_gives_its_book_file_and_nothing_else() {
        let tit = Some("ingredients/TIT.usfm".to_owned());
        assert_eq!(
            source_of_copy("20261017T000000Z-ingredients%2FTIT.usfm"),
            tit
        );
        assert_eq!(
            source_of_copy("20261017T000000Z-ingredients%2FTIT.usfm-3"),
            tit
        );
        assert_eq!(source_of_copy("20261017T000000Z-metadata.json-2"), None);
        assert_eq!(source_of_copy("20261017T000000Z-not-saved-x.tmp"), None);
        assert_eq!(
            source_of_copy("20261017T000000Z-ingredients%2FTIT.usfm-x"),
            None
        );
    }
}
