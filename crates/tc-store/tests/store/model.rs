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
//!   `.tc/external/` (§11.1 X1); a put the person overwrote before the app looked is retired.
//!
//! While a session runs, a save may also be only in a file in the folder (a journal file another
//! program cut in place leaves the session unable to read it; the next open reads the journal
//! again, and restores a pack HEAD holds). After an open and its first job, and at the end of a
//! run, it may not.
//!
//! The steps: saves, materializations and checkpoints, each of which can be killed before any of
//! its write points (with or without a power cut); a kill right after the rename of a file;
//! outside edits and reverts while open and while closed; pending files and packs cut at a line
//! or inside one, in place while open or while closed; restarts; the window's focus; the clock.

use crate::common::*;
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tc_journal::command::{Command, SectionValue};
use tc_journal::fold::DerivedFile;
use tc_journal::record::VerseValue;
use tc_journal::{MetaField, Record, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::{CheckpointReason, Leave, OpenOptions, ProjectHandle};

const USFM: &str = "ingredients/TIT.usfm";
const META: &str = "metadata.json";

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

/// A journal file another program cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Journal {
    Pending,
    Pack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The person acts. With `kill`, the process dies before the `at`-th write point from now
    /// (during the act, or the work it starts), with a power cut or not.
    Act { act: Act, kill: Option<(u16, bool)> },
    /// The process will die right after it next renames `file` into place.
    KillAfterRename { file: File },
    /// Another program holds a file open (`on`), or lets go of it: it cannot be read, renamed
    /// over or removed meanwhile.
    Lock { file: File, on: bool },
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
    /// Another program cuts a journal file: the `which`-th, keeping `keep` (a share of its
    /// lines), at a line end or inside a line.
    Cut {
        journal: Journal,
        which: u8,
        keep: u8,
        at_line: bool,
    },
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
    /// The bytes the app last left at each file: a put of the same bytes changes nothing.
    placed: BTreeMap<&'static str, Vec<u8>>,
    /// Whether the kill armed is a power cut.
    power_cut: bool,
    /// The files another program holds.
    locked: std::collections::BTreeSet<&'static str>,
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
            placed: BTreeMap::new(),
            power_cut: false,
            locked: std::collections::BTreeSet::new(),
            log: Vec::new(),
        };
        run.see_versions(true);
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
        self.relock();
        self.log.push(format!("crashed (power cut: {power_cut})"));
    }

    /// Whether the process died during what was just done (a kill armed may stop it at any
    /// later write point); if so, the next one starts.
    fn died(&mut self) -> bool {
        if self.fs().is_dead() {
            self.crash(self.power_cut);
            return true;
        }
        false
    }

    fn act(&mut self, act: Act, kill: Option<(u16, bool)>) -> Result<(), String> {
        self.open()?;
        if let Some((at, power_cut)) = kill {
            self.fs().kill_before(u64::from(at), false);
            self.power_cut = power_cut;
        }
        let handle = self.handle();
        let session = self.session;
        match act {
            Act::Save(v) => {
                let text = self.token();
                let handle = self.handle();
                if save_verse(handle, 1, v, &text).is_ok() {
                    self.saves.push(Save {
                        text,
                        target: Target::Verse(v),
                        session,
                        fate: Fate::Kept,
                        shown: false,
                    });
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
                if saved.is_ok() {
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
                if saved.is_ok() {
                    self.saves.push(Save {
                        text,
                        target: Target::Name,
                        session,
                        fate: Fate::Kept,
                        shown: false,
                    });
                }
            }
            Act::Leave => {
                let _ = handle.leave(Leave::Book(TIT)).wait();
            }
            Act::Checkpoint => {
                let _ = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
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
        if !self.fs().is_dead() {
            self.handle().wait_idle();
        }
        if self.died() {
            return Ok(());
        }
        self.check(false)
    }

    /// The files other programs hold, held again on a new process's filesystem.
    fn relock(&self) {
        let fs = self.fs();
        for rel in &self.locked {
            fs.set_unreadable(&self.path(rel), true);
            fs.set_busy(&self.path(rel), true);
        }
    }

    fn lock(&mut self, file: File, on: bool) -> Result<(), String> {
        if on {
            self.locked.insert(file.rel());
        } else {
            self.locked.remove(file.rel());
        }
        let fs = self.fs();
        let path = self.path(file.rel());
        fs.set_unreadable(&path, on);
        fs.set_busy(&path, on);
        if on || self.handle.is_none() {
            return Ok(());
        }
        // The writer tries a refused path again within 3 s; a leave does it now.
        let _ = self.handle().leave(Leave::Book(TIT)).wait();
        if !fs.is_dead() {
            self.handle().wait_idle();
        }
        if self.died() {
            return Ok(());
        }
        self.check(false)
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
        let placed = self.placed.get(rel) == Some(&bytes);
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
            retired: placed,
        });
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

    /// After a put while open, the store's watcher may act on it.
    fn settle_put(&mut self, told: bool) -> Result<(), String> {
        if told && self.handle.is_some() {
            std::thread::sleep(Duration::from_millis(60));
            if !self.fs().is_dead() {
                self.handle().wait_idle();
            }
            if self.died() {
                return Ok(());
            }
            return self.check(false);
        }
        Ok(())
    }

    fn focus(&mut self) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        let _ = self.handle().focus().wait();
        if !self.fs().is_dead() {
            self.handle().wait_idle();
        }
        if self.died() {
            return Ok(());
        }
        self.check(false)
    }

    fn cut(&mut self, journal: Journal, which: u8, keep: u8, at_line: bool) -> Result<(), String> {
        let fs = self.fs();
        let files: Vec<(PathBuf, Vec<u8>)> = files(&fs, &self.folder)
            .into_iter()
            .filter(|(rel, _)| match journal {
                Journal::Pending => rel.starts_with(".tc/pending/") && !rel.contains(".~"),
                Journal::Pack => rel.starts_with("ingredients/tc/changes/") && !rel.contains(".~"),
            })
            .map(|(rel, bytes)| (self.path(&rel), bytes))
            .collect();
        if files.is_empty() {
            return Ok(());
        }
        let (path, bytes) = files[usize::from(which) % files.len()].clone();
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
        self.by_another_program(|| {
            if open {
                fs.cut_in_place(&path, len);
            } else {
                fs.put_unseen(&path, &bytes[..len], None);
            }
        });
        self.log.push(format!(
            "cut {} to {len} of {} bytes",
            path.display(),
            bytes.len()
        ));
        if open { self.check(false) } else { Ok(()) }
    }

    /// Another program does `f` to the folder: a save whose every copy on disk it takes is
    /// destroyed (or held only by the session that saved it, which must write it again), and
    /// a save no file in the folder shows any more is no longer the app's to take from one.
    fn by_another_program(&mut self, f: impl FnOnce()) {
        let before: Vec<bool> = self.saves.iter().map(|s| self.on_disk(&s.text)).collect();
        let journal_before: Vec<bool> = self
            .saves
            .iter()
            .map(|s| self.in_journal(&s.text))
            .collect();
        f();
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
            if open && save.session == session {
                // The session that saved it holds it, and writes it again at its next seal
                // (§5.11 K1); until then it may write over a file that shows it.
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

    /// The no-loss invariant (see the module documentation). `settled`: an open and its first
    /// job are done, so a save shown only by a file in the folder is not kept.
    fn check(&mut self, settled: bool) -> Result<(), String> {
        if self.handle.is_none() {
            return Ok(());
        }
        let mut lost = Vec::new();
        let fs = self.fs();
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
            // While a session runs, a copy in the folder the next open reads (a journal line,
            // a pack HEAD holds, a file showing it) is not lost yet.
            if !settled && self.on_disk(&save.text) {
                continue;
            }
            // A file another program holds is not read yet: what it shows is not lost yet.
            if self.locked.iter().any(|rel| {
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
        let fs = self.fs();
        let projection = self
            .handle()
            .snapshot()
            .fold()
            .render(&DerivedFile::Usfm(TIT));
        let external: Vec<Vec<u8>> = fs
            .files_under(&self.path(".tc/external"))
            .into_values()
            .collect();
        let name = match self
            .handle()
            .snapshot()
            .fold()
            .projected(&RecordKey::ProjectMeta(MetaField::Name))
        {
            Some(Record::ProjectMeta {
                value: Some(value), ..
            }) => format!("{value:?}"),
            _ => String::new(),
        };
        let folder = self.folder.clone();
        let current_metadata = |bytes: &[u8]| describes_the_folder(&fs, &folder, bytes, &name);
        for put in &mut self.puts {
            if put.retired {
                continue;
            }
            let on_disk =
                fs.peek(&self.folder.join(put.rel)).as_deref() == Some(put.bytes.as_slice());
            let projected = match put.rel {
                USFM => projection.as_deref() == Some(put.bytes.as_slice()),
                _ => current_metadata(&put.bytes),
            };
            if projected {
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
            Ok(())
        } else {
            Err(lost.join("\n"))
        }
    }

    /// Notes the versions of the files the folder holds now; `app`: the app acted last.
    fn see_versions(&mut self, app: bool) {
        let fs = self.fs();
        for rel in [USFM, META] {
            if let Some(bytes) = fs.peek(&self.path(rel)) {
                // Bytes of a put the app has not written over yet are not the app's.
                let put = self.puts.iter().any(|p| !p.retired && p.bytes == bytes);
                if app && !put {
                    self.placed.insert(rel, bytes.clone());
                }
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
            Step::Lock { file, on } => self.lock(*file, *on),
            Step::Open => self.open(),
            Step::Edit { verse, told } => self.edit(*verse, *told),
            Step::EditName { told } => self.edit_name(*told),
            Step::Revert { file, back, told } => self.revert(*file, *back, *told, false),
            Step::RevertKeepTime { file, back } => self.revert(*file, *back, false, true),
            Step::Cut {
                journal,
                which,
                keep,
                at_line,
            } => self.cut(*journal, *which, *keep, *at_line),
            Step::Focus => self.focus(),
            Step::Advance => {
                self.fs().advance(Duration::from_secs(10));
                Ok(())
            }
        };
        self.died();
        let app = !matches!(
            step,
            Step::Edit { .. }
                | Step::EditName { .. }
                | Step::Revert { .. }
                | Step::RevertKeepTime { .. }
                | Step::Cut { .. }
        );
        self.see_versions(app);
        done
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
        for rel in std::mem::take(&mut self.locked) {
            fs.set_unreadable(&self.path(rel), false);
            fs.set_busy(&self.path(rel), false);
        }
        self.log.push("the last open".into());
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

/// Whether `metadata.json` bytes are what the project writes now: they list every ingredient
/// with the md5 of the bytes in the folder, every ingredient file the folder holds, and the
/// project's name (`name`, as the fold shows it).
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
    let same_files = listed.iter().all(|(rel, entry)| {
        on_disk.get(rel).is_some_and(|bytes| {
            entry["checksum"]["md5"].as_str() == Some(tc_journal::Md5::of(bytes).to_hex().as_str())
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

fn a_file() -> impl Strategy<Value = File> {
    prop_oneof![3 => Just(File::Usfm), 1 => Just(File::Meta)]
}

fn a_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        12 => an_act().prop_map(|act| Step::Act { act, kill: None }),
        3 => (an_act(), 0u16..40, any::<bool>())
            .prop_map(|(act, at, power)| Step::Act { act, kill: Some((at, power)) }),
        2 => a_file().prop_map(|file| Step::KillAfterRename { file }),
        1 => (a_file(), any::<bool>()).prop_map(|(file, on)| Step::Lock { file, on }),
        2 => Just(Step::Open),
        2 => (1u16..=4, any::<bool>()).prop_map(|(verse, told)| Step::Edit { verse, told }),
        1 => any::<bool>().prop_map(|told| Step::EditName { told }),
        2 => (a_file(), 0u8..3, any::<bool>())
            .prop_map(|(file, back, told)| Step::Revert { file, back, told }),
        1 => (a_file(), 0u8..3).prop_map(|(file, back)| Step::RevertKeepTime { file, back }),
        2 => (
            prop_oneof![3 => Just(Journal::Pending), 1 => Just(Journal::Pack)],
            any::<u8>(),
            any::<u8>(),
            any::<bool>()
        )
            .prop_map(|(journal, which, keep, at_line)| Step::Cut {
                journal,
                which,
                keep,
                at_line
            }),
        1 => Just(Step::Focus),
        1 => Just(Step::Advance),
    ]
}

fn histories(cases: u32, steps: usize) {
    let config = ProptestConfig {
        cases,
        max_shrink_iters: 2000,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    proptest!(config, |(history in proptest::collection::vec(a_step(), 1..steps))| {
        if let Err(lost) = Run::run(&history) {
            return Err(TestCaseError::fail(lost));
        }
    });
}

/// §16.1: random histories of saves, materializations and checkpoints killed at any write
/// point, outside edits and reverts, cut journal files and restarts lose no work.
#[test]
fn random_histories_lose_no_work() {
    histories(200, 12);
}

/// The nightly variant: more histories, and longer ones.
#[test]
#[ignore = "long: run nightly with --ignored"]
fn random_histories_lose_no_work_long() {
    histories(5000, 24);
}

// ---------------------------------------------------------------------------------------
// The cases the review of 2026-10-19 found, as histories of the model
// ---------------------------------------------------------------------------------------

fn act(act: Act) -> Step {
    Step::Act { act, kill: None }
}

/// The process dies before the close writes anything: the pending files stay unsealed.
fn crash() -> Step {
    Step::Act {
        act: Act::Close,
        kill: Some((0, false)),
    }
}

fn assert_kept(steps: &[Step]) {
    if let Err(lost) = Run::run(steps) {
        panic!("{lost}");
    }
}

/// A pending file holding one section save (one batch of two changes), cut at a line while the
/// app is closed after a crash: the book file shows both changes, and they are kept.
#[test]
fn a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file() {
    assert_kept(&[
        act(Act::Section(1)),
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 1,
            at_line: true,
        },
    ]);
}

/// The same batch cut inside a line: the change salvaged from the file waits for the member
/// the file lost, and the book file that shows both is the record of both.
#[test]
fn a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file() {
    assert_kept(&[
        act(Act::Section(1)),
        act(Act::Leave),
        crash(),
        Step::Cut {
            journal: Journal::Pending,
            which: 0,
            keep: 1,
            at_line: false,
        },
    ]);
}

/// A pending file the open read is cut in place: the value of a verse's head can no longer be
/// read, and the next job must not write the book without the verse.
#[test]
fn a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent() {
    assert_kept(&[
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
    ]);
}

/// Two writes of the book file killed after their renames, in two sessions, and a revert to
/// the bytes of the first while the app is closed: the revert is the person's, and is kept.
#[test]
fn a_revert_to_an_earlier_write_of_two_stopped_ones_is_kept() {
    assert_kept(&[
        act(Act::Save(1)),
        act(Act::Leave),
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Leave),
        Step::Lock {
            file: File::Usfm,
            on: true,
        },
        Step::Open,
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        Step::Lock {
            file: File::Usfm,
            on: false,
        },
        Step::Advance,
        Step::Revert {
            file: File::Usfm,
            back: 0,
            told: false,
        },
    ]);
}

/// `metadata.json` written by a group killed after its rename, and put back by another program
/// while the app is closed: the revert is the person's, and is kept.
#[test]
fn a_revert_of_metadata_json_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&[
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
    ]);
}

/// A book file written by a group killed right after its rename, and put back while the app
/// is closed by a backup tool that keeps the old version's size and time: the restore is the
/// person's, and is kept.
#[test]
fn a_restore_of_the_book_file_with_its_old_time_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&[
        act(Act::Save(1)),
        act(Act::Leave),
        Step::Advance,
        act(Act::Save(1)),
        Step::KillAfterRename { file: File::Usfm },
        act(Act::Leave),
        Step::RevertKeepTime {
            file: File::Usfm,
            back: 0,
        },
    ]);
}

/// The same for `metadata.json`.
#[test]
fn a_restore_of_metadata_json_with_its_old_time_after_a_kill_past_its_rename_is_kept() {
    assert_kept(&[
        act(Act::Name),
        act(Act::Leave),
        Step::Advance,
        act(Act::Name),
        Step::KillAfterRename { file: File::Meta },
        act(Act::Leave),
        Step::RevertKeepTime {
            file: File::Meta,
            back: 0,
        },
    ]);
}
