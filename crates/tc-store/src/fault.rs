//! `FaultFs` (`docs/STORAGE.md` §16.2): an in-memory filesystem that tracks, per file and per
//! directory entry, what is durable and what is only in the page cache.
//!
//! - **Durability.** A file's bytes become durable when the file is synced. A directory
//!   entry (a creation, a rename, a removal) becomes durable when its directory is synced,
//!   or, under [`PowerLoss::KeepSyncedNames`], when the file it names is synced: ext4 and
//!   NTFS commit a new file's name with its data, but not a later rename, so a power cut can
//!   keep a complete temporary file and drop the rename that followed it (§5.0 step 4).
//! - **Faults.** [`FaultFs::kill_before`] stops the writer before its n-th write point (a
//!   creation, a write, a file or directory sync, a rename, a removal, a directory
//!   creation): that operation and every later one fail, as if the process had died there.
//!   [`FaultFs::crash`] then gives the filesystem the next process sees, with or without a
//!   power cut. Other faults: an operation that fails without killing (a full disk, a flush
//!   error), reads that wait, reads that fail from some moment on, files another program
//!   holds open.
//! - **Machines.** [`FaultFs::other_machine`] shares the files but not the locks, as a
//!   folder a cloud client replicates looks to two computers.
//!
//! Paths are absolute, `/`-separated, without `.` or `..`.

use crate::vfs::{
    DirEntry, EntryKind, FileMeta, HeldByAnotherProgram, LockHandle, ReadHandle, Vfs, WriteHandle,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

type Ino = usize;

/// What a power cut drops (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerLoss {
    /// Only a directory sync makes an entry durable.
    Strict,
    /// Syncing a file also makes the entry that names it durable.
    KeepSyncedNames,
}

/// One write point of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub index: u64,
    pub kind: OpKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    Create,
    Write,
    SyncFile,
    Rename,
    RemoveFile,
    RemoveDir,
    CreateDir,
    SyncDir,
    LockWrite,
}

#[derive(Debug, Clone)]
struct FileNode {
    data: Arc<Vec<u8>>,
    durable: Option<Arc<Vec<u8>>>,
    /// The entry that names the file now, if any.
    link: Option<(Ino, OsString)>,
}

#[derive(Debug, Clone, Default)]
struct DirNode {
    entries: BTreeMap<OsString, Ino>,
    durable: BTreeMap<OsString, Ino>,
}

#[derive(Debug, Clone)]
enum Kind {
    File(FileNode),
    Dir(DirNode),
}

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    mtime_ns: i64,
}

#[derive(Debug, Clone, Default)]
struct Faults {
    kill_before: Option<u64>,
    torn: bool,
    dead: bool,
    /// The write point that just stopped the writer was a write to tear.
    tear_now: bool,
    fail_op: BTreeMap<u64, io::ErrorKind>,
    /// Every write point from this one on fails with this kind (a full disk).
    fail_from: Option<(u64, io::ErrorKind)>,
    read_delay: Option<Duration>,
    reads_fail_from: Option<Instant>,
    unreadable: BTreeSet<PathBuf>,
    busy: BTreeSet<PathBuf>,
    /// Folders whose new files another program holds, each for this many refused renames
    /// or removals (`None`: until the hold is released).
    hold_new: BTreeMap<PathBuf, Option<u32>>,
    /// Files held now, with the refusals left (`None`: until released).
    held: BTreeMap<PathBuf, Option<u32>>,
    /// The waits [`Vfs::wait_held`] was asked for.
    held_waits: Vec<Duration>,
}

#[derive(Debug, Clone)]
struct Model {
    nodes: Vec<Node>,
    faults: Faults,
    ops: u64,
    log: Vec<Op>,
    opened: Vec<PathBuf>,
    bytes_read: u64,
    power_loss: PowerLoss,
    /// The model's clock: modification times, nanoseconds since the epoch.
    now_ns: i64,
    /// Modification times are truncated to this many nanoseconds (2 s on FAT32).
    tick_ns: i64,
    /// Each write point moves the clock forward this much.
    step_ns: i64,
    /// The real folder libgit2 works in, made when the first project asks for it.
    mirror: Mirror,
}

/// The real folder beside the model where libgit2 keeps a project's `.git` (D3): git is outside
/// the `Vfs` (§16.2), so its files are real, one mirror for each independent filesystem. The
/// folder is removed when the last filesystem that uses it is dropped, and a folder left by
/// a process that no longer exists is removed by the next process that makes one.
#[derive(Debug, Clone)]
enum Mirror {
    /// Not made yet: no project asked for a repository.
    None,
    Made(Arc<MirrorDir>),
    /// A copy of the mirror could not be made; every use says why.
    Failed(Arc<str>),
}

#[derive(Debug)]
struct MirrorDir {
    root: PathBuf,
}

impl Drop for MirrorDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Where the mirrors of all filesystems in this process live.
fn mirrors_root() -> PathBuf {
    std::env::temp_dir().join("tc-faultfs")
}

/// Removes the folders of processes that ended without dropping their filesystems (a test
/// that aborted, a static corpus that is never dropped). Only on Linux, where `/proc` says
/// whether a process lives.
fn sweep_dead_mirrors(root: &Path) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_string_lossy().split('-').next().map(str::to_owned) else {
            continue;
        };
        if !pid.is_empty()
            && pid.bytes().all(|b| b.is_ascii_digit())
            && !Path::new("/proc").join(&pid).exists()
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

impl MirrorDir {
    fn make() -> io::Result<Arc<Self>> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static SWEPT: std::sync::Once = std::sync::Once::new();
        let base = mirrors_root();
        SWEPT.call_once(|| sweep_dead_mirrors(&base));
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = base.join(format!("{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        Ok(Arc::new(Self { root }))
    }

    /// The mirror of the project folder `folder`.
    fn at(&self, folder: &Path) -> PathBuf {
        let mut out = self.root.clone();
        for component in folder.components() {
            if let Component::Normal(part) = component {
                out.push(part);
            }
        }
        out
    }

    /// An independent copy: git's objects never change once written, so they are hard-linked
    /// (a copy where the filesystem will not link); every other file is copied.
    fn duplicate(&self) -> io::Result<Arc<Self>> {
        let copy = Self::make()?;
        fn walk(from: &Path, to: &Path, objects: bool) -> io::Result<()> {
            std::fs::create_dir_all(to)?;
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                let (src, dst) = (entry.path(), to.join(entry.file_name()));
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    let inside = objects || entry.file_name() == "objects";
                    walk(&src, &dst, inside)?;
                } else if objects && std::fs::hard_link(&src, &dst).is_ok() {
                    continue;
                } else {
                    std::fs::copy(&src, &dst)?;
                }
            }
            Ok(())
        }
        walk(&self.root, &copy.root, false)?;
        Ok(copy)
    }
}

impl Mirror {
    /// The mirror of an independent filesystem made from this one's files.
    fn duplicated(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Failed(why) => Self::Failed(why.clone()),
            Self::Made(dir) => match dir.duplicate() {
                Ok(copy) => Self::Made(copy),
                Err(e) => Self::Failed(format!("the mirror of git's files: {e}").into()),
            },
        }
    }

    fn dir(&self) -> Option<&Arc<MirrorDir>> {
        match self {
            Self::Made(dir) => Some(dir),
            _ => None,
        }
    }
}

const ROOT: Ino = 0;
/// 2026-10-17T00:00:00Z.
const START_NS: i64 = 1_792_195_200_000_000_000;

/// See the module documentation.
#[derive(Debug, Clone)]
pub struct FaultFs {
    model: Arc<Mutex<Model>>,
    locks: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl Default for FaultFs {
    fn default() -> Self {
        Self::new()
    }
}

fn killed() -> io::Error {
    io::Error::other("FaultFs: the writer was stopped here")
}

fn components(path: &Path) -> io::Result<Vec<OsString>> {
    let mut out = Vec::new();
    let mut rooted = false;
    for component in path.components() {
        match component {
            Component::RootDir => rooted = true,
            Component::Normal(name) => out.push(name.to_owned()),
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("FaultFs takes plain absolute paths: {}", path.display()),
                ));
            }
        }
    }
    if !rooted {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("FaultFs takes absolute paths: {}", path.display()),
        ));
    }
    Ok(out)
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} not found", path.display()),
    )
}

impl Model {
    fn new() -> Self {
        Self {
            nodes: vec![Node {
                kind: Kind::Dir(DirNode::default()),
                mtime_ns: START_NS,
            }],
            faults: Faults::default(),
            ops: 0,
            log: Vec::new(),
            opened: Vec::new(),
            bytes_read: 0,
            power_loss: PowerLoss::KeepSyncedNames,
            now_ns: START_NS,
            tick_ns: 1,
            step_ns: 1_000_000,
            mirror: Mirror::None,
        }
    }

    fn mtime(&self) -> i64 {
        self.now_ns - self.now_ns.rem_euclid(self.tick_ns)
    }

    fn dir(&self, ino: Ino) -> Option<&DirNode> {
        match &self.nodes.get(ino)?.kind {
            Kind::Dir(d) => Some(d),
            Kind::File(_) => None,
        }
    }

    fn dir_mut(&mut self, ino: Ino) -> Option<&mut DirNode> {
        match &mut self.nodes.get_mut(ino)?.kind {
            Kind::Dir(d) => Some(d),
            Kind::File(_) => None,
        }
    }

    fn file_mut(&mut self, ino: Ino) -> Option<&mut FileNode> {
        match &mut self.nodes.get_mut(ino)?.kind {
            Kind::File(f) => Some(f),
            Kind::Dir(_) => None,
        }
    }

    fn resolve(&self, path: &Path) -> io::Result<Ino> {
        let mut at = ROOT;
        for name in components(path)? {
            at = *self
                .dir(at)
                .and_then(|d| d.entries.get(&name))
                .ok_or_else(|| not_found(path))?;
        }
        Ok(at)
    }

    /// The directory that holds `path`'s last component, and that component.
    fn parent(&self, path: &Path) -> io::Result<(Ino, OsString)> {
        let mut parts = components(path)?;
        let name = parts
            .pop()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the root has no parent"))?;
        let mut at = ROOT;
        for part in parts {
            at = *self
                .dir(at)
                .and_then(|d| d.entries.get(&part))
                .ok_or_else(|| not_found(path))?;
        }
        if self.dir(at).is_none() {
            return Err(not_found(path));
        }
        Ok((at, name))
    }

    /// One write point: fails when the writer was stopped here or before.
    fn step(&mut self, kind: OpKind, path: &Path) -> io::Result<()> {
        if self.faults.dead {
            return Err(killed());
        }
        let index = self.ops;
        self.ops += 1;
        self.now_ns += self.step_ns;
        if self.faults.kill_before == Some(index) {
            self.faults.dead = true;
            self.faults.tear_now = self.faults.torn && kind == OpKind::Write;
            return Err(killed());
        }
        self.log.push(Op {
            index,
            kind,
            path: path.to_owned(),
        });
        if let Some(error) = self.faults.fail_op.remove(&index) {
            return Err(io::Error::new(
                error,
                format!("FaultFs: {kind:?} of {} failed", path.display()),
            ));
        }
        if let Some((from, error)) = self.faults.fail_from
            && index >= from
        {
            return Err(io::Error::new(
                error,
                format!("FaultFs: {kind:?} of {} failed", path.display()),
            ));
        }
        Ok(())
    }

    /// A rename or removal of `path`: refused while another program holds it (Windows).
    fn check_held(&mut self, path: &Path) -> io::Result<()> {
        if self.faults.busy.contains(path) {
            return Err(HeldByAnotherProgram::error(path));
        }
        match self.faults.held.get_mut(path) {
            None => Ok(()),
            Some(Some(0)) => {
                self.faults.held.remove(path);
                Ok(())
            }
            Some(Some(left)) => {
                *left -= 1;
                Err(HeldByAnotherProgram::error(path))
            }
            Some(None) => Err(HeldByAnotherProgram::error(path)),
        }
    }

    fn check_read(&mut self, path: &Path) -> io::Result<()> {
        if self.faults.dead {
            return Err(killed());
        }
        if let Some(from) = self.faults.reads_fail_from
            && Instant::now() >= from
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "FaultFs: the folder stopped being readable",
            ));
        }
        if self.faults.unreadable.contains(path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("FaultFs: {} is held by another program", path.display()),
            ));
        }
        Ok(())
    }

    fn unlink(&mut self, dir: Ino, name: &OsStr) -> Option<Ino> {
        let ino = self.dir_mut(dir)?.entries.remove(name)?;
        if let Some(file) = self.file_mut(ino)
            && file
                .link
                .as_ref()
                .is_some_and(|(d, n)| *d == dir && n == name)
        {
            file.link = None;
        }
        Some(ino)
    }

    fn link(&mut self, dir: Ino, name: OsString, ino: Ino) {
        if let Some(file) = self.file_mut(ino) {
            file.link = Some((dir, name.clone()));
        }
        if let Some(d) = self.dir_mut(dir) {
            d.entries.insert(name, ino);
        }
    }

    /// The filesystem as a power cut leaves it.
    fn after_power_loss(&self) -> Self {
        let mut out = self.clone();
        for node in &mut out.nodes {
            match &mut node.kind {
                Kind::File(f) => {
                    f.data = f.durable.clone().unwrap_or_default();
                    f.link = None;
                }
                Kind::Dir(d) => d.entries = d.durable.clone(),
            }
        }
        // Names come back from the durable entries.
        let links: Vec<(Ino, OsString, Ino)> = out
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(ino, n)| match &n.kind {
                Kind::Dir(d) => Some(
                    d.entries
                        .iter()
                        .map(move |(name, child)| (ino, name.clone(), *child))
                        .collect::<Vec<_>>(),
                ),
                Kind::File(_) => None,
            })
            .flatten()
            .collect();
        for (dir, name, child) in links {
            if let Some(f) = out.file_mut(child) {
                f.link = Some((dir, name));
            }
        }
        out
    }
}

impl FaultFs {
    pub fn new() -> Self {
        Self {
            model: Arc::new(Mutex::new(Model::new())),
            locks: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    fn model(&self) -> MutexGuard<'_, Model> {
        // A test thread that panicked while holding the model leaves it as it was; the
        // model has no invariant a half-done step can break.
        self.model.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The same files seen from another computer: a lock taken here is not seen there.
    pub fn other_machine(&self) -> Self {
        Self {
            model: self.model.clone(),
            locks: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    /// An independent copy of the files as they are now, with no fault armed.
    pub fn copy(&self) -> Self {
        let mut model = self.model().clone();
        model.mirror = model.mirror.duplicated();
        model.faults = Faults::default();
        model.log.clear();
        model.opened.clear();
        Self {
            model: Arc::new(Mutex::new(model)),
            locks: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    /// The files the next process finds after the writer stopped: as they are (a killed
    /// process), or as a power cut leaves them. Locks are released.
    pub fn crash(&self, power_cut: bool) -> Self {
        let model = self.model();
        let mut next = if power_cut {
            model.after_power_loss()
        } else {
            model.clone()
        };
        drop(model);
        next.mirror = next.mirror.duplicated();
        next.faults = Faults::default();
        next.log.clear();
        next.opened.clear();
        Self {
            model: Arc::new(Mutex::new(next)),
            locks: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    pub fn set_power_loss(&self, mode: PowerLoss) {
        self.model().power_loss = mode;
    }

    /// Stops the writer before its `n`-th write point from now (0 = the next one). With
    /// `torn`, a write stopped there writes the first half of its bytes.
    pub fn kill_before(&self, n: u64, torn: bool) {
        let mut model = self.model();
        model.faults.kill_before = Some(model.ops + n);
        model.faults.torn = torn;
    }

    /// Whether the writer was stopped.
    pub fn is_dead(&self) -> bool {
        self.model().faults.dead
    }

    /// The write point `n` from now fails with `kind`, and the writer goes on.
    pub fn fail_op(&self, n: u64, kind: io::ErrorKind) {
        let mut model = self.model();
        let at = model.ops + n;
        model.faults.fail_op.insert(at, kind);
    }

    /// From now on every write point fails with `kind` and the writer goes on (a full disk,
    /// a folder that became read-only); `None` ends it.
    pub fn fail_writes(&self, kind: Option<io::ErrorKind>) {
        let mut model = self.model();
        model.faults.fail_from = kind.map(|k| (model.ops, k));
    }

    /// Write points so far.
    pub fn ops(&self) -> u64 {
        self.model().ops
    }

    /// Every write point so far, in order.
    pub fn log(&self) -> Vec<Op> {
        self.model().log.clone()
    }

    pub fn clear_log(&self) {
        let mut model = self.model();
        model.log.clear();
        model.opened.clear();
        model.bytes_read = 0;
    }

    /// Every file opened for reading since the log was cleared, in order.
    pub fn opened(&self) -> Vec<PathBuf> {
        self.model().opened.clone()
    }

    pub fn bytes_read(&self) -> u64 {
        self.model().bytes_read
    }

    /// Every read waits this long.
    pub fn set_read_delay(&self, delay: Option<Duration>) {
        self.model().faults.read_delay = delay;
    }

    /// Every read from `at` on fails.
    pub fn fail_reads_from(&self, at: Option<Instant>) {
        self.model().faults.reads_fail_from = at;
    }

    /// Another program holds `path` open: reads fail with a sharing violation.
    pub fn set_unreadable(&self, path: &Path, on: bool) {
        let mut model = self.model();
        if on {
            model.faults.unreadable.insert(path.to_owned());
        } else {
            model.faults.unreadable.remove(path);
        }
    }

    /// Another program holds `path` open: a rename onto it or from it, and a removal of it,
    /// fail (Windows), with the error a held file gives.
    pub fn set_busy(&self, path: &Path, on: bool) {
        let mut model = self.model();
        if on {
            model.faults.busy.insert(path.to_owned());
        } else {
            model.faults.busy.remove(path);
        }
    }

    /// What a scanner does: every file created in `dir` from now on is held for its next
    /// `tries` renames or removals (`None`: until [`Self::release_new_files`]). Holds do
    /// not survive [`Self::copy`] or [`Self::crash`].
    pub fn hold_new_files(&self, dir: &Path, tries: Option<u32>) {
        self.model().faults.hold_new.insert(dir.to_owned(), tries);
    }

    /// The scanner lets go of the files of `dir`, and of the ones it will create.
    pub fn release_new_files(&self, dir: &Path) {
        let mut model = self.model();
        model.faults.hold_new.remove(dir);
        model
            .faults
            .held
            .retain(|path, _| path.parent() != Some(dir));
    }

    /// The waits the helpers of [`crate::atomic`] made for a held file, in order.
    pub fn held_waits(&self) -> Vec<Duration> {
        self.model().faults.held_waits.clone()
    }

    /// Modification times are truncated to `tick` (2 s on FAT32, §6.2).
    pub fn set_tick(&self, tick: Duration) {
        self.model().tick_ns = i64::try_from(tick.as_nanos()).unwrap_or(i64::MAX).max(1);
    }

    /// Moves the clock forward.
    pub fn advance(&self, by: Duration) {
        self.model().now_ns += i64::try_from(by.as_nanos()).unwrap_or(i64::MAX);
    }

    /// The clock, in nanoseconds since the epoch.
    pub fn now_ns(&self) -> i64 {
        self.model().now_ns
    }

    /// Writes `bytes` at `path` as another program would, creating the parents: no write
    /// point, durable at once, the modification time now (or `mtime_ns`).
    pub fn put(&self, path: &Path, bytes: &[u8], mtime_ns: Option<i64>) {
        let mut model = self.model();
        let Ok(parts) = components(path) else {
            return;
        };
        let mut at = ROOT;
        let now = model.mtime();
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            let existing = model.dir(at).and_then(|d| d.entries.get(part)).copied();
            at = match existing {
                Some(ino) if !last => ino,
                _ if !last => {
                    let ino = model.nodes.len();
                    model.nodes.push(Node {
                        kind: Kind::Dir(DirNode::default()),
                        mtime_ns: now,
                    });
                    if let Some(d) = model.dir_mut(at) {
                        d.entries.insert(part.clone(), ino);
                        d.durable.insert(part.clone(), ino);
                    }
                    ino
                }
                _ => {
                    let data = Arc::new(bytes.to_vec());
                    let ino = model.nodes.len();
                    model.nodes.push(Node {
                        kind: Kind::File(FileNode {
                            data: data.clone(),
                            durable: Some(data),
                            link: Some((at, part.clone())),
                        }),
                        mtime_ns: mtime_ns.unwrap_or(now),
                    });
                    if let Some(old) = model.unlink(at, part) {
                        let _ = old;
                    }
                    if let Some(d) = model.dir_mut(at) {
                        d.entries.insert(part.clone(), ino);
                        d.durable.insert(part.clone(), ino);
                    }
                    ino
                }
            };
        }
    }

    /// Sets a file's modification time, as `touch -d` or a sync client does.
    pub fn set_mtime(&self, path: &Path, mtime_ns: i64) -> io::Result<()> {
        let mut model = self.model();
        let ino = model.resolve(path)?;
        model.nodes[ino].mtime_ns = mtime_ns;
        Ok(())
    }

    /// Removes a file or folder as another program would: no write point, durable at once.
    pub fn delete(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let (dir, name) = model.parent(path)?;
        model.unlink(dir, &name).ok_or_else(|| not_found(path))?;
        if let Some(d) = model.dir_mut(dir) {
            d.durable.remove(&name);
        }
        Ok(())
    }

    /// The bytes at `path`, read without a fault or a count.
    pub fn peek(&self, path: &Path) -> Option<Vec<u8>> {
        let model = self.model();
        let ino = model.resolve(path).ok()?;
        match &model.nodes[ino].kind {
            Kind::File(f) => Some(f.data.to_vec()),
            Kind::Dir(_) => None,
        }
    }

    /// Every file under `dir`, by path, read without a fault or a count.
    pub fn files_under(&self, dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let model = self.model();
        let mut out = BTreeMap::new();
        let Ok(start) = model.resolve(dir) else {
            return out;
        };
        let mut stack = vec![(dir.to_owned(), start)];
        while let Some((path, ino)) = stack.pop() {
            match &model.nodes[ino].kind {
                Kind::File(f) => {
                    out.insert(path, f.data.to_vec());
                }
                Kind::Dir(d) => {
                    for (name, child) in &d.entries {
                        stack.push((path.join(name), *child));
                    }
                }
            }
        }
        out
    }
}

struct FaultRead {
    fs: FaultFs,
    path: PathBuf,
    data: Arc<Vec<u8>>,
}

impl ReadHandle for FaultRead {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        let delay = {
            let mut model = self.fs.model();
            model.check_read(&self.path)?;
            model.bytes_read += buf.len() as u64;
            model.faults.read_delay
        };
        if let Some(delay) = delay {
            std::thread::sleep(delay);
        }
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::UnexpectedEof)?;
        let end = start
            .checked_add(buf.len())
            .filter(|end| *end <= self.data.len())
            .ok_or(io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

struct FaultWrite {
    fs: FaultFs,
    path: PathBuf,
    ino: Ino,
}

impl WriteHandle for FaultWrite {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut model = self.fs.model();
        if let Err(error) = model.step(OpKind::Write, &self.path) {
            if std::mem::take(&mut model.faults.tear_now) {
                let half = bytes.len() / 2;
                let mtime = model.mtime();
                if let Some(f) = model.file_mut(self.ino) {
                    Arc::make_mut(&mut f.data).extend_from_slice(&bytes[..half]);
                }
                model.nodes[self.ino].mtime_ns = mtime;
            }
            return Err(error);
        }
        let mtime = model.mtime();
        if let Some(f) = model.file_mut(self.ino) {
            Arc::make_mut(&mut f.data).extend_from_slice(bytes);
        }
        model.nodes[self.ino].mtime_ns = mtime;
        Ok(())
    }

    fn sync_all(&mut self) -> io::Result<()> {
        let mut model = self.fs.model();
        model.step(OpKind::SyncFile, &self.path)?;
        sync_inode(&mut model, self.ino);
        Ok(())
    }
}

fn sync_inode(model: &mut Model, ino: Ino) {
    let keep_name = model.power_loss == PowerLoss::KeepSyncedNames;
    let Some(file) = model.file_mut(ino) else {
        return;
    };
    file.durable = Some(file.data.clone());
    let link = file.link.clone();
    if keep_name
        && let Some((dir, name)) = link
        && let Some(d) = model.dir_mut(dir)
    {
        d.durable.insert(name, ino);
    }
}

struct FaultLock {
    fs: FaultFs,
    path: PathBuf,
}

impl LockHandle for FaultLock {
    fn set_content(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut model = self.fs.model();
        model.step(OpKind::LockWrite, &self.path)?;
        let ino = model.resolve(&self.path)?;
        let mtime = model.mtime();
        if let Some(f) = model.file_mut(ino) {
            f.data = Arc::new(bytes.to_vec());
            f.durable = Some(f.data.clone());
        }
        model.nodes[ino].mtime_ns = mtime;
        Ok(())
    }
}

impl Drop for FaultLock {
    fn drop(&mut self) {
        self.fs
            .locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.path);
    }
}

impl Vfs for FaultFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let handle = self.open(path)?;
        let mut out = vec![0; handle.len() as usize];
        handle.read_at(&mut out, 0)?;
        Ok(out)
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        let delay = {
            let mut model = self.model();
            model.check_read(path)?;
            let ino = model.resolve(path)?;
            let data = match &model.nodes[ino].kind {
                Kind::File(f) => f.data.clone(),
                Kind::Dir(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::IsADirectory,
                        format!("{} is a folder", path.display()),
                    ));
                }
            };
            model.opened.push(path.to_owned());
            let delay = model.faults.read_delay;
            (delay, data)
        };
        if let Some(delay) = delay.0 {
            std::thread::sleep(delay);
        }
        Ok(Box::new(FaultRead {
            fs: self.clone(),
            path: path.to_owned(),
            data: delay.1,
        }))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        let mut model = self.model();
        model.check_read(path)?;
        let ino = model.resolve(path)?;
        let node = &model.nodes[ino];
        Ok(match &node.kind {
            Kind::File(f) => FileMeta {
                len: f.data.len() as u64,
                mtime_ns: node.mtime_ns,
                kind: EntryKind::File,
            },
            Kind::Dir(_) => FileMeta {
                len: 0,
                mtime_ns: node.mtime_ns,
                kind: EntryKind::Dir,
            },
        })
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut model = self.model();
        model.check_read(path)?;
        let ino = model.resolve(path)?;
        let dir = model.dir(ino).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is a file", path.display()),
            )
        })?;
        Ok(dir
            .entries
            .iter()
            .map(|(name, child)| DirEntry {
                name: name.clone(),
                kind: match model.nodes[*child].kind {
                    Kind::File(_) => EntryKind::File,
                    Kind::Dir(_) => EntryKind::Dir,
                },
            })
            .collect())
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        let mut model = self.model();
        let (dir, name) = model.parent(path)?;
        if model
            .dir(dir)
            .is_some_and(|d| d.entries.contains_key(&name))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists", path.display()),
            ));
        }
        model.step(OpKind::Create, path)?;
        let ino = model.nodes.len();
        let mtime = model.mtime();
        model.nodes.push(Node {
            kind: Kind::File(FileNode {
                data: Arc::new(Vec::new()),
                durable: None,
                link: None,
            }),
            mtime_ns: mtime,
        });
        model.link(dir, name, ino);
        if let Some(tries) = path
            .parent()
            .and_then(|d| model.faults.hold_new.get(d))
            .copied()
        {
            model.faults.held.insert(path.to_owned(), tries);
        }
        Ok(Box::new(FaultWrite {
            fs: self.clone(),
            path: path.to_owned(),
            ino,
        }))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut model = self.model();
        model.check_held(from)?;
        model.check_held(to)?;
        let (from_dir, from_name) = model.parent(from)?;
        let ino = model.resolve(from)?;
        let (to_dir, to_name) = model.parent(to)?;
        if let Some(existing) = model
            .dir(to_dir)
            .and_then(|d| d.entries.get(&to_name))
            .copied()
            && model.dir(existing).is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} is a folder", to.display()),
            ));
        }
        model.step(OpKind::Rename, to)?;
        model.unlink(from_dir, &from_name);
        model.unlink(to_dir, &to_name);
        model.link(to_dir, to_name, ino);
        model.faults.held.remove(from);
        Ok(())
    }

    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut model = self.model();
        let (from_dir, from_name) = model.parent(from)?;
        let ino = model.resolve(from)?;
        let (to_dir, to_name) = model.parent(to)?;
        if model
            .dir(to_dir)
            .is_some_and(|d| d.entries.contains_key(&to_name))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists", to.display()),
            ));
        }
        model.step(OpKind::Rename, to)?;
        model.unlink(from_dir, &from_name);
        model.link(to_dir, to_name, ino);
        if let Some(dir) = model.mirror.dir() {
            let (src, dst) = (dir.at(from), dir.at(to));
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(src, dst);
            }
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let (dir, name) = model.parent(path)?;
        let ino = model.resolve(path)?;
        model.check_held(path)?;
        if model.dir(ino).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is a folder", path.display()),
            ));
        }
        model.step(OpKind::RemoveFile, path)?;
        model.unlink(dir, &name);
        model.faults.held.remove(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let (dir, name) = model.parent(path)?;
        model.resolve(path)?;
        model.step(OpKind::RemoveDir, path)?;
        model.unlink(dir, &name);
        if let Some(dir) = model.mirror.dir() {
            let _ = std::fs::remove_dir_all(dir.at(path));
        }
        Ok(())
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let (dir, name) = model.parent(path)?;
        if model
            .dir(dir)
            .is_some_and(|d| d.entries.contains_key(&name))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists", path.display()),
            ));
        }
        model.step(OpKind::CreateDir, path)?;
        let ino = model.nodes.len();
        let mtime = model.mtime();
        model.nodes.push(Node {
            kind: Kind::Dir(DirNode::default()),
            mtime_ns: mtime,
        });
        model.link(dir, name, ino);
        Ok(())
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let ino = model.resolve(path)?;
        model.step(OpKind::SyncDir, path)?;
        if let Some(d) = model.dir_mut(ino) {
            d.durable = d.entries.clone();
        }
        Ok(())
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        let mut model = self.model();
        let ino = model.resolve(path)?;
        model.step(OpKind::SyncFile, path)?;
        sync_inode(&mut model, ino);
        Ok(())
    }

    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        {
            let mut model = self.model();
            if model.faults.dead {
                return Err(killed());
            }
            if model.resolve(path).is_err() {
                let (dir, name) = model.parent(path)?;
                model.step(OpKind::Create, path)?;
                let ino = model.nodes.len();
                let mtime = model.mtime();
                model.nodes.push(Node {
                    kind: Kind::File(FileNode {
                        data: Arc::new(Vec::new()),
                        durable: Some(Arc::new(Vec::new())),
                        link: None,
                    }),
                    mtime_ns: mtime,
                });
                model.link(dir, name.clone(), ino);
                if let Some(d) = model.dir_mut(dir) {
                    d.durable.insert(name, ino);
                }
            }
        }
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if !locks.insert(path.to_owned()) {
            return Ok(None);
        }
        Ok(Some(Box::new(FaultLock {
            fs: self.clone(),
            path: path.to_owned(),
        })))
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let model = self.model();
        model.resolve(path)?;
        Ok(path.to_owned())
    }

    fn wait_held(&self, delay: Duration) {
        self.model().faults.held_waits.push(delay);
    }

    fn outside_dir(&self, folder: &Path) -> io::Result<PathBuf> {
        let mut model = self.model();
        if matches!(model.mirror, Mirror::None) {
            model.mirror = Mirror::Made(MirrorDir::make()?);
        }
        let dir = match &model.mirror {
            Mirror::Made(dir) => dir.at(folder),
            Mirror::Failed(why) => return Err(io::Error::other(why.to_string())),
            Mirror::None => return Err(io::Error::other("no mirror")),
        };
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn write(fs: &FaultFs, path: &str, bytes: &[u8], sync: bool) {
        let mut f = fs.create_new(&p(path)).unwrap();
        f.write_all(bytes).unwrap();
        if sync {
            f.sync_all().unwrap();
        }
    }

    #[test]
    fn a_power_cut_drops_what_was_not_synced() {
        let fs = FaultFs::new();
        fs.set_power_loss(PowerLoss::Strict);
        fs.create_dir(&p("/a")).unwrap();
        fs.sync_dir(&p("/")).unwrap();
        write(&fs, "/a/synced", b"one", true);
        fs.sync_dir(&p("/a")).unwrap();
        write(&fs, "/a/unsynced", b"two", false);
        write(&fs, "/a/data-only", b"three", true);
        let after = fs.crash(true);
        assert_eq!(after.peek(&p("/a/synced")), Some(b"one".to_vec()));
        assert_eq!(after.peek(&p("/a/unsynced")), None);
        assert_eq!(
            after.peek(&p("/a/data-only")),
            None,
            "its name was never synced"
        );
        let killed = fs.crash(false);
        assert_eq!(killed.peek(&p("/a/unsynced")), Some(b"two".to_vec()));
    }

    #[test]
    fn a_power_cut_can_keep_a_synced_temp_file_and_drop_its_rename() {
        let fs = FaultFs::new();
        fs.create_dir(&p("/d")).unwrap();
        fs.sync_dir(&p("/")).unwrap();
        write(&fs, "/d/.~x.tmp", b"saved", true);
        fs.rename(&p("/d/.~x.tmp"), &p("/d/x")).unwrap();
        let cut = fs.crash(true);
        assert_eq!(cut.peek(&p("/d/x")), None);
        assert_eq!(cut.peek(&p("/d/.~x.tmp")), Some(b"saved".to_vec()));
        fs.sync_dir(&p("/d")).unwrap();
        let cut = fs.crash(true);
        assert_eq!(cut.peek(&p("/d/x")), Some(b"saved".to_vec()));
        assert_eq!(cut.peek(&p("/d/.~x.tmp")), None);
    }

    #[test]
    fn a_stopped_writer_fails_from_its_kill_point_on() {
        let fs = FaultFs::new();
        fs.kill_before(1, true);
        let mut f = fs.create_new(&p("/f")).unwrap();
        assert!(f.write_all(b"abcd").is_err());
        assert!(fs.is_dead());
        assert!(fs.create_dir(&p("/g")).is_err());
        let next = fs.crash(false);
        assert_eq!(next.peek(&p("/f")), Some(b"ab".to_vec()), "a torn write");
        assert!(next.create_dir(&p("/g")).is_ok());
    }

    #[test]
    fn locks_are_per_machine() {
        let fs = FaultFs::new();
        let first = fs.lock(&p("/lock")).unwrap();
        assert!(first.is_some());
        assert!(fs.lock(&p("/lock")).unwrap().is_none());
        assert!(fs.other_machine().lock(&p("/lock")).unwrap().is_some());
        drop(first);
        assert!(fs.lock(&p("/lock")).unwrap().is_some());
    }

    #[test]
    fn modification_times_follow_the_tick() {
        let fs = FaultFs::new();
        fs.set_tick(Duration::from_secs(2));
        write(&fs, "/a", b"1", true);
        let first = fs.metadata(&p("/a")).unwrap().mtime_ns;
        assert_eq!(first % 2_000_000_000, 0);
        fs.advance(Duration::from_secs(3));
        write(&fs, "/b", b"1", true);
        assert!(fs.metadata(&p("/b")).unwrap().mtime_ns > first);
    }

    /// Git's files live in a real folder beside the model. A copy and a crash get a folder of
    /// their own with the same files; a renamed or removed folder takes its git files along;
    /// and the folder goes when the filesystem does.
    #[test]
    fn the_mirror_follows_copy_crash_rename_and_removal() {
        let fs = FaultFs::new();
        fs.create_dir(&p("/projects")).unwrap();
        fs.create_dir(&p("/projects/.staging-x")).unwrap();
        let outside = fs.outside_dir(&p("/projects/.staging-x")).unwrap();
        std::fs::create_dir_all(outside.join(".git/objects/ab")).unwrap();
        std::fs::write(outside.join(".git/objects/ab/cdef"), b"object").unwrap();
        std::fs::write(outside.join(".git/HEAD"), b"ref: refs/heads/main").unwrap();
        assert_eq!(fs.outside_dir(&p("/projects/.staging-x")).unwrap(), outside);
        // A rename of the project folder moves its git files.
        fs.rename_dir_noreplace(&p("/projects/.staging-x"), &p("/projects/Titus"))
            .unwrap();
        let moved = fs.outside_dir(&p("/projects/Titus")).unwrap();
        assert_eq!(
            std::fs::read(moved.join(".git/objects/ab/cdef")).unwrap(),
            b"object"
        );
        assert!(!outside.exists());
        // A copy has a folder of its own: changes to either do not reach the other.
        let copy = fs.copy();
        let copied = copy.outside_dir(&p("/projects/Titus")).unwrap();
        assert_ne!(copied, moved);
        assert_eq!(
            std::fs::read(copied.join(".git/objects/ab/cdef")).unwrap(),
            b"object"
        );
        std::fs::write(moved.join(".git/HEAD"), b"changed").unwrap();
        assert_eq!(
            std::fs::read(copied.join(".git/HEAD")).unwrap(),
            b"ref: refs/heads/main"
        );
        // So does a crash, power cut or not.
        for cut in [false, true] {
            let after = fs.crash(cut);
            let kept = after.outside_dir(&p("/projects/Titus")).unwrap();
            assert_ne!(kept, moved);
            assert_eq!(std::fs::read(kept.join(".git/HEAD")).unwrap(), b"changed");
        }
        // A removal takes the git files along.
        copy.remove_dir_all(&p("/projects/Titus")).unwrap();
        assert!(!copied.exists());
        // The folders go with their filesystems.
        drop(copy);
        drop(fs);
        assert!(!moved.exists());
    }
}
