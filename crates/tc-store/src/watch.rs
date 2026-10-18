//! The file watcher of a project folder (`docs/STORAGE.md` §11.1, §5.0 "A second writer").
//!
//! The filesystem tells which paths another program changed; one thread per project gathers
//! them for a debounce interval (a program that saves a file writes it more than once) and
//! tells the writer the paths of derived and carried files, and the actor when a pending or
//! pack file it does not know has appeared (a second writer). The app's own writes are told
//! too on a real filesystem; the writer recognizes them by the entry it keeps for the path.

use crate::handle::Msg;
use crate::layout::{self, Folder};
use crate::vfs::{Vfs, WatchGuard};
use crate::writer::Job;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// A project's watches and the thread that gathers what they tell.
pub(crate) struct Watching {
    guards: Vec<WatchGuard>,
    thread: Option<JoinHandle<()>>,
    /// Dropped to end the thread once the guards no longer hold senders.
    stop: Option<Sender<Vec<PathBuf>>>,
}

impl Watching {
    /// Watches `ingredients/` (below `tc/changes/` too, for packs), the root, and
    /// `.tc/pending/`. `None` when the filesystem does not watch.
    pub(crate) fn start(
        vfs: &Arc<dyn Vfs>,
        folder: &Folder,
        debounce: Duration,
        writer: Sender<Job>,
        actor: Sender<Msg>,
    ) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
        let sink_tx = Mutex::new(tx.clone());
        let sink: crate::vfs::WatchSink = Arc::new(move |paths| {
            if let Ok(tx) = sink_tx.lock() {
                let _ = tx.send(paths);
            }
        });
        let mut guards = Vec::new();
        for (rel, recursive) in [
            (layout::INGREDIENTS, true),
            ("", false),
            (layout::PENDING, false),
        ] {
            let root = if rel.is_empty() {
                folder.root().to_owned()
            } else {
                folder.path(rel)
            };
            // A folder that cannot be watched (it is not there, the system has no watches
            // left) is left to the window focus.
            if let Ok(Some(guard)) = vfs.watch(&root, recursive, sink.clone()) {
                guards.push(guard);
            }
        }
        if guards.is_empty() {
            return None;
        }
        let root = folder.root().to_owned();
        let thread = std::thread::Builder::new()
            .name("tc-store watcher".into())
            .spawn(move || gather(&root, debounce, &rx, &writer, &actor))
            .ok()?;
        Some(Self {
            guards,
            thread: Some(thread),
            stop: Some(tx),
        })
    }

    /// Stops the watches and waits for the thread.
    pub(crate) fn stop(&mut self) {
        self.guards.clear();
        self.stop = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Collects paths until none arrives for `debounce`, then tells their owners.
fn gather(
    root: &std::path::Path,
    debounce: Duration,
    rx: &Receiver<Vec<PathBuf>>,
    writer: &Sender<Job>,
    actor: &Sender<Msg>,
) {
    let mut batch: BTreeSet<PathBuf> = BTreeSet::new();
    loop {
        let received = if batch.is_empty() {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(debounce)
        };
        match received {
            Ok(paths) => batch.extend(paths),
            Err(RecvTimeoutError::Timeout) => {
                if !tell(root, std::mem::take(&mut batch), writer, actor) {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                tell(root, std::mem::take(&mut batch), writer, actor);
                return;
            }
        }
    }
}

/// Splits the paths into the files of the project and the journal's. `false` when the
/// project has closed.
fn tell(
    root: &std::path::Path,
    paths: BTreeSet<PathBuf>,
    writer: &Sender<Job>,
    actor: &Sender<Msg>,
) -> bool {
    let mut files: Vec<String> = Vec::new();
    let mut journal = false;
    for path in paths {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        // The app's temporary files and the repository are none of the watcher's business.
        if name.starts_with(".~") || rel == ".git" || rel.starts_with(".git/") {
            continue;
        }
        if rel.starts_with(".tc/pending/") || rel.starts_with("ingredients/tc/changes/") {
            journal = true;
            continue;
        }
        if rel.starts_with(".tc/") || rel == ".tc" {
            continue;
        }
        files.push(rel);
    }
    let mut open = true;
    if journal {
        open &= actor.send(Msg::JournalChanged).is_ok();
    }
    if !files.is_empty() {
        open &= writer.send(Job::Paths(files)).is_ok();
    }
    open
}
