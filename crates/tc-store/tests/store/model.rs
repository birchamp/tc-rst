//! A model-based property test of the project store (`docs/STORAGE.md` §16.1, §6.2, §11): random
//! histories of a Titus project run against the real store on a `FaultFs`, and after every step
//! the store is compared with a small reference model of what the person did.
//!
//! The model keeps two kinds of work. A **save** is words the person saved through the app (a
//! verse, a section of several verses in one batch, the project's name) and was told were saved.
//! A **put** is the whole bytes another program wrote into the folder (an edit of a verse in the
//! book file, an edit of `metadata.json`, a version of either put back). The no-loss invariant:
//!
//! - every save is a version of its record in History (readable), unless the damage the run did
//!   took every copy of it (a journal line cut away, and no file in the folder showing it; for a
//!   save of the session that runs, the session holds it until its next seal, and a kill before
//!   that takes it unless a file still shows it); a
//!   member of a batch that lost another member waits for the rest (§7.4), and a save the
//!   journal lost that is in a byte copy in `.tc/external/` (with its notice), count as kept,
//!   unless a file in the folder showed it when the open read the journal: those bytes are then
//!   the record of it, and the app must record them, not write the file over without it;
//! - every put is in the folder still, is what the project would write there, or is copied to
//!   `.tc/external/` (§11.1 X1); a put the person overwrote before the app looked is retired,
//!   and so is a put independently observed to equal the complete current projection.
//!
//! Two more oracles catch work kept twice or refused: a history with no outside step (a put, a
//! cut, a deletion or change of a journal file, a lost `.tc/`) shows no outside-edit notice and
//! makes no copy in `.tc/external/`, except of the bytes a write under way replaced, which a
//! crash leaves for R9 (§6.2); and saves and checkpoints succeed unless the process died, or a
//! file another program holds or damaged stops the seal.
//!
//! While a session runs, a save may also be only in a file in the folder (a journal file another
//! program cut in place leaves the session unable to read it; the next open reads the journal
//! again, and restores a pack HEAD holds). After an open and its first job, and at the end of a
//! run, it may not.
//!
//! The steps: saves, materializations and checkpoints, each of which can be killed before any of
//! its write points (with or without a power cut, a torn write or not); a kill right after the
//! rename of a file; outside edits, reverts (keeping the old time or not) and deletions while
//! open and while closed; pending files and packs cut, changed or deleted, in
//! place while open or while closed, or held open by another program; files whose reads are
//! denied while replacement is allowed; a lost manifest or heads files; restarts; the window's
//! focus; the clock. The generator favours the sequences the reviews found bugs in: a crash
//! followed by another program's change while closed, a kill after a rename followed by a
//! revert, a file held around an open.
//!
//! One translationCore runs per computer (`rulings/2026-10-22-one-instance.md`), so a run
//! never has two sessions open at once. Disk damage that keeps a file's size and time is
//! outside what a translator can reach, and the 2026-10-21 rulings remove its hold (R10) and
//! the person's *Take in* / *Restore* of a held file: the model has no such steps.

use crate::common::*;
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tc_journal::command::{Command, SectionValue};
use tc_journal::fold::{DerivedFile, Notice};
use tc_journal::record::VerseValue;
use tc_journal::{MetaField, Record, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::handle::CheckpointSkip;
use tc_store::ledger::State;
use tc_store::manifest::Manifest;
use tc_store::{
    CheckpointReason, CommandError, Leave, Mode, OpenOptions, ProjectHandle, StoreEvent,
    StoreNotice,
};

const USFM: &str = "ingredients/TIT.usfm";
const META: &str = "metadata.json";
const MANIFEST: &str = ".tc/materialized.json";

/// What the person does in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Saves verse 1:v.
    Save(u16),
    /// Saves verses 1:v and 1:v+1 in one section save: one batch of two changes.
    Section(u16),
    /// Changes the project's name.
    Name,
    /// Leaves the book: the book's files are written.
    Leave,
    /// A checkpoint: seal, files, commit.
    Checkpoint,
    /// Closes the project.
    Close,
}

/// A file another program edits or puts back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum File {
    Usfm,
    Meta,
}

impl File {
    fn rel(self) -> &'static str {
        match self {
            Self::Usfm => USFM,
            Self::Meta => META,
        }
    }
}

/// A journal file another program cuts, changes or deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Journal {
    Pending,
    Pack,
}

/// A file another program holds open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holdable {
    File(File),
    /// The `which`-th pending file (letting go lets go of every pending file held);
    /// `readable`: the program shares it for reading (a scanner), so only its removal fails.
    Pending {
        which: u8,
        readable: bool,
    },
}

/// The process dies before the `at`-th write point from now (during the act, or the work it
/// starts), with a power cut or not; `torn`: the write stopped there writes half its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kill {
    pub at: u16,
    pub power_cut: bool,
    pub torn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The person acts, and the process may die meanwhile.
    Act { act: Act, kill: Option<Kill> },
    /// The process will die right after it next renames `file` into place.
    KillAfterRename { file: File },
    /// Another program holds a file open (`on`), or lets go of it; a readable pending file
    /// can still be read while its replacement and removal are denied.
    Lock { what: Holdable, on: bool },
    /// Reads of a derived file or `metadata.json` are denied (`on`), or allowed again;
    /// replacement and removal remain permitted unless another hold denies them.
    DenyRead { file: File, on: bool },
    /// The app opens the project (it does at any act while closed, too).
    Open,
    /// Another program puts new words in verse 1:v of the book file; `told`: the watcher sees it.
    Edit { verse: u16, told: bool },
    /// Another program changes the project's name in `metadata.json`.
    EditName { told: bool },
    /// Another program puts back the `back`-th older version of a file the run saw.
    Revert { file: File, back: u8, told: bool },
    /// A backup tool (`cp -p`, `rsync -a`, Time Machine) puts back the `back`-th older version
    /// of a file with the modification time it had when the run first saw it, unseen.
    RevertKeepTime { file: File, back: u8 },
    /// Another program deletes a file the app derives.
    Delete { file: File },
    /// Another program sets a file's modification time to now and leaves its bytes (a sync
    /// client, `touch`).
    Touch { file: File },
    /// Another program cuts a journal file: the `which`-th, keeping `keep` (a share of its
    /// lines), at a line end or inside a line.
    Cut {
        journal: Journal,
        which: u8,
        keep: u8,
        at_line: bool,
    },
    /// Another program deletes the `which`-th journal file.
    DeleteJournal { journal: Journal, which: u8 },
    /// A checkpoint, during which another program deletes the `which`-th pending file when the
    /// seal reaches its `at`-th step (after K1 read it, before K4 removes it, or between).
    DeleteDuringSeal { which: u8, at: u8 },
    /// Another program changes one byte after the header of the `which`-th journal file.
    Flip {
        journal: Journal,
        which: u8,
        at: u16,
    },
    /// `.tc/` loses the manifest, or every heads file, while the app is closed.
    LoseTc { heads: bool },
    /// The window gets focus.
    Focus,
    /// Ten seconds pass.
    Advance,
}

/// Where a save is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Verse(u16),
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Must be kept.
    Kept,
    /// Damage took every copy on disk while the session that saved it runs: that session holds
    /// it, and must write it again before it closes; a kill takes it.
    InMemory,
    /// Damage took every copy.
    Destroyed,
}

#[derive(Debug, Clone)]
struct Save {
    text: String,
    target: Target,
    session: u32,
    fate: Fate,
    /// A file in the folder showed it when the last open read the journal.
    shown: bool,
    /// [`Run::elapsed_ms`] when it was saved.
    at_ms: i64,
}

#[derive(Debug, Clone)]
struct Put {
    rel: &'static str,
    bytes: Vec<u8>,
    what: String,
    retired: bool,
    /// What the put shows that the app's last write of the path did not: what another
    /// program brought, however many puts it took.
    shows: Vec<Shown>,
    /// The app looks at it with no placement for its path (the path was deleted or
    /// `.tc/materialized.json` lost): its base is HEAD's file (R14).
    unplaced: bool,
}

/// A value an outside put shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shown {
    Verse(u16, String),
    Name(String),
}

/// The words of each verse of chapter 1 a book file shows.
fn chapter_one(bytes: &[u8]) -> BTreeMap<u16, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = BTreeMap::new();
    let mut in_one = false;
    for line in text.lines() {
        if let Some(chapter) = line.strip_prefix("\\c ") {
            in_one = chapter.trim() == "1";
        } else if in_one && let Some(rest) = line.strip_prefix("\\v ") {
            let (number, words) = rest.split_once(' ').unwrap_or((rest, ""));
            if let Ok(v) = number.parse() {
                out.insert(v, words.to_owned());
            }
        }
    }
    out
}

fn name_shown(bytes: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    json["identification"]["name"]["en"]
        .as_str()
        .map(str::to_owned)
}

/// What `bytes` at `rel` show that `before` did not.
fn shown_by(rel: &str, before: Option<&[u8]>, bytes: &[u8]) -> Vec<Shown> {
    debug_assert!(rel == USFM || rel == META);
    if rel == META {
        let name = name_shown(bytes);
        return match name {
            Some(name) if before.and_then(name_shown).as_ref() != Some(&name) => {
                vec![Shown::Name(name)]
            }
            _ => Vec::new(),
        };
    }
    let old = before.map(chapter_one).unwrap_or_default();
    chapter_one(bytes)
        .into_iter()
        .filter(|(v, words)| old.get(v) != Some(words))
        .map(|(v, words)| Shown::Verse(v, words))
        .collect()
}

/// A complete copy the public notice actually showed, retained across hold ending or dismissal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedCopy {
    source: String,
    copy: String,
    bytes: Vec<u8>,
}

thread_local! {
    static TERMINAL_BOUNDARIES: std::cell::Cell<[u64; 2]> =
        const { std::cell::Cell::new([0, 0]) };
}

fn terminal_boundaries() -> [u64; 2] {
    TERMINAL_BOUNDARIES.with(std::cell::Cell::get)
}

fn count_terminal_boundary(ready: bool) {
    TERMINAL_BOUNDARIES.with(|counts| {
        let mut next = counts.get();
        next[usize::from(!ready)] += 1;
        counts.set(next);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeniedAccess {
    Read,
    Replace,
    ReadAndReplace,
}

impl DeniedAccess {
    fn denies_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadAndReplace)
    }

    fn denies_replacement(self) -> bool {
        matches!(self, Self::Replace | Self::ReadAndReplace)
    }
}

/// One run of steps against the store.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a test model's independent facts about one run"
)]
pub struct Run {
    rig: Option<Rig>,
    /// The running store's clock, which [`Step::Advance`] moves with the folder's.
    clock: std::sync::Arc<tc_store::ManualClock>,
    /// The time [`Step::Advance`] has moved, the clock the 2 minutes of the acceptable-loss
    /// ruling are measured on.
    elapsed_ms: i64,
    folder: PathBuf,
    handle: Option<ProjectHandle>,
    session: u32,
    next: u32,
    saves: Vec<Save>,
    puts: Vec<Put>,
    observed_copies: Vec<ObservedCopy>,
    /// Every version of each file the run saw in the folder, oldest first, with its
    /// modification time when the run first saw it.
    versions: BTreeMap<&'static str, Vec<(Vec<u8>, i64)>>,
    /// The bytes the app last put at each path, as the run saw them appear: a file put back
    /// to them is the app's own (R2), however the ledger names it.
    app_wrote: BTreeMap<&'static str, Vec<u8>>,
    /// What the run last saw at each path.
    last_seen: BTreeMap<&'static str, Option<Vec<u8>>>,
    /// Every version the run saw the app write at each path.
    app_versions: BTreeMap<&'static str, Vec<Vec<u8>>>,
    /// Paths the app keeps no placement of since another program deleted them or the
    /// manifest was lost, until the run sees the app write them again.
    unplaced: std::collections::BTreeSet<&'static str>,
    /// Whether the kill armed is a power cut.
    power_cut: bool,
    /// Access another program denies to files, relative to the folder.
    locked: BTreeMap<String, DeniedAccess>,
    /// An outside step changed the folder: a put, a cut, a journal file changed, deleted or
    /// held unreadable, `.tc/` lost. Until then the app keeps nothing twice.
    outside: bool,
    /// The bytes a crash left at a path whose write was under way, the ones it replaced: R9
    /// keeps them (an extra conflict the owner accepted).
    replaced: Vec<Vec<u8>>,
    /// Another program held a pending file unreadable during this session: a session never reads again
    /// a journal file it could not read, so until the next open its seal and its checkpoints
    /// stop, and a save of a version whose value lies there is refused.
    journal_held: bool,
    /// A step that another program acts during checks once it has accounted for it.
    check_later: bool,
    /// Every journal file the run saw, in the order it first saw them: a step names a journal
    /// file by its place here, never by its name, which holds a random batch id.
    journal_seen: Vec<String>,
    log: Vec<String>,
}

/// A copy of a new Titus project's files, made once.
fn template() -> &'static (FaultFs, PathBuf) {
    static TEMPLATE: OnceLock<(FaultFs, PathBuf)> = OnceLock::new();
    TEMPLATE.get_or_init(|| {
        let (rig, folder) = titus();
        let fs = rig.fs.copy();
        (fs, folder)
    })
}

fn verse_key(v: u16) -> RecordKey {
    verse(1, v)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn md5(bytes: &[u8]) -> String {
    tc_journal::Md5::of(bytes).to_hex()
}

/// Notices that say the app found bytes it did not write.
fn outside_notice(notice: &Notice) -> bool {
    matches!(
        notice,
        Notice::OutsideEditRecorded { .. }
            | Notice::OutsideEditHeld { .. }
            | Notice::OutsideEditUnreadable { .. }
            | Notice::OutsideRemovalKept { .. }
            | Notice::StorageDamage { .. }
            | Notice::MetadataEditedOutside { .. }
            | Notice::DerivedFieldsEdited { .. }
            | Notice::FixedFileRestored { .. }
    )
}

impl Run {
    pub fn new() -> Self {
        let (fs, folder) = template();
        let (rig, clock) = Self::rig_on(fs.copy());
        let mut run = Self {
            rig: Some(rig),
            clock,
            elapsed_ms: 0,
            folder: folder.clone(),
            handle: None,
            session: 0,
            next: 0,
            saves: Vec::new(),
            puts: Vec::new(),
            observed_copies: Vec::new(),
            versions: BTreeMap::new(),
            app_wrote: BTreeMap::new(),
            last_seen: BTreeMap::new(),
            app_versions: BTreeMap::new(),
            unplaced: std::collections::BTreeSet::new(),
            power_cut: false,
            locked: BTreeMap::new(),
            outside: false,
            replaced: Vec::new(),
            journal_held: false,
            journal_seen: Vec::new(),
            check_later: false,
            log: Vec::new(),
        };
        run.see_versions();
        run
    }

    fn fs(&self) -> FaultFs {
        self.rig.as_ref().expect("a rig").fs.clone()
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.folder.join(rel)
    }

    fn token(&mut self) -> String {
        self.next += 1;
        format!("zq{}x", self.next)
    }

    fn handle(&self) -> &ProjectHandle {
        self.handle.as_ref().expect("open")
    }

    /// The ledger's record of `rel`, as the manifest on disk holds it.
    fn ledger(&self, rel: &str) -> Option<State> {
        let bytes = self.fs().peek(&self.path(MANIFEST))?;
        let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
        manifest.paths.get(rel).map(|e| e.state.clone())
    }

    /// Opens the project, and checks the store once the open's first job is done.
    fn open(&mut self) -> Result<(), String> {
        if self.handle.is_some() {
            return Ok(());
        }
        // What the folder shows of each save when the open reads the journal: a save the
        // journal lost, or holds only in a batch that lost a member, is the app's to take from
        // a file that shows it.
        let shown: Vec<bool> = self.saves.iter().map(|s| self.in_files(&s.text)).collect();
        for (save, shown) in self.saves.iter_mut().zip(shown) {
            save.shown = shown;
        }
        loop {
            let rig = self.rig.as_ref().expect("a rig");
            let opened = rig.store.open(&self.folder, &OpenOptions::default());
            if self.died() {
                // A kill armed stopped the open: the next process opens again.
                continue;
            }
            let handle = opened.map_err(|e| format!("the open failed: {e:?}"))?;
            handle.wait_idle();
            self.session += 1;
            self.journal_held = self
                .locked
                .iter()
                .any(|(rel, access)| rel.starts_with(".tc/pending/") && access.denies_read());
            self.handle = Some(handle);
            self.log.push(format!("opened (session {})", self.session));
            if !self.died() {
                return self.check(true);
            }
        }
    }

    /// A store on `fs`. Outside edits the watcher misses leave `metadata.json` listing bytes the
    /// folder no longer holds until the app looks, so the rig does not check the burrito here.
    fn rig_on(fs: FaultFs) -> (Rig, std::sync::Arc<tc_store::ManualClock>) {
        let (config, clock) = config(&fs, APP, "host-1");
        let store = tc_store::Store::start(config).unwrap();
        store.wait_cleanup();
        watch_burrito(&store, &fs);
        let rig = Rig { fs, store };
        damaged_on_purpose(&rig);
        (rig, clock)
    }

    fn restart_on(&mut self, fs: FaultFs) {
        let (rig, clock) = Self::rig_on(fs);
        self.rig = Some(rig);
        self.clock = clock;
    }

    /// The process died: the next one finds the files as a kill or a power cut leaves them.
    fn crash(&mut self, power_cut: bool) {
        let before: BTreeMap<&'static str, Option<Vec<u8>>> = [USFM, META]
            .into_iter()
            .map(|rel| (rel, self.fs().peek(&self.path(rel))))
            .collect();
        let fs = self.fs().crash(power_cut);
        self.observe_notices(&fs);
        // The narrow metadata witness uses the actual surviving complete Burrito.
        // A dying FaultRead or in-memory value cannot certify USFM after a power cut.
        self.observe_complete_metadata(&fs);
        if let Some(handle) = self.handle.take() {
            drop(handle);
        }
        self.rig = None;
        self.restart_on(fs);
        // A power cut can take the app's last write back to an earlier one of its own: that
        // earlier write is what the app placed last, whatever another program also put.
        for rel in [USFM, META] {
            let now = self.fs().peek(&self.path(rel));
            // Only bytes the power cut itself brought back can be the app's earlier write;
            // bytes that were there before it (another program's put) stay that program's.
            if let Some(bytes) = &now
                && power_cut
                && before.get(rel) != Some(&now)
                && self.app_wrote.get(rel) != Some(bytes)
                && self
                    .app_versions
                    .get(rel)
                    .is_some_and(|seen| seen.contains(bytes))
            {
                self.app_wrote.insert(rel, bytes.clone());
                self.last_seen.insert(rel, now);
            }
        }
        // What only the dead session held is gone, unless a file in the folder still shows it.
        let kept: Vec<bool> = self.saves.iter().map(|s| self.on_disk(&s.text)).collect();
        for (save, kept) in self.saves.iter_mut().zip(kept) {
            if save.fate == Fate::InMemory {
                save.fate = if kept { Fate::Kept } else { Fate::Destroyed };
            }
        }
        // A write under way whose rename did not happen leaves the bytes before it, which the
        // next look keeps (R9), at any path the app writes.
        let manifest: Option<Manifest> = self
            .fs()
            .peek(&self.path(MANIFEST))
            .and_then(|b| serde_json::from_slice(&b).ok());
        for (rel, entry) in manifest.map(|m| m.paths).unwrap_or_default() {
            if let (State::Writing { old: Some(old), .. }, Some(bytes)) =
                (&entry.state, self.fs().peek(&self.path(&rel)))
                && old.md5 == md5(&bytes)
            {
                self.replaced.push(bytes);
            }
        }
        self.relock();
        self.log.push(format!("crashed (power cut: {power_cut})"));
    }

    /// An old notice is evidence only while its complete byte copy still exists unchanged.
    fn observed_copy_exists(&self, fs: &FaultFs, seen: &ObservedCopy) -> bool {
        fs.peek(&self.path(&seen.copy)).as_deref() == Some(seen.bytes.as_slice())
    }

    fn observe_notice(&mut self, fs: &FaultFs, notice: &StoreNotice) {
        let Some(copy) = notice.copy.as_deref() else {
            return;
        };
        if !copy.starts_with(".tc/external/") {
            return;
        }
        let source = match &notice.notice {
            Notice::OutsideEditRecorded { path, .. }
            | Notice::OutsideRemovalKept { path, .. }
            | Notice::DerivedFieldsEdited { path, .. } => path.as_str(),
            Notice::OutsideEditHeld {
                path,
                copy: embedded,
            }
            | Notice::StorageDamage {
                path,
                copy: embedded,
            }
            | Notice::OutsideEditUnreadable {
                path,
                copy: embedded,
                ..
            }
            | Notice::FixedFileRestored {
                path,
                copy: embedded,
            } if embedded.as_ref() == copy => path.as_str(),
            Notice::MetadataEditedOutside { copy: embedded } if embedded.as_ref() == copy => META,
            _ => return,
        };
        let Some(bytes) = fs.peek(&self.path(copy)) else {
            return;
        };
        let seen = ObservedCopy {
            source: source.to_owned(),
            copy: copy.to_owned(),
            bytes,
        };
        if !self.observed_copies.contains(&seen) {
            self.observed_copies.push(seen);
        }
    }

    fn observe_notices(&mut self, fs: &FaultFs) {
        let notices = self
            .handle
            .as_ref()
            .map(ProjectHandle::notices)
            .unwrap_or_default();
        for notice in &notices {
            self.observe_notice(fs, notice);
        }
        // A kept copy is told in the conflict review's notices, with Import and Dismiss.
        let kept = self
            .handle
            .as_ref()
            .map(ProjectHandle::kept_copies)
            .unwrap_or_default();
        for kept in kept {
            let Some(bytes) = fs.peek(&self.path(&kept.copy)) else {
                continue;
            };
            let seen = ObservedCopy {
                source: kept.path,
                copy: kept.copy,
                bytes,
            };
            if !self.observed_copies.contains(&seen) {
                self.observed_copies.push(seen);
            }
        }
    }

    fn observe_notice_events(
        &mut self,
        fs: &FaultFs,
        events: impl IntoIterator<Item = StoreEvent>,
    ) {
        for event in events {
            if let StoreEvent::Notice(notice) = event {
                self.observe_notice(fs, &notice);
            }
        }
    }

    fn in_lost_copy(&self, fs: &FaultFs, save: &Save) -> bool {
        self.handle.as_ref().is_some_and(|handle| {
            handle.kept_copies().into_iter().any(|kept| {
                kept.why == tc_store::KeptWhy::Lost
                    && fs
                        .peek(&self.path(&kept.copy))
                        .is_some_and(|b| contains(&b, &save.text))
            })
        })
    }

    fn save_in_notified_copy(&self, save: &Save) -> bool {
        let source = match save.target {
            Target::Verse(_) => USFM,
            Target::Name => META,
        };
        let fs = self.fs();
        self.observed_copies.iter().any(|seen| {
            seen.source == source
                && contains(&seen.bytes, &save.text)
                && self.observed_copy_exists(&fs, seen)
        })
    }

    /// Only a live complete projection grants retirement; no ledger entry is evidence.
    /// A writer an armed kill stopped leaves its handle until `died` runs, and its values
    /// and page cache need not survive: only a filesystem alive on both sides of the look
    /// counts. `crash` observes the actual survivor instead.
    fn observe_live_projection(&mut self) {
        let fs = self.fs();
        self.observe_notices(&fs);
        if fs.is_dead() {
            return;
        }
        let Some(handle) = &self.handle else {
            return;
        };
        let snapshot = handle.snapshot();
        let fold = snapshot.fold();
        let before = fold.values_unreadable();
        let book = fold.render(&DerivedFile::Usfm(TIT));
        let readable = fold.values_unreadable() == before;
        drop(snapshot);
        if fs.is_dead() {
            return;
        }
        if readable && let Some(book) = &book {
            for put in &mut self.puts {
                if put.rel == USFM && put.bytes == *book {
                    put.retired = true;
                }
            }
            // The folder showing the projection is converged (R2): the app places those bytes.
            if fs.peek(&self.path(USFM)).as_ref() == Some(book) {
                self.app_wrote.insert(USFM, book.clone());
            }
        }
        self.observe_live_metadata(&fs);
    }

    fn observe_live_metadata(&mut self, fs: &FaultFs) {
        let retired: Vec<bool> = self.puts.iter().map(|p| p.retired).collect();
        self.observe_complete_metadata(fs);
        if fs.is_dead() {
            for (put, was) in self.puts.iter_mut().zip(retired) {
                put.retired = was;
            }
        }
    }

    /// A group can reach convergence and die before the next model step. Observe the
    /// actual complete Burrito then, without taking a ledger claim as proof of its bytes.
    /// This model edits the name and book bytes; broader metadata edits need their own
    /// convergence oracle before the generator can use them.
    fn observe_complete_metadata(&mut self, fs: &FaultFs) {
        let Some(handle) = &self.handle else {
            return;
        };
        let Some(bytes) = fs.peek(&self.path(META)) else {
            return;
        };
        let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return;
        };
        let snapshot = handle.snapshot();
        let source = snapshot.fold().metadata_source_nfc(
            (
                u64::from(tc_journal::FORMAT_MAJOR),
                u64::from(tc_journal::FORMAT_MINOR),
            ),
            true,
        );
        if !source.fields.contains_key("identification.name") {
            return;
        }
        let fields_match = source.fields.iter().all(|(path, value)| {
            tc_burrito::is_derived_field(path)
                || path.split('.').try_fold(&document, |at, part| at.get(part)) == Some(value)
        });
        if !fields_match
            || !metadata_fields_accounted_for(&document, "", &source.fields)
            || !burrito_problems(fs, &self.folder).is_empty()
        {
            return;
        }
        for put in &mut self.puts {
            if put.rel == META && put.bytes == bytes {
                put.retired = true;
            }
        }
    }

    /// Whether the process died during what was just done (a kill armed may stop it at any
    /// later write point); if so, the next one starts.
    fn died(&mut self) -> bool {
        if self.fs().is_dead() {
            if std::env::var_os("TC_MODEL_TRACE").is_some() {
                for op in self.fs().log() {
                    let path = op.path.display().to_string();
                    eprintln!(
                        "  op {} {:?} {}",
                        op.index,
                        op.kind,
                        path.rsplit("Titus/").next().unwrap_or(&path)
                    );
                }
            }
            self.crash(self.power_cut);
            return true;
        }
        false
    }

    /// A checkpoint or a choice the store refused: a failure unless the process died.
    fn refused(&mut self, what: &str, error: impl std::fmt::Debug) -> Result<(), String> {
        if self.fs().is_dead() {
            return Ok(());
        }
        Err(format!("{what} was refused: {error:?}"))
    }

    /// A save the store refused: a loss of the person's work unless the process died, or an
    /// outside step changed what the screen showed (a refusal by a rule, "look again").
    fn refused_save(&mut self, what: &str, error: CommandError) -> Result<(), String> {
        if (self.outside || self.journal_held)
            && matches!(error, CommandError::Refused(_) | CommandError::LookAgain)
        {
            return Ok(());
        }
        self.refused(what, error)
    }

    fn act(&mut self, act: Act, kill: Option<Kill>) -> Result<(), String> {
        self.open()?;
        if let Some(kill) = kill {
            self.fs().kill_before(u64::from(kill.at), kill.torn);
            self.power_cut = kill.power_cut;
        }
        let handle = self.handle();
        let session = self.session;
        match act {
            Act::Save(v) => {
                let text = self.token();
                let handle = self.handle();
                match save_verse(handle, 1, v, &text) {
                    Ok(_) => self.saves.push(Save {
                        text,
                        target: Target::Verse(v),
                        session,
                        fate: Fate::Kept,
                        shown: false,
                        at_ms: self.elapsed_ms,
                    }),
                    Err(e) => self.refused_save(&format!("the save of 1:{v}"), e)?,
                }
            }
            Act::Section(v) => {
                let texts = [self.token(), self.token()];
                let handle = self.handle();
                let mut shown = Vec::new();
                for (i, text) in texts.iter().enumerate() {
                    let key = verse_key(v + i as u16);
                    let Some(Record::Verse {
                        value:
                            VerseValue::Text {
                                marker,
                                layout,
                                last,
                                from,
                                ..
                            },
                        ..
                    }) = handle.snapshot().fold().projected(&key)
                    else {
                        return Ok(());
                    };
                    shown.push((
                        handle.mint(&key, None).map_err(|e| format!("{e:?}"))?,
                        Some(SectionValue::Verse(VerseValue::Text {
                            marker,
                            words: text.clone(),
                            layout,
                            last,
                            from,
                        })),
                    ));
                }
                let saved = handle
                    .submit(Command::SaveSection {
                        book: TIT,
                        shown,
                        layout: None,
                    })
                    .wait();
                match saved {
                    Ok(_) => {
                        for (i, text) in texts.into_iter().enumerate() {
                            self.saves.push(Save {
                                text,
                                target: Target::Verse(v + i as u16),
                                session,
                                fate: Fate::Kept,
                                shown: false,
                                at_ms: self.elapsed_ms,
                            });
                        }
                    }
                    Err(e) => self.refused_save(&format!("the section save of 1:{v}"), e)?,
                }
            }
            Act::Name => {
                let text = self.token();
                let handle = self.handle();
                let key = RecordKey::ProjectMeta(MetaField::Name);
                let value = tc_journal::Json::Obj(tc_journal::JsonMap::from([(
                    "en".to_owned(),
                    tc_journal::Json::from(text.as_str()),
                )]));
                let saved = handle
                    .submit(Command::SetMeta {
                        shown: handle.mint(&key, None).map_err(|e| format!("{e:?}"))?,
                        value: Some(value),
                    })
                    .wait();
                match saved {
                    Ok(_) => self.saves.push(Save {
                        text,
                        target: Target::Name,
                        session,
                        fate: Fate::Kept,
                        shown: false,
                        at_ms: self.elapsed_ms,
                    }),
                    Err(e) => self.refused_save("the save of the name", e)?,
                }
            }
            Act::Leave => {
                let _ = handle.leave(Leave::Book(TIT)).wait();
            }
            Act::Checkpoint => {
                let done = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
                // A seal stops for a journal file another program holds, damaged or removed;
                // nothing else in the model stops a checkpoint.
                let excused = self.outside || self.journal_held || !self.locked.is_empty();
                match done {
                    Ok(report) => match report.skipped {
                        None | Some(CheckpointSkip::NoChanges) => {}
                        Some(CheckpointSkip::SealFailed | CheckpointSkip::PackMissing)
                            if excused => {}
                        Some(skip) => self.refused("the checkpoint", skip)?,
                    },
                    Err(e) => self.refused("the checkpoint", e)?,
                }
            }
            Act::Close => {
                if kill.is_none() {
                    self.observe_live_projection();
                } else {
                    self.observe_notices(&self.fs());
                }
                let notices = self.handle().subscribe();
                let handle = self.handle.take().expect("open");
                let closed = handle.close();
                let fs = if self.fs().is_dead() {
                    self.fs().crash(self.power_cut)
                } else {
                    self.fs()
                };
                self.observe_notice_events(&fs, notices.try_iter());
                if self.died() {
                    return Ok(());
                }
                closed.map_err(|e| format!("the close failed: {e:?}"))?;
                for save in &mut self.saves {
                    if save.fate == Fate::InMemory {
                        save.fate = Fate::Kept;
                    }
                }
                // The process ends; the next one sees the files as they are, and the files
                // other programs hold stay held.
                let fs = self.fs().crash(false);
                self.restart_on(fs);
                self.relock();
                self.log.push("closed".into());
                return Ok(());
            }
        }
        self.settle()
    }

    /// Waits for the store's work, and checks it unless the process died (or the step checks
    /// once it has accounted for what another program did meanwhile).
    fn settle(&mut self) -> Result<(), String> {
        if !self.fs().is_dead() {
            self.handle().wait_idle();
        }
        if self.died() || self.check_later {
            return Ok(());
        }
        self.check(false)
    }

    /// The files other programs hold, held again on a new process's filesystem.
    fn relock(&self) {
        let fs = self.fs();
        for (rel, access) in &self.locked {
            fs.set_unreadable(&self.path(rel), access.denies_read());
            fs.set_busy(&self.path(rel), access.denies_replacement());
        }
    }

    /// The `which`-th journal file of a kind in the order the run first saw them, relative to
    /// the folder, with its bytes.
    fn journal_file(&self, journal: Journal, which: u8) -> Option<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = files(&self.fs(), &self.folder)
            .into_iter()
            .filter(|(rel, _)| match journal {
                Journal::Pending => rel.starts_with(".tc/pending/") && !rel.contains(".~"),
                Journal::Pack => rel.starts_with("ingredients/tc/changes/") && !rel.contains(".~"),
            })
            .collect();
        files.sort_by_key(|(rel, _)| {
            self.journal_seen
                .iter()
                .position(|seen| seen == rel)
                .unwrap_or(usize::MAX)
        });
        if files.is_empty() {
            return None;
        }
        Some(files[usize::from(which) % files.len()].clone())
    }

    fn lock(&mut self, what: Holdable, on: bool) -> Result<(), String> {
        let (rels, access): (Vec<String>, DeniedAccess) = match (what, on) {
            (Holdable::File(file), _) => {
                (vec![file.rel().to_owned()], DeniedAccess::ReadAndReplace)
            }
            (Holdable::Pending { which, readable }, true) => (
                self.journal_file(Journal::Pending, which)
                    .map(|(rel, _)| rel)
                    .into_iter()
                    .collect(),
                if readable {
                    DeniedAccess::Replace
                } else {
                    DeniedAccess::ReadAndReplace
                },
            ),
            (Holdable::Pending { .. }, false) => (
                self.locked
                    .keys()
                    .filter(|rel| rel.starts_with(".tc/pending/"))
                    .cloned()
                    .collect(),
                DeniedAccess::ReadAndReplace,
            ),
        };
        for rel in rels {
            if on && access.denies_read() && rel.starts_with(".tc/pending/") {
                // The fold of the session that cannot read it lacks its saves: a book file
                // that shows them is kept as someone's work (R4), a copy that stays.
                self.journal_held = true;
                self.outside = true;
            }
            self.set_access(rel, on.then_some(access));
        }
        if on {
            return Ok(());
        }
        self.retry_after_release()
    }

    fn deny_read(&mut self, file: File, on: bool) -> Result<(), String> {
        let rel = file.rel().to_owned();
        let access = match (self.locked.get(&rel).copied(), on) {
            (None | Some(DeniedAccess::Read), true) => Some(DeniedAccess::Read),
            (Some(DeniedAccess::Replace | DeniedAccess::ReadAndReplace), true) => {
                Some(DeniedAccess::ReadAndReplace)
            }
            (Some(DeniedAccess::Replace | DeniedAccess::ReadAndReplace), false) => {
                Some(DeniedAccess::Replace)
            }
            (None | Some(DeniedAccess::Read), false) => None,
        };
        self.set_access(rel, access);
        if on {
            return Ok(());
        }
        self.retry_after_release()
    }

    fn set_access(&mut self, rel: String, access: Option<DeniedAccess>) {
        let fs = self.fs();
        let path = self.path(&rel);
        fs.set_unreadable(&path, access.is_some_and(DeniedAccess::denies_read));
        fs.set_busy(&path, access.is_some_and(DeniedAccess::denies_replacement));
        match access {
            Some(access) => {
                self.locked.insert(rel, access);
            }
            None => {
                self.locked.remove(&rel);
            }
        }
    }

    fn retry_after_release(&mut self) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        // The writer tries a refused path again within 3 s; a leave does it now.
        let _ = self.handle().leave(Leave::Book(TIT)).wait();
        self.settle()
    }

    /// Another program writes `bytes` at `rel`, with the modification time now or `mtime_ns`.
    fn put(
        &mut self,
        rel: &'static str,
        bytes: Vec<u8>,
        mtime_ns: Option<i64>,
        told: bool,
        what: String,
    ) {
        self.observe_live_projection();
        let fs = self.fs();
        let path = self.path(rel);
        let now = fs.peek(&path);
        if now.as_deref() == Some(bytes.as_slice()) {
            return;
        }
        self.outside = true;
        // Bytes of an earlier put the app has not looked at yet: the person writes over them.
        for put in &mut self.puts {
            if put.rel == rel && !put.retired && now.as_deref() == Some(put.bytes.as_slice()) {
                put.retired = true;
            }
        }
        fs.advance(Duration::from_millis(1500));
        let tell = told && self.handle.is_some();
        self.by_another_program(|| {
            if tell {
                fs.put(&path, &bytes, mtime_ns);
            } else {
                fs.put_unseen(&path, &bytes, mtime_ns);
            }
        });
        // The app's own bytes back again: the app finds the file converged and writes it
        // from the project, which holds all they show (R2).
        let retired = self.app_wrote.get(rel) == Some(&bytes);
        let shows = shown_by(rel, self.app_wrote.get(rel).map(Vec::as_slice), &bytes);
        self.puts.push(Put {
            rel,
            bytes,
            what,
            retired,
            shows,
            unplaced: self.unplaced.contains(rel),
        });
        // A book put can complete a metadata listing before the next deliberate change.
        self.observe_live_projection();
    }

    fn edit(&mut self, v: u16, told: bool) -> Result<(), String> {
        let Some(bytes) = self.fs().peek(&self.path(USFM)) else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let token = self.token();
        let Some(edited) = with_verse_words(&text, v, &token) else {
            return Ok(());
        };
        self.put(
            USFM,
            edited.into_bytes(),
            None,
            told,
            format!("edit of 1:{v} to {token}"),
        );
        self.settle_put(told)
    }

    fn edit_name(&mut self, told: bool) -> Result<(), String> {
        let Some(bytes) = self.fs().peek(&self.path(META)) else {
            return Ok(());
        };
        let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(());
        };
        let token = self.token();
        json["identification"]["name"]["en"] = serde_json::Value::from(token.as_str());
        let edited = serde_json::to_vec_pretty(&json).unwrap();
        self.put(
            META,
            edited,
            None,
            told,
            format!("edit of the name to {token}"),
        );
        self.settle_put(told)
    }

    /// Another program puts back an older version of `file`; `keep_time`: with the time the
    /// run first saw it.
    fn revert(&mut self, file: File, back: u8, told: bool, keep_time: bool) -> Result<(), String> {
        let rel = file.rel();
        let now = self.fs().peek(&self.path(rel));
        let older: Vec<(Vec<u8>, i64)> = self
            .versions
            .get(rel)
            .map(|v| {
                v.iter()
                    .filter(|(b, _)| Some(b.as_slice()) != now.as_deref())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if older.is_empty() {
            return Ok(());
        }
        let (bytes, mtime_ns) = older[older.len() - 1 - usize::from(back) % older.len()].clone();
        let (mtime_ns, what) = if keep_time {
            (Some(mtime_ns), format!("revert of {rel} with its old time"))
        } else {
            (None, format!("revert of {rel}"))
        };
        self.put(rel, bytes, mtime_ns, told, what);
        self.settle_put(told)
    }

    /// Another program deletes a derived file: a deletion is never an edit (I11).
    fn delete(&mut self, file: File) -> Result<(), String> {
        self.observe_live_projection();
        let fs = self.fs();
        let path = self.path(file.rel());
        let Some(now) = fs.peek(&path) else {
            return Ok(());
        };
        // A put the app has not looked at yet: the person deletes it.
        for put in &mut self.puts {
            if put.rel == file.rel() && put.bytes == now {
                put.retired = true;
            }
        }
        self.by_another_program(|| {
            let _ = fs.delete(&path);
        });
        self.unplaced.insert(file.rel());
        self.settle_put(true)
    }

    fn delete_journal(&mut self, journal: Journal, which: u8) -> Result<(), String> {
        let Some((rel, _)) = self.journal_file(journal, which) else {
            return Ok(());
        };
        let fs = self.fs();
        let path = self.path(&rel);
        self.outside = true;
        self.by_another_program(|| {
            let _ = fs.delete(&path);
        });
        self.log.push(format!("deleted {rel}"));
        if self.handle.is_some() {
            self.check(false)
        } else {
            Ok(())
        }
    }

    fn delete_during_seal(&mut self, which: u8, at: u8) -> Result<(), String> {
        self.open()?;
        let Some((rel, _)) = self.journal_file(Journal::Pending, which) else {
            return Ok(());
        };
        let fs = self.fs();
        let path = self.path(&rel);
        let steps = std::sync::Arc::new(std::sync::Mutex::new(0u8));
        self.rig
            .as_ref()
            .expect("a rig")
            .store
            .hooks()
            .at_seal_step(move || {
                let mut steps = steps.lock().unwrap_or_else(|e| e.into_inner());
                *steps += 1;
                if *steps == at {
                    let _ = fs.delete(&path);
                }
            });
        self.outside = true;
        self.log.push(format!("{rel} deleted at seal step {at}"));
        let before = self.copies();
        self.check_later = true;
        let done = self.act(Act::Checkpoint, None);
        self.check_later = false;
        if let Some(rig) = &self.rig {
            rig.store.hooks().at_seal_step(|| {});
        }
        self.lost_copies(before, true);
        done?;
        self.check(false)
    }

    fn flip(&mut self, journal: Journal, which: u8, at: u16) -> Result<(), String> {
        let Some((rel, mut bytes)) = self.journal_file(journal, which) else {
            return Ok(());
        };
        let Some(header) = bytes.iter().position(|b| *b == b'\n').map(|i| i + 1) else {
            return Ok(());
        };
        let body: Vec<usize> = (header..bytes.len())
            .filter(|i| bytes[*i].is_ascii_alphanumeric())
            .collect();
        if body.is_empty() {
            return Ok(());
        }
        let i = body[usize::from(at) % body.len()];
        bytes[i] = if bytes[i] == b'0' { b'1' } else { b'0' };
        let fs = self.fs();
        let path = self.path(&rel);
        self.outside = true;
        // In place: the handles the running session opened read the new byte too, so it
        // holds the line only if it kept the bytes its open verified (§5.11 K1, §4.4).
        self.by_another_program(|| fs.change_in_place(&path, &bytes));
        self.log.push(format!("changed byte {i} of {rel}"));
        if self.handle.is_some() {
            self.check(false)
        } else {
            Ok(())
        }
    }

    /// `.tc/` loses the manifest or the heads files while the app is closed.
    fn lose_tc(&mut self, heads: bool) -> Result<(), String> {
        if self.handle.is_some() {
            return Ok(());
        }
        let fs = self.fs();
        let gone: Vec<PathBuf> = if heads {
            fs.files_under(&self.path(".tc/heads"))
                .into_keys()
                .collect()
        } else {
            vec![self.path(MANIFEST)]
        };
        for path in gone {
            if fs.delete(&path).is_ok() {
                self.outside = true;
                if !heads {
                    self.unplaced.extend([USFM, META]);
                    for put in &mut self.puts {
                        put.unplaced |= !put.retired;
                    }
                }
            }
        }
        Ok(())
    }

    /// The person settles the first held file.
    /// After a put while open, the store's watcher may act on it.
    fn settle_put(&mut self, told: bool) -> Result<(), String> {
        if told && self.handle.is_some() {
            std::thread::sleep(Duration::from_millis(60));
            return self.settle();
        }
        Ok(())
    }

    fn focus(&mut self) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        let _ = self.handle().focus().wait();
        self.settle()
    }

    fn cut(&mut self, journal: Journal, which: u8, keep: u8, at_line: bool) -> Result<(), String> {
        let Some((rel, bytes)) = self.journal_file(journal, which) else {
            return Ok(());
        };
        let fs = self.fs();
        let path = self.path(&rel);
        let ends: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        if ends.len() < 2 {
            return Ok(());
        }
        // Keep the header and at least one line less than the file has.
        let kept_lines = 1 + usize::from(keep) % (ends.len() - 1);
        let len = if at_line {
            ends[kept_lines - 1]
        } else {
            let start = ends[kept_lines - 1];
            start + (ends[kept_lines] - start) / 2
        };
        let open = self.handle.is_some();
        self.outside = true;
        let before = self.copies();
        if open {
            fs.cut_in_place(&path, len);
        } else {
            fs.put_unseen(&path, &bytes[..len], None);
        }
        self.lost_copies(before, false);
        self.log
            .push(format!("cut {rel} to {len} of {} bytes", bytes.len()));
        if open { self.check(false) } else { Ok(()) }
    }

    /// Another program does `f` to the folder: a save whose every copy on disk it takes is
    /// destroyed (or held only by the session that saved it, which must write it again), and
    /// a save no file in the folder shows any more is no longer the app's to take from one.
    fn by_another_program(&mut self, f: impl FnOnce()) {
        let before = self.copies();
        f();
        self.lost_copies(before, true);
    }

    /// For each save: whether a file in the folder holds it, whether a journal file does, and
    /// whether the running session's fold holds it (its own save, or one read at its open).
    fn copies(&self) -> [Vec<bool>; 3] {
        [
            self.saves.iter().map(|s| self.on_disk(&s.text)).collect(),
            self.saves
                .iter()
                .map(|s| self.in_journal(&s.text))
                .collect(),
            self.saves
                .iter()
                .map(|s| self.handle.is_some() && self.in_history(s))
                .collect(),
        ]
    }

    /// The saves whose copies another program took since `copies` (see
    /// [`Run::by_another_program`]). `readable`: the running session can still read the bytes
    /// it read at its open (the program replaced or removed the file; a cut in place takes
    /// them from the session too, so it holds only the saves it made itself).
    fn lost_copies(&mut self, [before, journal_before, held]: [Vec<bool>; 3], readable: bool) {
        let after: Vec<bool> = self.saves.iter().map(|s| self.on_disk(&s.text)).collect();
        let journal_after: Vec<bool> = self
            .saves
            .iter()
            .map(|s| self.in_journal(&s.text))
            .collect();
        let shown: Vec<bool> = self.saves.iter().map(|s| self.in_files(&s.text)).collect();
        let open = self.handle.is_some();
        let held_after: Vec<bool> = self
            .saves
            .iter()
            .map(|s| open && self.in_history(s))
            .collect();
        let session = self.session;
        let elapsed = self.elapsed_ms;
        for (i, save) in self.saves.iter_mut().enumerate() {
            save.shown &= shown[i];
            if save.fate == Fate::InMemory && !readable && save.session != session && !held_after[i]
            {
                // A change in place took the bytes the session read at its open: it holds
                // the save no more, and only a file still showing it keeps it.
                save.fate = if after[i] {
                    Fate::Kept
                } else {
                    Fate::Destroyed
                };
                continue;
            }
            if save.fate != Fate::Kept {
                continue;
            }
            if open && (save.session == session || (readable && held[i])) {
                // The running session holds it (it saved it, or read it at its open), and
                // writes it again at its next seal (§5.11 K1); until then it may write over a
                // file that shows it. A kill may take it only while it is at most 2 minutes
                // old (ruling 2026-10-22-acceptable-loss): an older save is sealed already.
                if journal_before[i] && !journal_after[i] && elapsed - save.at_ms <= 120_000 {
                    save.fate = Fate::InMemory;
                }
            } else if before[i] && !after[i] {
                save.fate = Fate::Destroyed;
            }
        }
    }

    /// Whether a verified line of a journal file holds `text`, or a pack of HEAD's tree the app
    /// restores.
    fn in_journal(&self, text: &str) -> bool {
        let fs = self.fs();
        let project = project_of(&fs, &self.folder);
        let in_files = files(&fs, &self.folder).into_iter().any(|(rel, bytes)| {
            let entries = if rel.starts_with(".tc/pending/") {
                tc_journal::pending::read_pending(&bytes, project).entries
            } else if rel.starts_with("ingredients/tc/changes/") && !rel.contains(".~") {
                tc_journal::pack::read_pack(&rel, &bytes, project).entries
            } else {
                return false;
            };
            entries.iter().any(|e| contains(e.raw(), text))
        });
        in_files || self.in_git(text)
    }

    /// Whether a file in the folder holds `text`: a verified line of a journal file (a pack
    /// HEAD holds is restored by the app, so a cut one still counts), or
    /// another file's bytes. Quarantined files, the fold cache and temporary files of derived
    /// files do not count, nor do copies in `.tc/external/`: no open reads them, and one the
    /// person was told about is credited as a notified copy instead.
    fn on_disk(&self, text: &str) -> bool {
        let fs = self.fs();
        let project = project_of(&fs, &self.folder);
        for (rel, bytes) in files(&fs, &self.folder) {
            if rel.starts_with(".tc/quarantine/")
                || rel.starts_with(".tc/external/")
                || rel == ".tc/index.bin"
            {
                continue;
            }
            if rel.starts_with(".tc/pending/") {
                let read = tc_journal::pending::read_pending(&bytes, project);
                if read.entries.iter().any(|e| contains(e.raw(), text)) {
                    return true;
                }
            } else if rel.starts_with("ingredients/tc/changes/") {
                if rel.contains(".~") {
                    continue;
                }
                let read = tc_journal::pack::read_pack(&rel, &bytes, project);
                if read.entries.iter().any(|e| contains(e.raw(), text)) {
                    return true;
                }
            } else if !rel.rsplit('/').next().unwrap_or("").starts_with(".~")
                && contains(&bytes, text)
            {
                return true;
            }
        }
        self.in_git(text)
    }

    /// The files of HEAD's tree, read from Git itself.
    fn head_files(&self) -> BTreeMap<String, Vec<u8>> {
        let fs = self.fs();
        if !git_dir_of(&fs, &self.folder).exists() {
            return BTreeMap::new();
        }
        let repo = repo_of(&fs, &self.folder);
        commits_of(&repo)
            .first()
            .map(|head| commit_files(&repo, *head))
            .unwrap_or_default()
    }

    /// Whether a pack of HEAD's tree holds `text`: the app restores a damaged pack of the
    /// expected set from it (§4.4).
    fn in_git(&self, text: &str) -> bool {
        let fs = self.fs();
        if !git_dir_of(&fs, &self.folder).exists() {
            return false;
        }
        let repo = repo_of(&fs, &self.folder);
        let Some(head) = commits_of(&repo).first().copied() else {
            return false;
        };
        commit_files(&repo, head)
            .iter()
            .any(|(rel, bytes)| rel.starts_with("ingredients/tc/changes/") && contains(bytes, text))
    }

    /// Whether a derived file in the folder shows `text`.
    fn in_files(&self, text: &str) -> bool {
        let fs = self.fs();
        [USFM, META]
            .iter()
            .any(|rel| fs.peek(&self.path(rel)).is_some_and(|b| contains(&b, text)))
    }

    fn text_in_external(&self, text: &str) -> bool {
        self.fs()
            .files_under(&self.path(".tc/external"))
            .values()
            .any(|b| contains(b, text))
    }

    /// Whether a version of the save's record in History holds its text, readable.
    fn in_history(&self, save: &Save) -> bool {
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        let key = match save.target {
            Target::Verse(v) => verse_key(v),
            Target::Name => RecordKey::ProjectMeta(MetaField::Name),
        };
        fold.history(&key)
            .iter()
            .any(|id| match fold.body(id).map(|b| b.record) {
                Some(Record::Verse {
                    value: VerseValue::Text { words, .. },
                    ..
                }) => words == save.text,
                Some(Record::ProjectMeta {
                    value: Some(value), ..
                }) => format!("{value:?}").contains(&save.text),
                _ => false,
            })
    }

    /// Whether the project took the put in as an outside edit (§11.1 X4): a readable change
    /// History lists names these exact bytes as its source, and every value the put
    /// brought is a readable version in History. A notice of the recording may be gone
    /// with a crash; History is where the person finds it.
    fn recorded_outside(&self, put: &Put) -> bool {
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        let seen = tc_journal::Md5::of(&put.bytes);
        let from_these_bytes = fold.record_keys().iter().any(|key| {
            fold.history(key).iter().any(|id| {
                fold.body(id).is_some_and(|body| {
                    matches!(
                        &body.cause,
                        tc_journal::Cause::External {
                            source: tc_journal::ExternalSource::LocalFile { path, seen_md5 },
                            ..
                        } if path.as_str() == put.rel && *seen_md5 == seen
                    )
                })
            })
        });
        from_these_bytes
            && put.shows.iter().all(|shown| {
                let (key, text) = match shown {
                    Shown::Verse(v, words) => (verse_key(*v), words),
                    Shown::Name(name) => (RecordKey::ProjectMeta(MetaField::Name), name),
                };
                fold.history(&key)
                    .iter()
                    .any(|id| match fold.body(id).map(|b| b.record) {
                        Some(Record::Verse {
                            value: VerseValue::Text { words, .. },
                            ..
                        }) => words == *text,
                        Some(Record::ProjectMeta {
                            value: Some(value), ..
                        }) => format!("{value:?}").contains(text.as_str()),
                        _ => false,
                    })
            })
    }

    /// Whether the project shows every value the put brought (§11.1 X3 rule 7: a value the
    /// project shows already is not a change), so nothing in it can be lost.
    fn shown_by_the_project(&self, put: &Put) -> bool {
        // The model edits only the name of `metadata.json`; any other field a put of it
        // shows is one the app derived or wrote, so a put that brings no name brings
        // nothing of the person's. A book file's other lines need a recording or a notice.
        if put.rel == META && put.shows.is_empty() {
            return true;
        }
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        !put.shows.is_empty()
            && put.shows.iter().all(|shown| match shown {
                Shown::Verse(v, words) => matches!(
                    fold.projected(&verse_key(*v)),
                    Some(Record::Verse { value: VerseValue::Text { words: w, .. }, .. }) if w == *words
                ),
                Shown::Name(name) => matches!(
                    fold.projected(&RecordKey::ProjectMeta(MetaField::Name)),
                    Some(Record::ProjectMeta { value: Some(value), .. }) if format!("{value:?}").contains(name.as_str())
                ),
            })
    }

    /// Whether the app, knowing no placement of the path, takes HEAD's file as the base
    /// (R14) and the put shows nothing but HEAD's values and the project's: it records
    /// nothing, and every value it shows is one a commit or the project holds.
    fn head_based(&self, put: &Put) -> bool {
        if !put.unplaced {
            return false;
        }
        let Some(head) = self.head_files().remove(put.rel) else {
            return false;
        };
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        let head_verses = chapter_one(&head);
        put.shows.iter().all(|shown| match shown {
            Shown::Verse(v, words) => {
                head_verses.get(v) == Some(words)
                    || matches!(
                        fold.projected(&verse_key(*v)),
                        Some(Record::Verse { value: VerseValue::Text { words: w, .. }, .. }) if w == *words
                    )
            }
            Shown::Name(name) => {
                name_shown(&head).as_ref() == Some(name)
                    || matches!(
                        fold.projected(&RecordKey::ProjectMeta(MetaField::Name)),
                        Some(Record::ProjectMeta { value: Some(value), .. }) if format!("{value:?}").contains(name.as_str())
                    )
            }
        })
    }

    /// Whether a change the fold holds and has not applied (its batch lost a member, §7.4)
    /// holds the save's text, and the fold lists its batch as waiting.
    fn waiting(&self, save: &Save) -> bool {
        let fs = self.fs();
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        let project = project_of(&fs, &self.folder);
        let waiting: Vec<tc_journal::BatchId> = fold
            .anomalies()
            .filter_map(|a| match a {
                tc_journal::fold::Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
                _ => None,
            })
            .collect();
        for (rel, bytes) in files(&fs, &self.folder) {
            let entries = if rel.starts_with(".tc/pending/") {
                tc_journal::pending::read_pending(&bytes, project).entries
            } else if rel.starts_with("ingredients/tc/changes/") && !rel.contains(".~") {
                tc_journal::pack::read_pack(&rel, &bytes, project).entries
            } else {
                continue;
            };
            for entry in entries {
                if contains(entry.raw(), &save.text)
                    && let Some(view) = fold.change(&entry.id())
                    && waiting.contains(&view.batch)
                {
                    return true;
                }
            }
        }
        false
    }

    /// With no outside step yet, the app kept nothing twice: no outside-edit notice and no copy
    /// in `.tc/external/`, except of the bytes a write under way replaced (R9).
    fn kept_nothing_twice(&self, lost: &mut Vec<String>) {
        if self.outside {
            return;
        }
        let fs = self.fs();
        for (path, bytes) in fs.files_under(&self.path(".tc/external")) {
            if !self.replaced.contains(&bytes) {
                lost.push(format!(
                    "a copy {} with no outside step: the app kept its own bytes twice",
                    path.display()
                ));
            }
        }
        for notice in self.handle().notices() {
            if !outside_notice(&notice.notice) {
                continue;
            }
            let explained = match &notice.copy {
                Some(copy) => fs
                    .peek(&self.path(copy))
                    .is_some_and(|b| self.replaced.contains(&b)),
                None => !self.replaced.is_empty(),
            };
            if !explained {
                lost.push(format!(
                    "the notice {:?} with no outside step: the app took its own bytes for \
                     someone's",
                    notice.notice
                ));
            }
        }
    }

    /// The no-loss invariant (see the module documentation). `settled`: an open and its first
    /// job are done, so a save shown only by a file in the folder is not kept.
    fn check(&mut self, settled: bool) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        self.observe_live_projection();
        let mut lost = Vec::new();
        self.kept_nothing_twice(&mut lost);
        let fs = self.fs();
        let held_files: Vec<&str> = [USFM, META]
            .into_iter()
            .filter(|rel| matches!(self.ledger(rel), Some(State::Held { .. })))
            .collect();
        // A claimed hold imposes a notice obligation; it never proves preservation.
        for rel in &held_files {
            let Some(State::Held { held }) = self.ledger(rel) else {
                continue;
            };
            let shown = self.observed_copies.iter().any(|seen| {
                seen.source == *rel
                    && seen.copy == held.copy
                    && md5(&seen.bytes) == held.md5
                    && self.observed_copy_exists(&fs, seen)
            });
            if !shown {
                lost.push(format!(
                    "held {rel}: no observed notice leads to its complete copy {}",
                    held.copy
                ));
            }
        }
        for save in &self.saves {
            if save.fate == Fate::Destroyed {
                continue;
            }
            if self.in_history(save) {
                continue;
            }
            if !save.shown && (self.waiting(save) || self.save_in_notified_copy(save)) {
                continue;
            }
            // A file showing it that the app holds as it is, with a copy and a notice, until
            // the person takes it in or restores it (§6.2 held).
            if self.save_in_notified_copy(save)
                && held_files.iter().any(|rel| {
                    fs.peek(&self.path(rel))
                        .is_some_and(|b| contains(&b, &save.text))
                })
            {
                continue;
            }
            // While a session runs, a copy in the folder the next open reads (a journal line,
            // a pack HEAD holds, a file showing it) is not lost yet.
            if !settled && self.on_disk(&save.text) {
                continue;
            }
            // A file another program holds is not read yet: what it shows is not lost yet. Nor
            // is what a derived file shows while a journal file is held: the app cannot tell
            // whether the save is in it, and leaves the file as it is (R6).
            if self.locked.keys().any(|rel| {
                fs.peek(&self.folder.join(rel))
                    .is_some_and(|b| contains(&b, &save.text))
            }) {
                continue;
            }
            // `metadata.json` lists the book file's bytes (M6): while another program holds
            // the book file, the app decides metadata.json later and leaves it as it is.
            if self.locked.contains_key(USFM)
                && fs
                    .peek(&self.path(META))
                    .is_some_and(|b| contains(&b, &save.text))
            {
                continue;
            }
            if save.fate == Fate::InMemory && !settled {
                continue;
            }
            // Work the journal lost that a file the app wrote still shows: it is kept in a copy
            // listed with *Bring it back* until the person answers (H22 part 1, row N4).
            if self.in_lost_copy(&fs, save) {
                continue;
            }
            lost.push(format!(
                "the save {} of {:?} (session {}, {:?}, shown: {}): not in History{}",
                save.text,
                save.target,
                save.session,
                save.fate,
                save.shown,
                if self.text_in_external(&save.text) {
                    " (a copy in .tc/external has it)"
                } else {
                    ""
                }
            ));
        }
        let external: Vec<(String, Vec<u8>)> = self
            .observed_copies
            .iter()
            .filter(|seen| self.observed_copy_exists(&fs, seen))
            .map(|seen| (seen.source.clone(), seen.bytes.clone()))
            .collect();
        let credited: Vec<bool> = self
            .puts
            .iter()
            .map(|put| {
                !put.retired
                    && (self.recorded_outside(put)
                        || self.head_based(put)
                        || self.shown_by_the_project(put))
            })
            .collect();
        for (put, credited) in self.puts.iter_mut().zip(credited) {
            if put.retired {
                continue;
            }
            if credited {
                // The put's values are in the project. Whether the app now counts its bytes as
                // its own placement is the app's read to make, not this check's: the run's
                // own copy of what the app wrote (`app_wrote`) changes only when the app is
                // seen to write, or the folder to show the projection.
                put.retired = true;
                continue;
            }
            let on_disk =
                fs.peek(&self.folder.join(put.rel)).as_deref() == Some(put.bytes.as_slice());
            if on_disk
                || external
                    .iter()
                    .any(|(source, bytes)| source == put.rel && *bytes == put.bytes)
            {
                continue;
            }
            lost.push(format!(
                "the {}: not in the folder or complete projection, no notified full copy in .tc/external",
                put.what
            ));
        }
        if lost.is_empty() {
            return Ok(());
        }
        // What the app knew, for the reader of a failure.
        let snapshot = self.handle().snapshot();
        let shown: Vec<String> = (1u16..=5)
            .map(|v| match snapshot.fold().projected(&verse_key(v)) {
                Some(Record::Verse {
                    value: VerseValue::Text { words, .. },
                    ..
                }) => format!("1:{v} {words}"),
                other => format!("1:{v} {other:?}"),
            })
            .collect();
        drop(snapshot);
        lost.push(format!(
            "projection: {shown:?}\nledger: {USFM} {:?}; {META} {:?}\nexternal: {:?}\nnotices: {:?}",
            self.ledger(USFM),
            self.ledger(META),
            fs.files_under(&self.path(".tc/external"))
                .into_keys()
                .collect::<Vec<_>>(),
            self.handle()
                .notices()
                .into_iter()
                .map(|n| n.notice)
                .collect::<Vec<_>>()
        ));
        Err(lost.join("\n"))
    }

    /// Notes the versions of the files the folder holds now, and the journal files.
    fn see_versions(&mut self) {
        let fs = self.fs();
        for rel in files(&fs, &self.folder).into_keys() {
            if (rel.starts_with(".tc/pending/") || rel.starts_with("ingredients/tc/changes/"))
                && !rel.contains(".~")
                && !self.journal_seen.contains(&rel)
            {
                self.journal_seen.push(rel);
            }
        }
        for rel in [USFM, META] {
            let now = fs.peek(&self.path(rel));
            if self.last_seen.get(rel) != Some(&now) {
                // Bytes no other program put there are the app's: the run's own copy of
                // what it wrote, never the ledger's word for it.
                if let Some(bytes) = &now
                    && !self.puts.iter().any(|p| p.rel == rel && p.bytes == *bytes)
                {
                    self.app_wrote.insert(rel, bytes.clone());
                    self.app_versions
                        .entry(rel)
                        .or_default()
                        .push(bytes.clone());
                    self.unplaced.remove(rel);
                }
                self.last_seen.insert(rel, now);
            }
            if let Some(bytes) = fs.peek(&self.path(rel)) {
                let mtime_ns = tc_store::Vfs::metadata(&fs, &self.path(rel))
                    .map_or(fs.now_ns(), |m| m.mtime_ns);
                let seen = self.versions.entry(rel).or_default();
                if !seen.iter().any(|(b, _)| *b == bytes) {
                    seen.push((bytes, mtime_ns));
                }
            }
        }
    }

    pub fn step(&mut self, step: &Step) -> Result<(), String> {
        self.log.push(format!("{step:?}"));
        let done = match step {
            Step::Act { act, kill } => self.act(*act, *kill),
            Step::KillAfterRename { file } => {
                self.fs().kill_after_rename(&self.path(file.rel()));
                self.power_cut = false;
                Ok(())
            }
            Step::Lock { what, on } => self.lock(*what, *on),
            Step::DenyRead { file, on } => self.deny_read(*file, *on),
            Step::Open => self.open(),
            Step::Edit { verse, told } => self.edit(*verse, *told),
            Step::EditName { told } => self.edit_name(*told),
            Step::Revert { file, back, told } => self.revert(*file, *back, *told, false),
            Step::RevertKeepTime { file, back } => self.revert(*file, *back, false, true),
            Step::Delete { file } => self.delete(*file),
            Step::Touch { file } => {
                let fs = self.fs();
                let _ = fs.set_mtime(&self.path(file.rel()), fs.now_ns());
                Ok(())
            }
            Step::Cut {
                journal,
                which,
                keep,
                at_line,
            } => self.cut(*journal, *which, *keep, *at_line),
            Step::DeleteJournal { journal, which } => self.delete_journal(*journal, *which),
            Step::DeleteDuringSeal { which, at } => self.delete_during_seal(*which, *at),
            Step::Flip { journal, which, at } => self.flip(*journal, *which, *at),
            Step::LoseTc { heads } => self.lose_tc(*heads),
            Step::Focus => self.focus(),
            Step::Advance => {
                self.fs().advance(Duration::from_secs(10));
                self.elapsed_ms += 10_000;
                self.clock.advance(10_000);
                // The actor's timer fires in that time: a seal that is due runs now.
                if let Some(handle) = &self.handle {
                    handle.wait_idle();
                }
                Ok(())
            }
        };
        self.died();
        self.see_versions();
        if std::env::var_os("TC_MODEL_TRACE").is_some() {
            self.trace(step);
        }
        done
    }

    /// With `TC_MODEL_TRACE` set: what each step left, for the reader of a failure.
    fn trace(&self, step: &Step) {
        let fs = self.fs();
        let short = |b: &[u8]| md5(b)[..6].to_owned();
        let disk: Vec<String> = [USFM, META]
            .iter()
            .map(|rel| fs.peek(&self.path(rel)).map_or("-".into(), |b| short(&b)))
            .collect();
        let external: Vec<String> = fs
            .files_under(&self.path(".tc/external"))
            .values()
            .map(|b| short(b))
            .collect();
        let puts: Vec<String> = self
            .puts
            .iter()
            .map(|p| format!("{}{}", short(&p.bytes), if p.retired { "r" } else { "" }))
            .collect();
        let journal: Vec<String> = files(&fs, &self.folder)
            .into_iter()
            .filter(|(rel, _)| {
                rel.starts_with(".tc/pending/")
                    || rel.starts_with(".tc/quarantine/")
                    || rel.starts_with("ingredients/tc/changes/")
            })
            .map(|(rel, bytes)| {
                let texts: Vec<&str> = self
                    .saves
                    .iter()
                    .filter(|save| contains(&bytes, &save.text))
                    .map(|save| save.text.as_str())
                    .collect();
                format!(
                    "{}:{texts:?}",
                    rel.rsplit('/')
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(12)
                        .collect::<String>()
                )
            })
            .collect();
        eprintln!("  journal {journal:?}");
        eprintln!(
            "TRACE {step:?}\n  disk {disk:?} ledger {:?} / {:?}\n  external {external:?} puts {puts:?}",
            self.ledger(USFM)
                .map(|s| format!("{s:?}").chars().take(90).collect::<String>()),
            self.ledger(META)
                .map(|s| format!("{s:?}").chars().take(90).collect::<String>()),
        );
    }

    /// The end of a run: a close if the project is open, an open, a leave, and the check.
    pub fn finish(&mut self) -> Result<(), String> {
        if self.handle.is_some() {
            self.step(&Step::Act {
                act: Act::Close,
                kill: None,
            })?;
        }
        // Every program lets go of the files in the end.
        let fs = self.fs();
        for rel in std::mem::take(&mut self.locked).into_keys() {
            fs.set_unreadable(&self.path(&rel), false);
            fs.set_busy(&self.path(&rel), false);
        }
        self.log.push("the last open".into());
        if std::env::var_os("TC_MODEL_TRACE").is_some() {
            self.trace(&Step::Open);
        }
        self.open()?;
        // The last look is not killed.
        self.fs().disarm();
        let repaired = self.handle().leave(Leave::Book(TIT)).wait();
        self.handle().wait_idle();
        self.check(true)?;
        let snapshot = self.handle().snapshot();
        let fold = snapshot.fold();
        let before = fold.values_unreadable();
        for file in fold.derived_files() {
            let _ = fold.render(&file);
        }
        let metadata_readable = fold
            .metadata_heads()
            .into_iter()
            .flat_map(|(_, heads)| heads)
            .all(|id| fold.body(&id).is_some());
        let readable = metadata_readable
            && fold.values_unreadable() == before
            && fold.unreadable_records().is_empty();
        drop(snapshot);
        let writable = matches!(self.handle().facts().mode, Mode::Writable);
        let ready = repaired.is_ok() && writable && readable;
        count_terminal_boundary(ready);
        self.log.push(format!(
            "terminal Burrito boundary: repair={repaired:?}, writable={writable}, readable={readable}"
        ));
        if ready {
            let problems = burrito_problems(&self.fs(), &self.folder);
            if !problems.is_empty() {
                return Err(format!(
                    "terminal repaired Burrito is invalid: {problems:?}"
                ));
            }
        }
        self.observe_live_projection();
        let notices = self.handle().subscribe();
        if let Some(handle) = self.handle.take() {
            let closed = handle.close();
            let fs = self.fs();
            self.observe_notice_events(&fs, notices.try_iter());
            closed.map_err(|e| format!("the last close: {e:?}"))?;
        }
        if ready {
            let problems = burrito_problems(&self.fs(), &self.folder);
            if !problems.is_empty() {
                return Err(format!("terminal closed Burrito is invalid: {problems:?}"));
            }
        }
        Ok(())
    }

    /// Runs `steps` and the end, and says what was lost.
    pub fn run(steps: &[Step]) -> Result<(), String> {
        if std::env::var_os("TC_MODEL_TRACE").is_some() {
            eprintln!("MODEL HISTORY {steps:#?}");
        }
        let mut run = Self::new();
        let result = steps
            .iter()
            .try_for_each(|step| run.step(step))
            .and_then(|()| run.finish());
        result.map_err(|lost| format!("{lost}\nafter:\n  {}", run.log.join("\n  ")))
    }
}

/// Missing or unknown non-derived fields cannot establish convergence, even if every
/// field the current fold can read happens to match.
fn metadata_fields_accounted_for(
    value: &serde_json::Value,
    path: &str,
    fields: &BTreeMap<String, serde_json::Value>,
) -> bool {
    if tc_burrito::is_derived_field(path) {
        return true;
    }
    if let Some(expected) = fields.get(path) {
        return value == expected;
    }
    // These format defaults carry no authored record. Accept only their exact values;
    // an added field or a different copyright still needs preservation.
    match path {
        "copyright" => {
            return *value == serde_json::json!({"shortStatements": [{"statement": "Not stated"}]});
        }
        "confidential" => return *value == serde_json::Value::Bool(false),
        "localizedNames" => return *value == serde_json::json!({}),
        _ => {}
    }
    match value {
        serde_json::Value::Object(children) if !children.is_empty() => {
            children.iter().all(|(key, child)| {
                if key.contains('.') {
                    return false;
                }
                let path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                metadata_fields_accounted_for(child, &path, fields)
            })
        }
        _ => false,
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        drop(self.handle.take());
    }
}

/// The project's name as the fold shows it.
fn name_of(handle: &ProjectHandle) -> String {
    match handle
        .snapshot()
        .fold()
        .projected(&RecordKey::ProjectMeta(MetaField::Name))
    {
        Some(Record::ProjectMeta {
            value: Some(value), ..
        }) => format!("{value:?}"),
        _ => String::new(),
    }
}

/// `text` with the words of verse 1:v replaced by `words`.
fn with_verse_words(text: &str, v: u16, words: &str) -> Option<String> {
    let chapter = text.find("\\c 1\n")?;
    let marker = format!("\\v {v} ");
    let start = chapter + text[chapter..].find(&marker)? + marker.len();
    let end = start + text[start..].find('\n')?;
    Some(format!("{}{words}{}", &text[..start], &text[end..]))
}

// ---------------------------------------------------------------------------------------
// The generator
// ---------------------------------------------------------------------------------------

fn an_act() -> impl Strategy<Value = Act> {
    prop_oneof![
        4 => (1u16..=4).prop_map(Act::Save),
        2 => (1u16..=3).prop_map(Act::Section),
        1 => Just(Act::Name),
        3 => Just(Act::Leave),
        2 => Just(Act::Checkpoint),
        1 => Just(Act::Close),
    ]
}

/// A kill at any write point of an act: a checkpoint after a save on a fresh project has 51,
/// a close 58.
fn a_kill() -> impl Strategy<Value = Kill> {
    (0u16..80, any::<bool>(), any::<bool>()).prop_map(|(at, power_cut, torn)| Kill {
        at,
        power_cut,
        torn,
    })
}

fn a_file() -> impl Strategy<Value = File> {
    prop_oneof![3 => Just(File::Usfm), 1 => Just(File::Meta)]
}

fn a_journal() -> impl Strategy<Value = Journal> {
    prop_oneof![3 => Just(Journal::Pending), 1 => Just(Journal::Pack)]
}

fn a_holdable() -> impl Strategy<Value = Holdable> {
    prop_oneof![
        2 => a_file().prop_map(Holdable::File),
        1 => (any::<u8>(), any::<bool>())
            .prop_map(|(which, readable)| Holdable::Pending { which, readable }),
    ]
}

/// What another program or the disk does to the folder, unseen by the watcher.
fn a_fault() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (a_journal(), any::<u8>(), any::<u8>(), any::<bool>()).prop_map(
            |(journal, which, keep, at_line)| Step::Cut {
                journal,
                which,
                keep,
                at_line
            }
        ),
        2 => (a_file(), 0u8..3).prop_map(|(file, back)| Step::Revert {
            file,
            back,
            told: false
        }),
        2 => (a_file(), 0u8..3).prop_map(|(file, back)| Step::RevertKeepTime { file, back }),
        1 => a_file().prop_map(|file| Step::Delete { file }),
        1 => (a_journal(), any::<u8>())
            .prop_map(|(journal, which)| Step::DeleteJournal { journal, which }),
        1 => (a_journal(), any::<u8>(), any::<u16>())
            .prop_map(|(journal, which, at)| Step::Flip { journal, which, at }),
        1 => any::<bool>().prop_map(|heads| Step::LoseTc { heads }),
    ]
}

fn a_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        12 => an_act().prop_map(|act| Step::Act { act, kill: None }),
        3 => (an_act(), a_kill()).prop_map(|(act, kill)| Step::Act { act, kill: Some(kill) }),
        2 => a_file().prop_map(|file| Step::KillAfterRename { file }),
        1 => (a_holdable(), any::<bool>()).prop_map(|(what, on)| Step::Lock { what, on }),
        1 => (a_file(), any::<bool>()).prop_map(|(file, on)| Step::DenyRead { file, on }),
        2 => Just(Step::Open),
        2 => (1u16..=4, any::<bool>()).prop_map(|(verse, told)| Step::Edit { verse, told }),
        1 => any::<bool>().prop_map(|told| Step::EditName { told }),
        2 => (a_file(), 0u8..3, any::<bool>())
            .prop_map(|(file, back, told)| Step::Revert { file, back, told }),
        3 => a_fault(),
        1 => a_file().prop_map(|file| Step::Touch { file }),
        1 => Just(Step::Focus),
        2 => Just(Step::Advance),
    ]
}

/// A short sequence of steps: one step, or one of the sequences the reviews found bugs in.
fn a_chunk() -> impl Strategy<Value = Vec<Step>> {
    prop_oneof![
        8 => a_step().prop_map(|step| vec![step]),
        // A crash, then another program's change while the app is closed.
        3 => (an_act(), a_kill(), a_fault()).prop_map(|(act, kill, fault)| {
            vec![Step::Act { act, kill: Some(Kill { at: kill.at % 40, ..kill }) }, fault]
        }),
        2 => a_fault().prop_map(|fault| vec![crash(), fault]),
        // A kill right after a rename, then a revert of that file while closed.
        3 => (a_file(), an_act(), any::<bool>(), 0u8..3, any::<bool>()).prop_map(
            |(file, act, advance, back, keep_time)| {
                let mut steps = vec![
                    Step::KillAfterRename { file },
                    Step::Act { act, kill: None },
                ];
                if advance {
                    steps.push(Step::Advance);
                }
                steps.push(if keep_time {
                    Step::RevertKeepTime { file, back }
                } else {
                    Step::Revert {
                        file,
                        back,
                        told: false,
                    }
                });
                steps
            }
        ),
        // A file held around an open and an act.
        2 => (a_holdable(), an_act()).prop_map(|(what, act)| {
            vec![
                Step::Lock { what, on: true },
                Step::Open,
                Step::Act { act, kill: None },
                Step::Lock { what, on: false },
            ]
        }),
        // A replacement is allowed while the file's reads are denied, including after a crash.
        2 => (a_file(), prop_oneof![(1u16..=4).prop_map(Act::Save), Just(Act::Name)])
            .prop_map(|(file, save)| {
                vec![
                    Step::Open,
                    match file {
                        File::Usfm => Step::Edit { verse: 1, told: false },
                        File::Meta => Step::EditName { told: false },
                    },
                    Step::DenyRead { file, on: true },
                    crash(),
                    Step::Open,
                    act(save),
                    act(Act::Leave),
                    Step::DenyRead { file, on: false },
                ]
            }),
        // A kill right after a rename, and the file held at the next open.
        2 => (a_file(), an_act()).prop_map(|(file, act)| {
            let what = Holdable::File(file);
            vec![
                Step::KillAfterRename { file },
                Step::Act { act, kill: None },
                Step::Act { act: Act::Leave, kill: None },
                Step::Lock { what, on: true },
                Step::Open,
                Step::Act { act: Act::Save(1), kill: None },
                Step::Lock { what, on: false },
            ]
        }),
        // A pending file another program deletes while the app runs, then a checkpoint, or
        // during the checkpoint's seal.
        1 => any::<u8>().prop_map(|which| {
            vec![
                Step::Act { act: Act::Save(1), kill: None },
                Step::DeleteJournal { journal: Journal::Pending, which },
                Step::Act { act: Act::Checkpoint, kill: None },
            ]
        }),
        1 => (any::<u8>(), 1u8..5).prop_map(|(which, at)| {
            vec![
                Step::Act { act: Act::Save(1), kill: None },
                Step::DeleteDuringSeal { which, at },
            ]
        }),
        // A pending file a scanner holds through a checkpoint, then more saves.
        1 => (any::<u8>(), any::<bool>()).prop_map(|(which, readable)| {
            let what = Holdable::Pending { which, readable };
            vec![
                Step::Act { act: Act::Save(1), kill: None },
                Step::Lock { what, on: true },
                Step::Act { act: Act::Checkpoint, kill: None },
                Step::Act { act: Act::Save(2), kill: None },
                Step::Lock { what, on: false },
                Step::Act { act: Act::Save(3), kill: None },
            ]
        }),
        // A pending file the open read cut in place, so a value can no longer be read, and
        // the book file deleted: the job must not rebuild it without the value (R15).
        1 => any::<bool>().prop_map(|told| {
            vec![
                Step::Act { act: Act::Save(1), kill: None },
                Step::Act { act: Act::Leave, kill: None },
                crash(),
                Step::Open,
                Step::Cut { journal: Journal::Pending, which: 0, keep: 0, at_line: false },
                if told {
                    Step::Delete { file: File::Usfm }
                } else {
                    Step::Focus
                },
                Step::Delete { file: File::Usfm },
                Step::Act { act: Act::Leave, kill: None },
            ]
        }),
    ]
}

fn a_history(chunks: usize) -> impl Strategy<Value = Vec<Step>> {
    proptest::collection::vec(a_chunk(), 1..chunks)
        .prop_map(|chunks| chunks.into_iter().flatten().collect())
}

/// The rows and events of the ledger's table (§6.2) the model cannot reach, and the tests
/// that take them: R1 needs a save served while a checkpoint's job runs
/// (`a_checkpoint_job_never_places_an_older_version_over_a_newer_edit`); R7, R17 and Removed
/// a book removed (`a_book_is_added_with_stubs_removed_with_what_was_seen_and_restored_either_way`).
/// R3, R10, R13, Kept, TakeIn and Restore need a held book file or `metadata.json`, which only
/// R10's hold for disk damage made: the 2026-10-21 rulings remove R10 and *Take in* /
/// *Restore* (docs/STORAGE-DELTA-2026-10-21.md §3), and S3 step 9 deletes them.
const ROWS_OUT_OF_REACH: [&str; 10] = [
    "R1", "R3", "R7", "R10", "R13", "R17", "Kept", "Removed", "TakeIn", "Restore",
];

fn histories(cases: u32, chunks: usize, every_row: bool) {
    let terminal_before = terminal_boundaries();
    // The histories the reviews found, on every run.
    held_metadata_retries_keep_the_same_copies_and_notices();
    for (name, history) in named_histories() {
        if let Err(lost) = run_to_a_validated_end(&history) {
            panic!("{name}: {lost}");
        }
    }
    let config = ProptestConfig {
        cases,
        max_shrink_iters: 2000,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    proptest!(config, |(history in a_history(chunks))| {
        if let Err(lost) = Run::run(&history) {
            return Err(TestCaseError::fail(lost));
        }
    });
    let terminal_after = terminal_boundaries();
    let ready = terminal_after[0] - terminal_before[0];
    let blocked = terminal_after[1] - terminal_before[1];
    eprintln!("terminal Burrito boundaries: ready={ready}, blocked={blocked}");
    assert!(ready > 0, "no terminal repair boundary validated a Burrito");
    let hits = tc_store::ledger::hits();
    if std::env::var_os("TC_MODEL_ROWS").is_some() {
        eprintln!("ledger rows and events taken: {hits:?}");
    }
    if every_row {
        let missed: Vec<&str> = hits
            .iter()
            .filter(|(name, n)| **n == 0 && !ROWS_OUT_OF_REACH.contains(name))
            .map(|(name, _)| *name)
            .collect();
        assert!(missed.is_empty(), "rows and events never taken: {missed:?}");
    }
}

/// INV-NO-OWN-SAVE-LOST (I2), §16.1: random histories of saves, materializations and
/// checkpoints killed at any write point, outside edits and reverts, damaged journal files and restarts lose no work.
#[test]
fn random_histories_lose_no_work() {
    histories(200, 8, false);
}

/// The nightly variant: more histories, and longer ones, which take every row of the
/// ledger's table the model can reach.
#[test]
#[ignore = "long: run nightly with --ignored"]
fn random_histories_lose_no_work_long() {
    histories(5000, 16, true);
}

// ---------------------------------------------------------------------------------------
// The cases the reviews of 2026-10-19 found, as histories of the model
// ---------------------------------------------------------------------------------------

fn act(act: Act) -> Step {
    Step::Act { act, kill: None }
}

/// The process dies before the close writes anything: the pending files stay unsealed.
fn crash() -> Step {
    Step::Act {
        act: Act::Close,
        kill: Some(Kill {
            at: 0,
            power_cut: false,
            torn: false,
        }),
    }
}

/// A pending file holding one section save (one batch of two changes), cut at a line while the
/// app is closed after a crash: the book file shows both changes, and they are kept.
fn a_batch_cut_at_a_line_while_closed() -> Vec<Step> {
    vec![
        act(Act::Section(1)),
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 1,
            at_line: true,
        },
    ]
}

/// The same batch cut inside a line: the change salvaged from the file waits for the member
/// the file lost, and the book file that shows both is the record of both.
fn a_salvaged_change_whose_batch_lost_a_member() -> Vec<Step> {
    vec![
        act(Act::Section(1)),
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 1,
            at_line: false,
        },
    ]
}

/// A pending file the open read is cut in place: the value of a verse's head can no longer be
/// read, and the next job must not write the book without the verse.
fn a_head_whose_value_can_no_longer_be_read() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: true,
        },
        act(Act::Save(2)),
        act(Act::Leave),
    ]
}

/// The metadata.json put back while open, listing the book file another program put back
/// first: the app's look finds it converged, and writes it again with the book (nightly seed
/// 5 found the oracle calling that a loss).
fn a_converged_metadata_json_written_again() -> Vec<Step> {
    vec![
        Step::Edit {
            verse: 1,
            told: false,
        },
        act(Act::Leave),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        act(Act::Leave),
    ]
}

/// `metadata.json` showing a name whose pending file is cut inside its line while the app runs,
/// then a crash: at the next open the file is clean by stat (settled), and the job must decide
/// it before writing it over, not take its stat for leave to write.
fn a_settled_metadata_json_holding_a_lost_name() -> Vec<Step> {
    vec![
        act(Act::Name),
        act(Act::Leave),
        Step::Advance,
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        crash(),
    ]
}

/// The book file held by another program at an open that finds `.tc/` without its manifest:
/// no valid `metadata.json` can list the folder, and the job waits for the file instead of
/// failing as a bug that makes the project read-only (oracle 2).
fn a_held_book_file_with_its_manifest_lost() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::LoseTc { heads: false },
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        act(Act::Save(2)),
    ]
}

/// The book file deleted while the app runs, the process killed while it rebuilds it, and the
/// path held by another program at the next open: the rebuild cannot be renamed into place, and
/// `metadata.json` stays as it is instead of failing as a bug that makes the project read-only.
fn a_deleted_book_file_whose_path_is_held() -> Vec<Step> {
    vec![
        Step::Act {
            act: Act::Leave,
            kill: Some(Kill {
                at: 5,
                power_cut: false,
                torn: false,
            }),
        },
        Step::Delete { file: File::Usfm },
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
    ]
}

/// A pending file cut inside its line while the app is closed, and held by a scanner that
/// lets it be read: the open cannot move it to quarantine, and leaves it for the next open
/// instead of turning the project read-only (oracle 2).
fn a_damaged_pending_file_a_scanner_holds() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        Step::Lock {
            what: Holdable::Pending {
                which: 0,
                readable: true,
            },
            on: true,
        },
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        act(Act::Save(2)),
    ]
}

/// A pending file held at an open (the device rotates), a checkpoint of the new device killed
/// after it made its pack folder and before it flushed the folder's entry, and a power cut
/// during a later session's seal into that folder: the seal flushes the entries of the folders
/// a killed process made, or the power cut takes the pack and the pending files it replaced.
fn a_pack_folder_a_killed_seal_made() -> Vec<Step> {
    let usfm = Holdable::File(File::Usfm);
    let pending = Holdable::Pending {
        which: 0,
        readable: false,
    };
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Lock {
            what: usfm,
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::Lock {
            what: usfm,
            on: false,
        },
        Step::Lock {
            what: pending,
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::Lock {
            what: pending,
            on: false,
        },
        Step::Act {
            act: Act::Save(1),
            kill: Some(Kill {
                at: 6,
                power_cut: false,
                torn: false,
            }),
        },
        Step::Lock {
            what: usfm,
            on: true,
        },
        Step::Open,
        act(Act::Checkpoint),
        Step::Lock {
            what: usfm,
            on: false,
        },
        Step::Open,
        Step::Act {
            act: Act::Leave,
            kill: Some(Kill {
                at: 12,
                power_cut: true,
                torn: false,
            }),
        },
    ]
}

/// The held book file of `a_held_book_file_with_its_manifest_lost`, edited outside first, so
/// the files of no book are a group of their own that finds the listing invalid.
fn a_held_edited_book_file_with_its_manifest_lost() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Section(2)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Edit {
            verse: 2,
            told: false,
        },
        Step::Act {
            act: Act::Leave,
            kill: Some(Kill {
                at: 9,
                power_cut: false,
                torn: false,
            }),
        },
        Step::LoseTc { heads: false },
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
    ]
}

/// `metadata.json` showing a new name, its group killed right after its rename; the next
/// session reads it as the write's new bytes (the ledger's entry kept the old placement's scope
/// digest), the name's pending line is cut, and the process dies: at the last open the entry's
/// stale scope matched the fold that lost the name, so the open took the file for converged
/// and hid the name it shows.
fn a_metadata_json_found_after_a_kill_past_its_rename() -> Vec<Step> {
    let usfm = Holdable::File(File::Usfm);
    vec![
        Step::KillAfterRename { file: File::Meta },
        act(Act::Name),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        Step::Lock {
            what: usfm,
            on: true,
        },
        Step::Open,
        act(Act::Leave),
        Step::Lock {
            what: usfm,
            on: false,
        },
        Step::Act {
            act: Act::Leave,
            kill: Some(Kill {
                at: 0,
                power_cut: false,
                torn: false,
            }),
        },
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
    ]
}

/// A pending file the open read cut in place (a value of the book can no longer be read),
/// the book file deleted, and later a checkpoint: the book file waits for the value (R15), and
/// its absence leaves no valid listing, which the job takes for a wait, never a bug that fails
/// the checkpoint and the close.
fn a_deleted_book_file_waiting_for_an_unreadable_value() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        Step::Act {
            act: Act::Section(1),
            kill: Some(Kill {
                at: 12,
                power_cut: false,
                torn: false,
            }),
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        act(Act::Save(1)),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 0,
        },
        act(Act::Checkpoint),
    ]
}

/// `metadata.json` renamed by a group killed right after the rename, then held unreadable at
/// the next open while a checkpoint's job runs: the job must not begin a second write of it
/// before a read settles the first, or the first write's bytes, its own, are found later as
/// someone's edit (a false `MetadataEditedOutside` and a copy).
fn a_held_metadata_json_after_a_kill_past_its_rename() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Meta },
        Step::Act {
            act: Act::Save(1),
            kill: None,
        },
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        Step::Act {
            act: Act::Save(1),
            kill: None,
        },
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: false,
        },
        Step::Lock {
            what: Holdable::File(File::Meta),
            on: true,
        },
        Step::Open,
        Step::Act {
            act: Act::Checkpoint,
            kill: None,
        },
        Step::Lock {
            what: Holdable::File(File::Meta),
            on: false,
        },
    ]
}

fn an_unreadable_prerender() -> Vec<Step> {
    vec![
        Step::Act {
            act: Act::Section(3),
            kill: Some(Kill {
                at: 20,
                power_cut: true,
                torn: false,
            }),
        },
        Step::Flip {
            journal: Journal::Pending,
            which: 173,
            at: 52175,
        },
        act(Act::Save(1)),
        Step::Lock {
            what: Holdable::Pending {
                which: 34,
                readable: true,
            },
            on: true,
        },
        act(Act::Checkpoint),
        act(Act::Save(2)),
        Step::Lock {
            what: Holdable::Pending {
                which: 34,
                readable: true,
            },
            on: false,
        },
        act(Act::Save(3)),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Save(1)),
        act(Act::Leave),
        Step::Lock {
            what: Holdable::File(File::Meta),
            on: true,
        },
        Step::Open,
    ]
}

fn an_incomplete_outside_batch() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::Edit {
            verse: 2,
            told: false,
        },
        Step::Focus,
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 1,
            at_line: true,
        },
        Step::Open,
    ]
}

fn metadata_unreadable_but_replaceable() -> Vec<Step> {
    vec![
        Step::Open,
        Step::EditName { told: false },
        Step::DenyRead {
            file: File::Meta,
            on: true,
        },
        act(Act::Save(1)),
        act(Act::Name),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Meta,
            on: false,
        },
        act(Act::Close),
        Step::Open,
    ]
}

/// Found by proptest on 2026-10-26 (the G5 lane's gate on main 29faa1ef): a save sealed into
/// a pack no commit holds yet, its pending file deleted and the book file reverted by other
/// programs, then one byte of the pack changed in place (same size) while a later session
/// that only read it runs. The seal stored the pack as a blob (§5.11 K3) and the manifest
/// kept it, so the next open writes the pack back from it (§4.4, LOSS-2).
fn a_flipped_pack_after_a_cut_pending_and_a_reverted_book() -> Vec<Step> {
    let kill = |at| {
        Some(Kill {
            at,
            power_cut: false,
            torn: false,
        })
    };
    vec![
        act(Act::Save(1)),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 0,
        },
        act(Act::Checkpoint),
        Step::Act {
            act: Act::Close,
            kill: kill(0),
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        act(Act::Leave),
        Step::Act {
            act: Act::Leave,
            kill: kill(15),
        },
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        act(Act::Save(1)),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 0,
        },
        act(Act::Checkpoint),
        Step::Act {
            act: Act::Leave,
            kill: kill(5),
        },
        Step::Flip {
            journal: Journal::Pack,
            which: 53,
            at: 0,
        },
    ]
}

/// Found by proptest on 2026-10-26 (gate/batch-24): a save in a pending file, its session
/// killed, then one byte of that save's line changed in place and the next session saves
/// again and is killed. The book file still showed the save when the damage was found, so
/// the open keeps it in a copy with *Bring it back* (§6.2 N4). The 2d912e4a oracle missed it
/// because its flip replaced the file, which the open's handle still read whole (LOSS-2).
fn a_flipped_pending_file_under_a_session_that_saves_again() -> Vec<Step> {
    let usfm = Holdable::File(File::Usfm);
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Name),
        act(Act::Leave),
        Step::Lock {
            what: usfm,
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::Lock {
            what: usfm,
            on: false,
        },
        killed(Act::Save(1), 30),
        Step::Flip {
            journal: Journal::Pending,
            which: 136,
            at: 0,
        },
        act(Act::Save(1)),
        act(Act::Leave),
        killed(Act::Close, 0),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
    ]
}

/// A save another save of the same verse replaced, so only History shows it, in a pending
/// file a later session read at its open; another program changes a byte of its line in
/// place, and the session closes. The handle the open kept reads the damage too: the seal
/// writes the line again from the bytes the open verified (§5.11 K1, LOSS-2).
fn a_superseded_save_read_at_open_and_changed_in_place() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Flip {
            journal: Journal::Pending,
            which: 0,
            at: 0,
        },
        act(Act::Close),
    ]
}

/// Two and a half minutes of [`Step::Advance`].
fn over_two_minutes() -> Vec<Step> {
    vec![Step::Advance; 13]
}

/// A save a killed session left in a pending file, older than 2 minutes when a later session
/// opens; a byte of the pending file changed in place, the book file put back as it was, and
/// that session killed too. The open sealed it before anything else (ruling
/// 2026-10-22-acceptable-loss: no save older than 2 minutes may be lost, LOSS-2).
fn an_inherited_save_older_than_two_minutes() -> Vec<Step> {
    let mut steps = vec![act(Act::Save(1)), crash()];
    steps.extend(over_two_minutes());
    steps.extend([
        Step::Open,
        Step::Flip {
            journal: Journal::Pending,
            which: 0,
            at: 0,
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        crash(),
    ]);
    steps
}

fn an_own_save_older_than_two_minutes() -> Vec<Step> {
    let mut steps = vec![act(Act::Save(1))];
    steps.extend(over_two_minutes());
    steps.extend([
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Flip {
            journal: Journal::Pending,
            which: 0,
            at: 0,
        },
        crash(),
    ]);
    steps
}

fn named_histories() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        (
            "a_flipped_pending_file_under_a_session_that_saves_again",
            a_flipped_pending_file_under_a_session_that_saves_again(),
        ),
        (
            "a_superseded_save_read_at_open_and_changed_in_place",
            a_superseded_save_read_at_open_and_changed_in_place(),
        ),
        (
            "an_inherited_save_older_than_two_minutes",
            an_inherited_save_older_than_two_minutes(),
        ),
        (
            "an_own_save_older_than_two_minutes",
            an_own_save_older_than_two_minutes(),
        ),
        (
            "a_flipped_pack_after_a_cut_pending_and_a_reverted_book",
            a_flipped_pack_after_a_cut_pending_and_a_reverted_book(),
        ),
        (
            "oracle_independence_false_adoption",
            false_adoption_witness(),
        ),
        (
            "metadata_unreadable_but_replaceable",
            metadata_unreadable_but_replaceable(),
        ),
        ("an_unreadable_prerender", an_unreadable_prerender()),
        ("an_incomplete_outside_batch", an_incomplete_outside_batch()),
        (
            "metadata_converges_during_a_book_revert",
            metadata_converges_during_a_book_revert(),
        ),
        (
            "a_held_metadata_json_after_a_kill_past_its_rename",
            a_held_metadata_json_after_a_kill_past_its_rename(),
        ),
        (
            "a_deleted_book_file_waiting_for_an_unreadable_value",
            a_deleted_book_file_waiting_for_an_unreadable_value(),
        ),
        (
            "a_metadata_json_found_after_a_kill_past_its_rename",
            a_metadata_json_found_after_a_kill_past_its_rename(),
        ),
        (
            "a_pack_folder_a_killed_seal_made",
            a_pack_folder_a_killed_seal_made(),
        ),
        (
            "a_held_edited_book_file_with_its_manifest_lost",
            a_held_edited_book_file_with_its_manifest_lost(),
        ),
        (
            "a_damaged_pending_file_a_scanner_holds",
            a_damaged_pending_file_a_scanner_holds(),
        ),
        (
            "a_held_book_file_with_its_manifest_lost",
            a_held_book_file_with_its_manifest_lost(),
        ),
        (
            "a_deleted_book_file_whose_path_is_held",
            a_deleted_book_file_whose_path_is_held(),
        ),
        (
            "a_settled_metadata_json_holding_a_lost_name",
            a_settled_metadata_json_holding_a_lost_name(),
        ),
        (
            "a_batch_cut_at_a_line_while_closed",
            a_batch_cut_at_a_line_while_closed(),
        ),
        (
            "a_salvaged_change_whose_batch_lost_a_member",
            a_salvaged_change_whose_batch_lost_a_member(),
        ),
        (
            "a_head_whose_value_can_no_longer_be_read",
            a_head_whose_value_can_no_longer_be_read(),
        ),
        (
            "a_converged_metadata_json_written_again",
            a_converged_metadata_json_written_again(),
        ),
        (
            "a_revert_after_a_quarantined_pending_and_a_killed_save",
            a_revert_after_a_quarantined_pending_and_a_killed_save(),
        ),
        (
            "a_revert_after_two_kills_past_the_rename",
            a_revert_after_two_kills_past_the_rename(),
        ),
        (
            "a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing",
            a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing(),
        ),
        (
            "a_metadata_revert_after_a_held_journal_and_kills_past_its_rename",
            a_metadata_revert_after_a_held_journal_and_kills_past_its_rename(),
        ),
        (
            "a_book_revert_after_an_outside_edit_and_a_kill_past_its_rename",
            a_book_revert_after_an_outside_edit_and_a_kill_past_its_rename(),
        ),
        (
            "a_book_revert_before_tc_is_lost",
            a_book_revert_before_tc_is_lost(),
        ),
        (
            "a_recorded_revert_whose_pending_file_is_deleted",
            a_recorded_revert_whose_pending_file_is_deleted(),
        ),
        (
            "a_metadata_revert_recorded_then_its_pending_file_deleted",
            a_metadata_revert_recorded_then_its_pending_file_deleted(),
        ),
        (
            "reverts_around_kills_a_cut_and_a_lost_manifest",
            reverts_around_kills_a_cut_and_a_lost_manifest(),
        ),
        (
            "reverts_of_both_files_after_an_unreadable_book_file",
            reverts_of_both_files_after_an_unreadable_book_file(),
        ),
        (
            "a_save_cut_from_its_pending_file_while_its_book_file_is_deleted",
            a_save_cut_from_its_pending_file_while_its_book_file_is_deleted(),
        ),
        (
            "a_save_cut_from_its_pending_file_after_a_lost_manifest",
            a_save_cut_from_its_pending_file_after_a_lost_manifest(),
        ),
        (
            "a_pending_file_cut_twice_around_a_revert_and_a_held_open",
            a_pending_file_cut_twice_around_a_revert_and_a_held_open(),
        ),
        (
            "a_saved_name_after_two_cuts_a_lost_manifest_and_an_unreadable_book",
            a_saved_name_after_two_cuts_a_lost_manifest_and_an_unreadable_book(),
        ),
        (
            "an_outside_edit_after_a_flip_and_a_cut_of_held_pending_files",
            an_outside_edit_after_a_flip_and_a_cut_of_held_pending_files(),
        ),
        (
            "reverts_of_both_files_around_a_cut_and_kills",
            reverts_of_both_files_around_a_cut_and_kills(),
        ),
        (
            "a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file",
            a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file(),
        ),
        (
            "the_apps_placed_bytes_put_back_over_an_unread_older_version",
            the_apps_placed_bytes_put_back_over_an_unread_older_version(),
        ),
        (
            "a_put_of_an_earlier_write_before_a_power_cut",
            a_put_of_an_earlier_write_before_a_power_cut(),
        ),
        (
            "a_retired_put_of_an_earlier_write_before_a_power_cut",
            a_retired_put_of_an_earlier_write_before_a_power_cut(),
        ),
    ]
}

fn false_adoption_witness() -> Vec<Step> {
    vec![
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::Open,
        act(Act::Save(2)),
        act(Act::Leave),
    ]
}

/// These run only the generic oracle; independent probe controls remain separate.
#[test]
fn oracle_independence_false_adoption() {
    assert_kept(&false_adoption_witness());
}

/// [`Run::run`] for a history that must end with every block released and a repair that can
/// read what it writes: a run whose end is blocked never validates its terminal Burrito, so it
/// would pass without that check.
fn run_to_a_validated_end(steps: &[Step]) -> Result<(), String> {
    let before = terminal_boundaries()[0];
    Run::run(steps)?;
    match terminal_boundaries()[0] - before {
        1 => Ok(()),
        _ => Err("its terminal Burrito was never validated".into()),
    }
}

fn assert_kept(steps: &[Step]) {
    if let Err(lost) = run_to_a_validated_end(steps) {
        panic!("INV-NO-EXCUSED-LOSS: {lost}");
    }
}

#[test]
fn a_flipped_pack_read_at_open_is_restored_from_its_stored_blob() {
    if let Err(lost) =
        run_to_a_validated_end(&a_flipped_pack_after_a_cut_pending_and_a_reverted_book())
    {
        panic!("INV-PACK-RESTORE: {lost}");
    }
}

#[test]
fn a_flipped_pending_file_under_a_session_that_saves_again_is_kept() {
    assert_kept(&a_flipped_pending_file_under_a_session_that_saves_again());
}

#[test]
fn an_inherited_save_older_than_two_minutes_is_sealed_at_the_open() {
    assert_kept(&an_inherited_save_older_than_two_minutes());
}

#[test]
fn an_own_save_older_than_two_minutes_is_sealed_by_then() {
    assert_kept(&an_own_save_older_than_two_minutes());
}

#[test]
fn a_superseded_save_changed_in_place_is_written_again_from_the_opens_bytes() {
    assert_kept(&a_superseded_save_read_at_open_and_changed_in_place());
}

#[test]
fn a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file() {
    assert_kept(&a_batch_cut_at_a_line_while_closed());
}

#[test]
fn a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file() {
    assert_kept(&a_salvaged_change_whose_batch_lost_a_member());
}

#[test]
fn a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent() {
    assert_kept(&a_head_whose_value_can_no_longer_be_read());
}

#[test]
fn a_converged_metadata_json_written_again_is_no_loss() {
    assert_kept(&a_converged_metadata_json_written_again());
}

#[test]
fn a_settled_metadata_json_holding_a_lost_name_is_recorded_before_it_is_written_over() {
    assert_kept(&a_settled_metadata_json_holding_a_lost_name());
}

#[test]
fn a_held_book_file_with_its_manifest_lost_leaves_the_project_writable() {
    assert_kept(&a_held_book_file_with_its_manifest_lost());
}

#[test]
fn a_deleted_book_file_whose_path_is_held_leaves_the_project_writable() {
    assert_kept(&a_deleted_book_file_whose_path_is_held());
}

#[test]
fn a_damaged_pending_file_a_scanner_holds_leaves_the_project_writable() {
    assert_kept(&a_damaged_pending_file_a_scanner_holds());
}

#[test]
fn a_pack_in_a_folder_a_killed_seal_made_survives_a_power_cut() {
    assert_kept(&a_pack_folder_a_killed_seal_made());
}

#[test]
fn a_held_edited_book_file_with_its_manifest_lost_leaves_the_project_writable() {
    assert_kept(&a_held_edited_book_file_with_its_manifest_lost());
}

#[test]
fn a_metadata_json_found_after_a_kill_past_its_rename_is_never_taken_for_converged() {
    assert_kept(&a_metadata_json_found_after_a_kill_past_its_rename());
}

#[test]
fn a_deleted_book_file_waiting_for_an_unreadable_value_fails_no_checkpoint() {
    assert_kept(&a_deleted_book_file_waiting_for_an_unreadable_value());
}

#[test]
fn a_held_metadata_json_after_a_kill_past_its_rename_is_not_taken_for_an_edit() {
    held_metadata_retries_keep_the_same_copies_and_notices();
}

fn held_metadata_retries_keep_the_same_copies_and_notices() {
    let steps = a_held_metadata_json_after_a_kill_past_its_rename();
    let (prefix, tail) = steps.split_at(steps.len() - 2);
    assert!(matches!(prefix.last(), Some(Step::Open)));
    let mut run = Run::new();
    for step in prefix {
        run.step(step).unwrap();
    }
    assert!(matches!(
        run.locked.get(META),
        Some(DeniedAccess::ReadAndReplace)
    ));
    assert!(matches!(run.ledger(META), Some(State::Writing { .. })));
    let metadata_notices = |run: &Run| {
        run.handle()
            .notices()
            .into_iter()
            .filter(|notice| matches!(&notice.notice, Notice::MetadataEditedOutside { .. }))
            .collect::<Vec<_>>()
    };
    // The earlier outside revert legitimately kept copies. Only this held write and its
    // release must add none, including when a reused filename would hide a duplicate.
    let copies = run.fs().files_under(&run.path(".tc/external"));
    let notices = metadata_notices(&run);
    for step in tail {
        run.step(step).unwrap();
        assert_eq!(
            run.fs().files_under(&run.path(".tc/external")),
            copies,
            "metadata checkpoint/release changed preservation copies after {step:?}"
        );
        assert_eq!(
            metadata_notices(&run),
            notices,
            "metadata checkpoint/release reported an extra outside edit after {step:?}"
        );
    }
    run.finish().unwrap();
}

#[test]
fn an_unreadable_prerender_never_replaces_the_last_copy_of_a_save() {
    assert_kept(&an_unreadable_prerender());
}

fn metadata_converges_during_a_book_revert() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Section(1)),
        act(Act::Leave),
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: false,
        },
        Step::KillAfterRename { file: File::Meta },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: true,
        },
    ]
}

#[test]
fn a_metadata_revert_is_kept_when_a_book_revert_triggers_its_materialization() {
    let mut run = Run::new();
    for step in metadata_converges_during_a_book_revert() {
        run.step(&step).unwrap();
    }
    assert!(
        run.handle.is_none(),
        "the metadata rename must trigger the crash"
    );
    // The book revert is held for the person's answer (delta §2.4), so `metadata.json` lists
    // the held bytes and is written over the put-back version; the oracle checks that version
    // is kept.
    assert!(run.puts.iter().any(|put| put.rel == META));
    assert_valid_burrito(&run.fs(), &run.folder);
    run.open().unwrap();
    assert_eq!(run.handle().waiting_books().len(), 1);
    run.finish().unwrap();
}

#[test]
fn a_valid_outside_metadata_name_is_not_retired_as_converged_at_a_crash() {
    let mut run = Run::new();
    run.step(&Step::Open).unwrap();
    run.step(&Step::EditName { told: false }).unwrap();
    let put = run.puts.iter().find(|put| put.rel == META).unwrap();
    let bytes = put.bytes.clone();
    assert!(!put.retired);
    assert_valid_burrito(&run.fs(), &run.folder);
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        !name_of(run.handle()).contains(document["identification"]["name"]["en"].as_str().unwrap())
    );
    run.crash(false);
    assert!(!run.puts.iter().find(|put| put.rel == META).unwrap().retired);
    assert_eq!(run.fs().peek(&run.path(META)), Some(bytes.clone()));
    run.finish().unwrap();
    assert!(
        run.fs()
            .files_under(&run.path(".tc/external"))
            .values()
            .any(|copy| *copy == bytes)
    );
    run.open().unwrap();
    assert!(run.in_history(&Save {
        text: "zq1x".into(),
        target: Target::Name,
        session: 0,
        fate: Fate::Kept,
        shown: false,
        at_ms: 0,
    }));
    run.finish().unwrap();
}

#[test]
fn unreadable_metadata_is_preserved_before_a_permitted_replacement() {
    assert_kept(&metadata_unreadable_but_replaceable());
}

fn a_revert_after_a_quarantined_pending_and_a_killed_save() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        Step::Act {
            act: Act::Save(2),
            kill: Some(Kill {
                at: 3,
                power_cut: false,
                torn: false,
            }),
        },
        Step::Revert {
            file: File::Usfm,
            back: 1,
            told: false,
        },
        Step::EditName { told: false },
    ]
}

#[test]
fn a_revert_after_a_quarantined_pending_and_a_killed_save_is_kept() {
    assert_kept(&a_revert_after_a_quarantined_pending_and_a_killed_save());
}

fn killed(act: Act, at: u16) -> Step {
    Step::Act {
        act,
        kill: Some(Kill {
            at,
            power_cut: false,
            torn: false,
        }),
    }
}

/// A revert of the book file to the bytes before a write killed past its rename, made while
/// a save's write was killed again.
fn a_revert_after_two_kills_past_the_rename() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        act(Act::Leave),
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::Lock {
            what: Holdable::File(File::Usfm),
            on: false,
        },
        killed(Act::Save(1), 5),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        crash(),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn a_revert_after_two_kills_past_the_rename_is_kept() {
    assert_kept(&a_revert_after_two_kills_past_the_rename());
}

/// The last copy of a saved name is `metadata.json`, while the book file is missing and
/// waits for a value it cannot read: no book is listed, so the metadata the app would write
/// lists no scope.
fn a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        crash(),
        Step::Open,
        act(Act::Name),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Usfm,
            on: false,
        },
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        killed(Act::Save(1), 0),
        Step::Delete { file: File::Usfm },
    ]
}

#[test]
fn a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing_is_kept() {
    assert_kept(&a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing());
}

fn lock(what: Holdable, on: bool) -> Step {
    Step::Lock { what, on }
}

fn pending_held(which: u8, readable: bool) -> Holdable {
    Holdable::Pending { which, readable }
}

/// Found by the bounded model on 2026-10-20 (round iter-9).
fn a_metadata_revert_after_a_held_journal_and_kills_past_its_rename() -> Vec<Step> {
    vec![
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 235,
            keep: 195,
            at_line: true,
        },
        act(Act::Save(1)),
        lock(pending_held(153, true), true),
        act(Act::Checkpoint),
        act(Act::Save(2)),
        lock(pending_held(153, true), false),
        act(Act::Save(3)),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Checkpoint),
        act(Act::Leave),
        lock(Holdable::File(File::Meta), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Meta), false),
        Step::RevertKeepTime {
            file: File::Usfm,
            back: 0,
        },
        act(Act::Section(3)),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Leave),
        Step::Advance,
        Step::Revert {
            file: File::Meta,
            back: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Meta,
            on: false,
        },
    ]
}

#[test]
fn a_metadata_revert_after_a_held_journal_and_kills_past_its_rename_is_kept() {
    assert_kept(&a_metadata_revert_after_a_held_journal_and_kills_past_its_rename());
}

/// Found by the bounded model on 2026-10-20 (round iter-12).
fn a_book_revert_after_an_outside_edit_and_a_kill_past_its_rename() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        lock(pending_held(0, false), true),
        act(Act::Checkpoint),
        act(Act::Save(2)),
        lock(pending_held(0, false), false),
        act(Act::Save(3)),
        killed(Act::Save(1), 5),
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn a_book_revert_after_an_outside_edit_and_a_kill_past_its_rename_is_kept() {
    assert_kept(&a_book_revert_after_an_outside_edit_and_a_kill_past_its_rename());
}

/// Found by the bounded model on 2026-10-20 (round r2-2).
fn a_book_revert_before_tc_is_lost() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Name),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(2)),
        lock(Holdable::File(File::Usfm), false),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
        crash(),
        Step::LoseTc { heads: false },
    ]
}

#[test]
fn a_book_revert_before_tc_is_lost_is_kept() {
    assert_kept(&a_book_revert_before_tc_is_lost());
}

/// Found by the bounded model on 2026-10-20 (round r3-11).
fn a_recorded_revert_whose_pending_file_is_deleted() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        lock(pending_held(0, false), true),
        act(Act::Checkpoint),
        act(Act::Save(2)),
        lock(pending_held(0, false), false),
        act(Act::Save(3)),
        Step::Cut {
            journal: Journal::Pending,
            which: 151,
            keep: 0,
            at_line: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(3)),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
        crash(),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 1,
        },
    ]
}

#[test]
fn a_recorded_revert_whose_pending_file_is_deleted_is_kept() {
    assert_kept(&a_recorded_revert_whose_pending_file_is_deleted());
}

/// Found by the bounded model on 2026-10-20 (round r3-12).
fn a_metadata_revert_recorded_then_its_pending_file_deleted() -> Vec<Step> {
    vec![
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Section(3)),
        lock(Holdable::File(File::Usfm), false),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Section(3)),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        killed(Act::Save(1), 0),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 110,
        },
    ]
}

#[test]
fn a_metadata_revert_recorded_then_its_pending_file_deleted_is_kept() {
    assert_kept(&a_metadata_revert_recorded_then_its_pending_file_deleted());
}

/// Found by the bounded model on 2026-10-20 (round r3-13).
fn reverts_around_kills_a_cut_and_a_lost_manifest() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        killed(Act::Save(1), 0),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        killed(Act::Leave, 0),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        killed(Act::Save(2), 5),
        Step::LoseTc { heads: false },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn reverts_around_kills_a_cut_and_a_lost_manifest_are_kept() {
    assert_kept(&reverts_around_kills_a_cut_and_a_lost_manifest());
}

/// Found by the bounded model on 2026-10-20 (round r3-14).
fn reverts_of_both_files_after_an_unreadable_book_file() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        crash(),
        Step::Open,
        act(Act::Save(1)),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Usfm,
            on: false,
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::KillAfterRename { file: File::Meta },
        act(Act::Leave),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn reverts_of_both_files_after_an_unreadable_book_file_are_kept() {
    assert_kept(&reverts_of_both_files_after_an_unreadable_book_file());
}

/// Found by the bounded model on 2026-10-21 (mutation run's clean round): a save whose
/// pending file is cut inside its first line while the book file showing it is deleted.
fn a_save_cut_from_its_pending_file_while_its_book_file_is_deleted() -> Vec<Step> {
    vec![
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(2)),
        lock(Holdable::File(File::Usfm), false),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(4)),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        killed(Act::Save(1), 21),
        Step::RevertKeepTime {
            file: File::Usfm,
            back: 0,
        },
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
    ]
}

#[test]
fn a_save_cut_from_its_pending_file_while_its_book_file_is_deleted_is_kept() {
    assert_kept(&a_save_cut_from_its_pending_file_while_its_book_file_is_deleted());
}

/// Found by the long model on 2026-10-21 (seed 20261021001): the same cut and deletion after
/// a lost manifest.
fn a_save_cut_from_its_pending_file_after_a_lost_manifest() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        lock(pending_held(0, false), true),
        act(Act::Checkpoint),
        act(Act::Save(2)),
        lock(pending_held(0, false), false),
        act(Act::Save(3)),
        crash(),
        Step::LoseTc { heads: false },
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
    ]
}

#[test]
fn a_save_cut_from_its_pending_file_after_a_lost_manifest_is_kept() {
    assert_kept(&a_save_cut_from_its_pending_file_after_a_lost_manifest());
}

/// Found by the long model on 2026-10-21 (seed 20261021002).
fn a_pending_file_cut_twice_around_a_revert_and_a_held_open() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        crash(),
        Step::RevertKeepTime {
            file: File::Meta,
            back: 0,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        lock(pending_held(0, false), true),
        Step::Open,
        act(Act::Close),
        lock(pending_held(0, false), false),
    ]
}

#[test]
fn a_pending_file_cut_twice_around_a_revert_and_a_held_open_is_kept() {
    assert_kept(&a_pending_file_cut_twice_around_a_revert_and_a_held_open());
}

/// Found by the long model on 2026-10-21 (seed 20261021003).
fn a_saved_name_after_two_cuts_a_lost_manifest_and_an_unreadable_book() -> Vec<Step> {
    vec![
        killed(Act::Name, 5),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        crash(),
        Step::LoseTc { heads: false },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        killed(Act::Save(1), 0),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
    ]
}

#[test]
fn a_saved_name_after_two_cuts_a_lost_manifest_and_an_unreadable_book_is_kept() {
    assert_kept(&a_saved_name_after_two_cuts_a_lost_manifest_and_an_unreadable_book());
}

/// Found by the long model on 2026-10-21 (seed 20261021004).
fn an_outside_edit_after_a_flip_and_a_cut_of_held_pending_files() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        crash(),
        Step::Open,
        act(Act::Save(1)),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Usfm,
            on: false,
        },
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Checkpoint),
        lock(Holdable::File(File::Usfm), false),
        killed(Act::Leave, 4),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(4)),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        Step::Flip {
            journal: Journal::Pending,
            which: 180,
            at: 0,
        },
        Step::Cut {
            journal: Journal::Pending,
            which: 3,
            keep: 0,
            at_line: false,
        },
        Step::Edit {
            verse: 4,
            told: false,
        },
    ]
}

#[test]
fn an_outside_edit_after_a_flip_and_a_cut_of_held_pending_files_is_kept() {
    assert_kept(&an_outside_edit_after_a_flip_and_a_cut_of_held_pending_files());
}

/// Found by the long model on 2026-10-21 (seed 20261021003).
fn reverts_of_both_files_around_a_cut_and_kills() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Meta },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        Step::EditName { told: false },
        killed(Act::Leave, 33),
        Step::Cut {
            journal: Journal::Pending,
            which: 30,
            keep: 0,
            at_line: false,
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        killed(Act::Save(1), 5),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn reverts_of_both_files_around_a_cut_and_kills_are_kept() {
    assert_kept(&reverts_of_both_files_around_a_cut_and_kills());
}

/// Found by the bounded model on 2026-10-22 (S2 step 1, on a clean tree): the open adopts a
/// book file equal to the projection that the ledger last named otherwise, and
/// `metadata.json` must list it as it is (I7).
fn a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        crash(),
        Step::Open,
        act(Act::Name),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Usfm,
            on: false,
        },
        killed(Act::Leave, 5),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 7,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

#[test]
fn a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file_leaves_a_valid_burrito() {
    assert_kept(&a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file());
}

/// Found by the long model on 2026-10-22 (seed 20261021004), an oracle bug: another program
/// puts back an older version of the book file the project already shows, and then the
/// app's own placed bytes over it before the app reads either. The app's own bytes are what
/// it last placed, not bytes the project merely shows.
fn the_apps_placed_bytes_put_back_over_an_unread_older_version() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        killed(Act::Name, 0),
        Step::RevertKeepTime {
            file: File::Usfm,
            back: 2,
        },
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Name),
        lock(Holdable::File(File::Usfm), false),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Checkpoint),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
        killed(Act::Section(1), 5),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
    ]
}

#[test]
fn the_apps_placed_bytes_put_back_over_an_unread_older_version_are_no_loss() {
    assert_kept(&the_apps_placed_bytes_put_back_over_an_unread_older_version());
}

/// Found by the long model on 2026-10-22 (seed 20261021004, after the fix above), an oracle
/// bug: another program puts back an earlier write of the app, a power cut follows, and the
/// oracle took those bytes for the power cut's rollback of the app's own write.
fn a_put_of_an_earlier_write_before_a_power_cut() -> Vec<Step> {
    vec![
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        kill_with(Act::Checkpoint, 0, true, false),
        Step::RevertKeepTime {
            file: File::Usfm,
            back: 1,
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Touch { file: File::Usfm },
    ]
}

#[test]
fn a_put_of_an_earlier_write_before_a_power_cut_is_no_loss() {
    assert_kept(&a_put_of_an_earlier_write_before_a_power_cut());
}

fn kill_with(act: Act, at: u16, power_cut: bool, torn: bool) -> Step {
    Step::Act {
        act,
        kill: Some(Kill {
            at,
            power_cut,
            torn,
        }),
    }
}

/// Found by the long model on 2026-10-22 (seed 20261021001; shortened): another program
/// changes a byte of a pack, a checkpoint commits that damaged pack (no good blob exists),
/// and then cuts the pack in the folder. The verified lines of every copy are salvaged into
/// a pending batch (§4.4 "All copies damaged"), so a save only HEAD's damaged copy still
/// holds stays in History.
fn a_save_only_a_damaged_committed_pack_holds() -> Vec<Step> {
    vec![
        killed(Act::Name, 17),
        act(Act::Save(1)),
        act(Act::Checkpoint),
        act(Act::Save(3)),
        Step::Flip {
            journal: Journal::Pack,
            which: 103,
            at: 6578,
        },
        act(Act::Checkpoint),
        act(Act::Save(1)),
        act(Act::Checkpoint),
        Step::Cut {
            journal: Journal::Pack,
            which: 105,
            keep: 0,
            at_line: false,
        },
    ]
}

#[test]
fn a_save_only_a_damaged_committed_pack_holds_is_salvaged() {
    assert_kept(&a_save_only_a_damaged_committed_pack_holds());
}

/// Found by the long model on 2026-10-22 (seed 20261021003, after the fixes above), an
/// oracle bug of the same kind: another program's put of an earlier app write was retired
/// (the project shows its text) before a power cut, and the oracle took it for the power
/// cut's rollback of the app's write. Only bytes the power cut brought back can be that.
fn a_retired_put_of_an_earlier_write_before_a_power_cut() -> Vec<Step> {
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        crash(),
        Step::Open,
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        Step::Focus,
        Step::Delete { file: File::Usfm },
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 0,
            at_line: false,
        },
        killed(Act::Leave, 0),
        Step::RevertKeepTime {
            file: File::Meta,
            back: 0,
        },
        crash(),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Name),
        act(Act::Leave),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(1)),
        lock(Holdable::File(File::Usfm), false),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
        Step::EditName { told: true },
        kill_with(Act::Leave, 0, true, false),
        Step::Revert {
            file: File::Usfm,
            back: 2,
            told: false,
        },
    ]
}

#[test]
fn a_retired_put_of_an_earlier_write_before_a_power_cut_is_no_loss() {
    assert_kept(&a_retired_put_of_an_earlier_write_before_a_power_cut());
}

/// Found by the nightly model run 40085200557 (2026-10-24): a program puts an older book file
/// back, the next checkpoint keeps and holds it, and `.tc/materialized.json` is lost before
/// the next open. The copy in `.tc/external/` is listed again for the person (the notice is
/// not lost with the manifest).
fn a_kept_book_copy_after_a_lost_manifest() -> Vec<Step> {
    vec![
        act(Act::Save(3)),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Leave),
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
        act(Act::Checkpoint),
        lock(Holdable::File(File::Usfm), true),
        Step::Open,
        act(Act::Save(3)),
        lock(Holdable::File(File::Usfm), false),
        killed(Act::Save(1), 5),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        killed(Act::Checkpoint, 0),
        Step::LoseTc { heads: false },
    ]
}

#[test]
fn a_kept_book_copy_after_a_lost_manifest_is_listed_again() {
    assert_kept(&a_kept_book_copy_after_a_lost_manifest());
}

/// The history `random_histories_lose_no_work` shrank to after the merge of track-D1 and main
/// (2026-10-25): a book file the open could not read, a crash, saves and kills, then the file
/// reverted twice to earlier versions of itself while the project is open.
fn two_reverts_of_a_book_file_after_crashes() -> Vec<Step> {
    vec![
        Step::Open,
        Step::Edit {
            verse: 1,
            told: false,
        },
        Step::DenyRead {
            file: File::Usfm,
            on: true,
        },
        killed(Act::Close, 0),
        Step::Open,
        act(Act::Save(1)),
        act(Act::Leave),
        Step::DenyRead {
            file: File::Usfm,
            on: false,
        },
        killed(Act::Close, 0),
        Step::Delete { file: File::Usfm },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Save(1)),
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Leave),
        Step::Revert {
            file: File::Usfm,
            back: 1,
            told: false,
        },
    ]
}

#[test]
fn two_reverts_of_a_book_file_after_crashes_lose_nothing() {
    assert_kept(&two_reverts_of_a_book_file_after_crashes());
}

/// Found by CI on 2026-10-27: a power cut while leaving, then another program puts the book
/// file back and touches it. The checkpoints keep the outside version as a copy in
/// `.tc/external/`, and a notice must lead the translator to that copy.
fn an_outside_revert_after_a_power_cut_while_leaving() -> Vec<Step> {
    vec![
        act(Act::Save(2)),
        Step::Focus,
        act(Act::Save(1)),
        Step::DeleteJournal {
            journal: Journal::Pending,
            which: 214,
        },
        act(Act::Checkpoint),
        Step::Act {
            act: Act::Leave,
            kill: Some(Kill {
                at: 13,
                power_cut: true,
                torn: false,
            }),
        },
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
        Step::Touch { file: File::Usfm },
        act(Act::Checkpoint),
        act(Act::Checkpoint),
    ]
}

#[test]
fn an_outside_revert_after_a_power_cut_while_leaving_is_kept_and_shown() {
    assert_kept(&an_outside_revert_after_a_power_cut_while_leaving());
}
