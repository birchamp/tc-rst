//! The fold's `ValueSource` over the project's files (`docs/STORAGE.md` §2.6, §14): the fold
//! keeps where each change's JSON lies, not its bytes, and reads it here when it merges,
//! lists a candidate or projects.
//!
//! A file is named by a number the loader gave it. When a seal or a consolidation removes a
//! file whose changes now lie in another (a sealed pending file, a replaced pack), the file's
//! number is moved: each offset it held maps to the place of the same bytes in the new file,
//! so a value the fold took from the old file is still read, and the old file can go.
//! Handles are opened at the first read and kept: on Windows with Defender on, opening is
//! the costly part of a read (PERFORMANCE.md, owner ruling 2026-10-16).

use crate::vfs::{ReadHandle, Vfs};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tc_journal::fold::{ValueAt, ValueSource};

enum Slot {
    File {
        path: PathBuf,
        handle: Mutex<Option<Arc<dyn ReadHandle>>>,
    },
    /// The file is gone; its values lie elsewhere.
    Moved(HashMap<u64, ValueAt>),
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File { path, .. } => write!(f, "File({})", path.display()),
            Self::Moved(map) => write!(f, "Moved({} values)", map.len()),
        }
    }
}

/// See the module documentation.
#[derive(Debug)]
pub struct FileSource {
    vfs: Arc<dyn Vfs>,
    slots: RwLock<Vec<Arc<Slot>>>,
    /// Values read, and bytes read, since the source was made.
    pub reads: AtomicU64,
    pub bytes: AtomicU64,
}

impl FileSource {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self {
            vfs,
            slots: RwLock::new(Vec::new()),
            reads: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// Gives `path` the next file number.
    pub fn add(&self, path: &Path) -> u32 {
        let mut slots = self.slots.write().unwrap_or_else(|e| e.into_inner());
        slots.push(Arc::new(Slot::File {
            path: path.to_owned(),
            handle: Mutex::new(None),
        }));
        (slots.len() - 1) as u32
    }

    /// The number `path` already has, if any.
    pub fn number_of(&self, path: &Path) -> Option<u32> {
        let slots = self.slots.read().unwrap_or_else(|e| e.into_inner());
        slots
            .iter()
            .position(|s| matches!(&**s, Slot::File { path: p, .. } if p == path))
            .map(|i| i as u32)
    }

    /// Keeps the handle the loader read file `file` through, so a value read later does
    /// not open the file again (an open reads each file once, §16.5a).
    pub fn attach(&self, file: u32, handle: Arc<dyn ReadHandle>) {
        let slots = self.slots.read().unwrap_or_else(|e| e.into_inner());
        if let Some(Slot::File { handle: held, .. }) = slots.get(file as usize).map(|s| &**s) {
            let mut held = held.lock().unwrap_or_else(|e| e.into_inner());
            if held.is_none() {
                *held = Some(handle);
            }
        }
    }

    /// File `file` is going away: each of its offsets in `map` is read at the place given
    /// instead. The handle is closed, so the file can be removed on every platform.
    pub fn moved(&self, file: u32, map: HashMap<u64, ValueAt>) {
        let mut slots = self.slots.write().unwrap_or_else(|e| e.into_inner());
        if let Some(slot) = slots.get_mut(file as usize) {
            *slot = Arc::new(Slot::Moved(map));
        }
    }

    fn slot(&self, file: u32) -> Option<Arc<Slot>> {
        self.slots
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(file as usize)
            .cloned()
    }

    fn read_located(&self, at: ValueAt, depth: u8) -> Option<Vec<u8>> {
        let slot = self.slot(at.file)?;
        match &*slot {
            Slot::Moved(map) => {
                let next = *map.get(&at.offset)?;
                if depth > 8 || next.len != at.len {
                    return None;
                }
                self.read_located(next, depth + 1)
            }
            Slot::File { path, handle } => {
                let handle = {
                    let mut held = handle.lock().unwrap_or_else(|e| e.into_inner());
                    match &*held {
                        Some(h) => h.clone(),
                        None => {
                            let opened: Arc<dyn ReadHandle> = Arc::from(self.vfs.open(path).ok()?);
                            *held = Some(opened.clone());
                            opened
                        }
                    }
                };
                let mut buf = vec![0u8; at.len as usize];
                handle.read_at(&mut buf, at.offset).ok()?;
                self.reads.fetch_add(1, Ordering::Relaxed);
                self.bytes.fetch_add(u64::from(at.len), Ordering::Relaxed);
                Some(buf)
            }
        }
    }
}

impl ValueSource for FileSource {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        self.read_located(at, 0)
    }
}
