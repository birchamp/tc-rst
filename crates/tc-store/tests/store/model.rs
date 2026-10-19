//! A model-based property test of the project store (`docs/STORAGE.md` §16.1, §6.2, §11): random
//! histories of a Titus project run against the real store on a `FaultFs`, and after every step
//! the store is compared with a small reference model of what the person did.
//!
//! The model keeps two kinds of work. A **save** is words the person saved through the app (a
//! verse, a section of several verses in one batch, the project's name) and was told were saved.
//! A **put** is the whole bytes another program wrote into the folder (an edit of a verse in the
//! book file, an edit of `metadata.json`, a version of either put back, a byte the disk changed
//! under the same size and time). The no-loss invariant:
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
//!   and so is a put the app's look found converged or the ledger names as the app's own.
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
//! rename of a file; outside edits, reverts (keeping the old time or not), same-size damage and
//! deletions while open and while closed; pending files and packs cut, changed or deleted, in
//! place while open or while closed, or held open by another program; a lost manifest or heads
//! files; the person's choice on a held file; restarts; the window's focus; the clock. The
//! generator favours the sequences the reviews found bugs in: a crash followed by damage while
//! closed, a kill after a rename followed by a revert, a file held around an open.

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
use tc_store::{CheckpointReason, CommandError, Leave, OpenOptions, ProjectHandle};

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
    /// Another program holds a file open (`on`), or lets go of it: it cannot be read, renamed
    /// over or removed meanwhile.
    Lock { what: Holdable, on: bool },
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
    /// The disk changes a letter of a file and leaves its size and time as they were, unseen.
    Damage { file: File },
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
    /// The person settles a held file: takes it in as an edit, or restores it.
    Choose { take_in: bool },
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
}

#[derive(Debug, Clone)]
struct Put {
    rel: &'static str,
    bytes: Vec<u8>,
    what: String,
    retired: bool,
}

/// One run of steps against the store.
pub struct Run {
    rig: Option<Rig>,
    folder: PathBuf,
    handle: Option<ProjectHandle>,
    session: u32,
    next: u32,
    saves: Vec<Save>,
    puts: Vec<Put>,
    /// Every version of each file the run saw in the folder, oldest first, with its
    /// modification time when the run first saw it.
    versions: BTreeMap<&'static str, Vec<(Vec<u8>, i64)>>,
    /// Whether the kill armed is a power cut.
    power_cut: bool,
    /// The files another program holds, relative to the folder, and whether they can be read.
    locked: BTreeMap<String, bool>,
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
    /// The project's name as the fold showed it at the last check.
    name: String,
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
        let rig = Self::rig_on(fs.copy());
        let mut run = Self {
            rig: Some(rig),
            folder: folder.clone(),
            handle: None,
            session: 0,
            next: 0,
            saves: Vec::new(),
            puts: Vec::new(),
            versions: BTreeMap::new(),
            power_cut: false,
            locked: BTreeMap::new(),
            outside: false,
            replaced: Vec::new(),
            journal_held: false,
            name: String::new(),
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
                .any(|(rel, readable)| rel.starts_with(".tc/pending/") && !readable);
            self.handle = Some(handle);
            self.log.push(format!("opened (session {})", self.session));
            if !self.died() {
                return self.check(true);
            }
        }
    }

    /// A store on `fs`. Outside edits the watcher misses leave `metadata.json` listing bytes the
    /// folder no longer holds until the app looks, so the rig does not check the burrito here.
    fn rig_on(fs: FaultFs) -> Rig {
        let rig = rig_as(fs, APP, "host-1");
        damaged_on_purpose(&rig);
        rig
    }

    /// The process died: the next one finds the files as a kill or a power cut leaves them.
    fn crash(&mut self, power_cut: bool) {
        let fs = self.fs().crash(power_cut);
        if let Some(handle) = self.handle.take() {
            drop(handle);
        }
        self.rig = None;
        self.rig = Some(Self::rig_on(fs));
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
                let handle = self.handle.take().expect("open");
                let closed = handle.close();
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
                self.rig = Some(Self::rig_on(fs));
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
        for (rel, readable) in &self.locked {
            fs.set_unreadable(&self.path(rel), !readable);
            fs.set_busy(&self.path(rel), true);
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
        let (rels, readable): (Vec<String>, bool) = match (what, on) {
            (Holdable::File(file), _) => (vec![file.rel().to_owned()], false),
            (Holdable::Pending { which, readable }, true) => (
                self.journal_file(Journal::Pending, which)
                    .map(|(rel, _)| rel)
                    .into_iter()
                    .collect(),
                readable,
            ),
            (Holdable::Pending { .. }, false) => (
                self.locked
                    .keys()
                    .filter(|rel| rel.starts_with(".tc/pending/"))
                    .cloned()
                    .collect(),
                false,
            ),
        };
        let fs = self.fs();
        for rel in rels {
            if on && !readable && rel.starts_with(".tc/pending/") {
                // The fold of the session that cannot read it lacks its saves: a book file
                // that shows them is kept as someone's work (R4), a copy that stays.
                self.journal_held = true;
                self.outside = true;
            }
            if on {
                self.locked.insert(rel.clone(), readable);
            } else {
                self.locked.remove(&rel);
            }
            let path = self.path(&rel);
            fs.set_unreadable(&path, on && !readable);
            fs.set_busy(&path, on);
        }
        if on || self.handle.is_none() {
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
        let fs = self.fs();
        let path = self.path(rel);
        let now = fs.peek(&path);
        if now.as_deref() == Some(bytes.as_slice()) {
            return;
        }
        self.outside = true;
        // Bytes the ledger names as the app's own: what they show the project holds.
        let ours = match self.ledger(rel) {
            Some(State::Placed { placed }) => placed.md5 == md5(&bytes),
            Some(State::Writing { new, .. }) => new == md5(&bytes),
            _ => false,
        };
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
        self.puts.push(Put {
            rel,
            bytes,
            what,
            retired: ours,
        });
        // A put of `metadata.json` in the folder that lists the folder as it is now and the
        // project's name: the app's look finds it converged (R2), whatever happens later. A
        // put of the book file can make an earlier one so.
        let name = match &self.handle {
            Some(handle) => name_of(handle),
            None => self.name.clone(),
        };
        let meta = fs.peek(&self.path(META));
        for put in &mut self.puts {
            if put.rel == META
                && !put.retired
                && meta.as_deref() == Some(put.bytes.as_slice())
                && describes_the_folder(&fs, &self.folder, &put.bytes, &name)
            {
                put.retired = true;
            }
        }
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

    /// The disk changes a letter in the words of a file and keeps its size and time.
    fn damage(&mut self, file: File) -> Result<(), String> {
        let rel = file.rel();
        let fs = self.fs();
        let path = self.path(rel);
        let (Some(mut bytes), Ok(meta)) = (fs.peek(&path), tc_store::Vfs::metadata(&fs, &path))
        else {
            return Ok(());
        };
        // The first letter of a word of the book file (never a marker's), or of the name.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let from = match file {
            File::Usfm => Some(0),
            File::Meta => text
                .find("\"name\"")
                .and_then(|i| text[i..].find("\"en\": \"").map(|j| i + j + 7)),
        };
        let Some(at) = from.and_then(|from| {
            let mut marker = false;
            for (i, b) in bytes.iter().enumerate().skip(from) {
                match *b {
                    b'\\' => marker = true,
                    b'\n' if file == File::Meta => return None,
                    b if b.is_ascii_whitespace() => marker = false,
                    b if b.is_ascii_lowercase() && !marker => return Some(i),
                    _ => {}
                }
            }
            None
        }) else {
            return Ok(());
        };
        bytes[at] = b'a' + (bytes[at] - b'a' + 1) % 26;
        self.put(
            rel,
            bytes,
            Some(meta.mtime_ns),
            false,
            format!("damage of {rel}"),
        );
        Ok(())
    }

    /// Another program deletes a derived file: a deletion is never an edit (I11).
    fn delete(&mut self, file: File) -> Result<(), String> {
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
        self.by_another_program(|| fs.put_unseen(&path, &bytes, None));
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
            }
        }
        Ok(())
    }

    /// The person settles the first held file.
    fn choose(&mut self, take_in: bool) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        let held = self
            .handle()
            .notices()
            .into_iter()
            .find_map(|n| match n.notice {
                Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. } => {
                    Some(path.as_str().to_owned())
                }
                _ => None,
            });
        let Some(rel) = held else {
            return Ok(());
        };
        self.log.push(format!("chose {rel} (take in: {take_in})"));
        let done = if take_in {
            self.handle().take_in_held(&rel).wait().map(|_| ())
        } else {
            self.handle().restore_held(&rel).wait()
        };
        if let Err(e) = done {
            self.refused(&format!("the choice for {rel}"), e)?;
        }
        self.settle()
    }

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
        let session = self.session;
        for (i, save) in self.saves.iter_mut().enumerate() {
            save.shown &= shown[i];
            if save.fate != Fate::Kept {
                continue;
            }
            if open && (save.session == session || (readable && held[i])) {
                // The running session holds it (it saved it, or read it at its open), and
                // writes it again at its next seal (§5.11 K1); until then it may write over a
                // file that shows it.
                if journal_before[i] && !journal_after[i] {
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
    /// files do not count.
    fn on_disk(&self, text: &str) -> bool {
        let fs = self.fs();
        let project = project_of(&fs, &self.folder);
        for (rel, bytes) in files(&fs, &self.folder) {
            if rel.starts_with(".tc/quarantine/") || rel == ".tc/index.bin" {
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
        self.name = name_of(self.handle());
        let mut lost = Vec::new();
        self.kept_nothing_twice(&mut lost);
        let fs = self.fs();
        let held_files: Vec<&str> = [USFM, META]
            .into_iter()
            .filter(|rel| matches!(self.ledger(rel), Some(State::Held { .. })))
            .collect();
        for save in &self.saves {
            if save.fate == Fate::Destroyed {
                continue;
            }
            if self.in_history(save) {
                continue;
            }
            if !save.shown && (self.waiting(save) || self.text_in_external(&save.text)) {
                continue;
            }
            // A file showing it that the app holds as it is, with a copy and a notice, until
            // the person takes it in or restores it (§6.2 held).
            if self.text_in_external(&save.text)
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
            if save.fate == Fate::InMemory && !settled {
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
        let projection = self
            .handle()
            .snapshot()
            .fold()
            .render(&DerivedFile::Usfm(TIT));
        // The bytes the ledger names as placed: the app's look found a put converged (R2) or
        // recorded it (with a copy); either way nothing in it is the app's to keep apart.
        let placed: Vec<(&str, String)> = [USFM, META]
            .into_iter()
            .filter_map(|rel| match self.ledger(rel) {
                Some(State::Placed { placed }) => Some((rel, placed.md5)),
                _ => None,
            })
            .collect();
        let external: Vec<Vec<u8>> = fs
            .files_under(&self.path(".tc/external"))
            .into_values()
            .collect();
        for put in &mut self.puts {
            if put.retired {
                continue;
            }
            let on_disk =
                fs.peek(&self.folder.join(put.rel)).as_deref() == Some(put.bytes.as_slice());
            if placed.contains(&(put.rel, md5(&put.bytes))) {
                put.retired = true;
                continue;
            }
            if put.rel == USFM && projection.as_deref() == Some(put.bytes.as_slice()) {
                // What the project would write there: nothing in the bytes is the app's to
                // keep apart from the project, now or later.
                put.retired = true;
                continue;
            }
            if on_disk || external.contains(&put.bytes) {
                continue;
            }
            lost.push(format!(
                "the {}: not in the folder, not the projection, no copy in .tc/external",
                put.what
            ));
        }
        if lost.is_empty() {
            return Ok(());
        }
        // What the app knew, for the reader of a failure.
        lost.push(format!(
            "ledger: {USFM} {:?}; {META} {:?}\nexternal: {:?}\nnotices: {:?}",
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
            Step::Open => self.open(),
            Step::Edit { verse, told } => self.edit(*verse, *told),
            Step::EditName { told } => self.edit_name(*told),
            Step::Revert { file, back, told } => self.revert(*file, *back, *told, false),
            Step::RevertKeepTime { file, back } => self.revert(*file, *back, false, true),
            Step::Damage { file } => self.damage(*file),
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
            Step::Choose { take_in } => self.choose(*take_in),
            Step::Focus => self.focus(),
            Step::Advance => {
                self.fs().advance(Duration::from_secs(10));
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
        let _ = self.handle().leave(Leave::Book(TIT)).wait();
        self.handle().wait_idle();
        self.check(true)?;
        if let Some(handle) = self.handle.take() {
            handle
                .close()
                .map_err(|e| format!("the last close: {e:?}"))?;
        }
        Ok(())
    }

    /// Runs `steps` and the end, and says what was lost.
    pub fn run(steps: &[Step]) -> Result<(), String> {
        let mut run = Self::new();
        let result = steps
            .iter()
            .try_for_each(|step| run.step(step))
            .and_then(|()| run.finish());
        result.map_err(|lost| format!("{lost}\nafter:\n  {}", run.log.join("\n  ")))
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

/// Whether `metadata.json` bytes are what the project writes now: they list every ingredient
/// the folder holds, each with the md5 of its bytes in the folder or the md5 the ledger places
/// there (the app lists a file whose outside edit it has not recorded yet as the bytes it
/// placed), and the project's name (`name`, as the fold shows it).
fn describes_the_folder(fs: &FaultFs, folder: &std::path::Path, bytes: &[u8], name: &str) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    let Some(listed) = json["ingredients"].as_object() else {
        return false;
    };
    let on_disk: BTreeMap<String, Vec<u8>> = files(fs, folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with("ingredients/") && !rel.contains(".~"))
        .collect();
    if listed.len() != on_disk.len() {
        return false;
    }
    let manifest: Option<Manifest> = fs
        .peek(&folder.join(MANIFEST))
        .and_then(|b| serde_json::from_slice(&b).ok());
    let placed = |rel: &str| match manifest.as_ref().and_then(|m| m.paths.get(rel)) {
        Some(entry) => entry.md5().map(str::to_owned),
        None => None,
    };
    let same_files = listed.iter().all(|(rel, entry)| {
        let listed = entry["checksum"]["md5"].as_str();
        on_disk.get(rel).is_some_and(|bytes| {
            listed == Some(md5(bytes).as_str())
                || (listed.is_some() && listed.map(str::to_owned) == placed(rel))
        })
    });
    let shown = json["identification"]["name"]["en"]
        .as_str()
        .unwrap_or_default();
    same_files && !shown.is_empty() && name.contains(&format!("{shown:?}"))
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
        1 => a_file().prop_map(|file| Step::Damage { file }),
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
        2 => Just(Step::Open),
        2 => (1u16..=4, any::<bool>()).prop_map(|(verse, told)| Step::Edit { verse, told }),
        1 => any::<bool>().prop_map(|told| Step::EditName { told }),
        2 => (a_file(), 0u8..3, any::<bool>())
            .prop_map(|(file, back, told)| Step::Revert { file, back, told }),
        3 => a_fault(),
        1 => any::<bool>().prop_map(|take_in| Step::Choose { take_in }),
        1 => a_file().prop_map(|file| Step::Touch { file }),
        1 => Just(Step::Focus),
        2 => Just(Step::Advance),
    ]
}

/// A short sequence of steps: one step, or one of the sequences the reviews found bugs in.
fn a_chunk() -> impl Strategy<Value = Vec<Step>> {
    prop_oneof![
        8 => a_step().prop_map(|step| vec![step]),
        // A crash, then damage while the app is closed.
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
        // Bytes settled by a read after their time stamp, damaged, found by the next open's
        // storage check and held, and the person's choice.
        1 => (a_file(), any::<bool>()).prop_map(|(file, take_in)| {
            vec![
                Step::Act { act: Act::Leave, kill: None },
                Step::Advance,
                crash(),
                Step::Open,
                Step::Damage { file },
                crash(),
                Step::Open,
                Step::Advance,
                Step::Touch { file },
                Step::Focus,
                Step::Choose { take_in },
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
const ROWS_OUT_OF_REACH: [&str; 4] = ["R1", "R7", "R17", "Removed"];

fn histories(cases: u32, chunks: usize, every_row: bool) {
    // The histories the reviews found, on every run.
    for (name, history) in named_histories() {
        if let Err(lost) = Run::run(&history) {
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

/// §16.1: random histories of saves, materializations and checkpoints killed at any write
/// point, outside edits and reverts, damaged journal files and restarts lose no work.
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

/// Two writes of the book file killed after their renames, in two sessions, and a revert to
/// the bytes of the first while the app is closed: the revert is the person's, and is kept.
fn a_revert_to_an_earlier_write_of_two_stopped_ones() -> Vec<Step> {
    let usfm = Holdable::File(File::Usfm);
    vec![
        act(Act::Save(1)),
        act(Act::Leave),
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Leave),
        Step::Lock {
            what: usfm,
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        Step::Lock {
            what: usfm,
            on: false,
        },
        Step::Advance,
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]
}

/// `metadata.json` written by a group killed after its rename, and put back by another program
/// while the app is closed: the revert is the person's, and is kept.
fn a_revert_of_metadata_json_after_a_kill_past_its_rename() -> Vec<Step> {
    vec![
        act(Act::Name),
        act(Act::Leave),
        act(Act::Name),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Leave),
        Step::Advance,
        Step::Revert {
            file: File::Meta,
            back: 0,
            told: false,
        },
    ]
}

/// A file written by a group killed right after its rename, and put back while the app is
/// closed by a backup tool that keeps the old version's size and time: the restore is the
/// person's, and is kept.
fn a_restore_with_its_old_time_after_a_kill_past_its_rename(file: File) -> Vec<Step> {
    let save = match file {
        File::Usfm => Act::Save(1),
        File::Meta => Act::Name,
    };
    vec![
        act(save),
        act(Act::Leave),
        Step::Advance,
        act(save),
        Step::KillAfterRename { file },
        act(Act::Leave),
        Step::RevertKeepTime { file, back: 0 },
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

fn named_histories() -> Vec<(&'static str, Vec<Step>)> {
    vec![
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
            "a_revert_to_an_earlier_write_of_two_stopped_ones",
            a_revert_to_an_earlier_write_of_two_stopped_ones(),
        ),
        (
            "a_revert_of_metadata_json_after_a_kill_past_its_rename",
            a_revert_of_metadata_json_after_a_kill_past_its_rename(),
        ),
        (
            "a_restore_of_the_book_file_with_its_old_time",
            a_restore_with_its_old_time_after_a_kill_past_its_rename(File::Usfm),
        ),
        (
            "a_restore_of_metadata_json_with_its_old_time",
            a_restore_with_its_old_time_after_a_kill_past_its_rename(File::Meta),
        ),
        (
            "a_converged_metadata_json_written_again",
            a_converged_metadata_json_written_again(),
        ),
    ]
}

fn assert_kept(steps: &[Step]) {
    if let Err(lost) = Run::run(steps) {
        panic!("{lost}");
    }
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
fn a_revert_to_an_earlier_write_of_two_stopped_ones_is_kept() {
    assert_kept(&a_revert_to_an_earlier_write_of_two_stopped_ones());
}

#[test]
fn a_revert_of_metadata_json_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&a_revert_of_metadata_json_after_a_kill_past_its_rename());
}

#[test]
fn a_restore_of_the_book_file_with_its_old_time_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&a_restore_with_its_old_time_after_a_kill_past_its_rename(
        File::Usfm,
    ));
}

#[test]
fn a_restore_of_metadata_json_with_its_old_time_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&a_restore_with_its_old_time_after_a_kill_past_its_rename(
        File::Meta,
    ));
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
