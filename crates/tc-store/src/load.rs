//! Reading the journal (`docs/STORAGE.md` §6.1 O8): every pack and pending file, in
//! parallel, each read once. A file whose path, size and fingerprint equal a section of the
//! fold cache (§3.6) takes that section's prepared changes; every other file is verified,
//! salvaged and parsed a line at a time. Each file's changes go to the fold as soon as a
//! reading thread has them, so the fold indexes while the threads read the rest.

use crate::layout::{Folder, PACKS, PENDING};
use crate::progress::Tracker;
use crate::source::FileSource;
use crate::vfs::{EntryKind, ReadHandle, Vfs};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use tc_journal::fold::{FoldInput, ValueAt};
use tc_journal::index::{Facts, FileKey, Fingerprinter, Index, Section, load_pack, load_pending};
use tc_journal::{ChangeId, ProjectId};

/// Whether a journal file is a pack or a pending file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JournalKind {
    Pack,
    Pending,
}

/// One file the listing found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalFile {
    /// The path in the project folder: a pack's repository path, `.tc/pending/<name>`.
    pub rel: String,
    pub abs: PathBuf,
    pub kind: JournalKind,
    pub size: u64,
}

/// What reading one file gave.
#[derive(Debug)]
pub struct FileRead {
    pub file: JournalFile,
    /// The loader's number for the file (the fold's `ValueAt::file`).
    pub number: u32,
    pub outcome: Result<FileFacts, io::Error>,
}

#[derive(Debug)]
pub struct FileFacts {
    pub key: FileKey,
    pub facts: Facts,
    /// The section made of the file when the cache did not stand for it; `None` when it
    /// came from the cache (the cache's own section stands) or cannot be cached.
    pub made: Option<Section>,
    /// It came from the cache.
    pub cached: bool,
    /// Changes the file held.
    pub changes: usize,
}

/// A listed file's size. A file whose stat fails (a cloud placeholder, a delete-pending file
/// on Windows, a sharing violation) is still listed, with size 0: the load opens it, fails
/// the same way, and reports it as unreadable (`FileUnreadable`, §4.4, §6.1), so one file
/// that cannot be statted never fails the listing, and the open (L65).
fn size_of(vfs: &dyn Vfs, abs: &std::path::Path) -> u64 {
    vfs.metadata(abs).map_or(0, |m| m.len)
}

/// Lists the packs and pending files of `folder`, temporary files left out (O2 handled
/// them), each with its size.
pub fn list(vfs: &dyn Vfs, folder: &Folder) -> io::Result<Vec<JournalFile>> {
    let mut out = Vec::new();
    let packs = folder.path(PACKS);
    let mut stack: Vec<(PathBuf, String)> = vec![(packs, PACKS.to_owned())];
    while let Some((dir, rel)) = stack.pop() {
        let entries = match vfs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let name = entry.name.to_string_lossy().into_owned();
            if crate::atomic::temp_target(&name).is_some() {
                continue;
            }
            let abs = dir.join(&entry.name);
            let child = format!("{rel}/{name}");
            match entry.kind {
                EntryKind::Dir => stack.push((abs, child)),
                EntryKind::File => {
                    let size = size_of(vfs, &abs);
                    out.push(JournalFile {
                        rel: child,
                        abs,
                        kind: JournalKind::Pack,
                        size,
                    });
                }
                EntryKind::Other => {}
            }
        }
    }
    let pending = folder.path(PENDING);
    match vfs.read_dir(&pending) {
        Ok(entries) => {
            for entry in entries {
                let name = entry.name.to_string_lossy().into_owned();
                if entry.kind != EntryKind::File || crate::atomic::temp_target(&name).is_some() {
                    continue;
                }
                let abs = pending.join(&entry.name);
                let size = size_of(vfs, &abs);
                out.push(JournalFile {
                    rel: format!("{PENDING}/{name}"),
                    abs,
                    kind: JournalKind::Pending,
                    size,
                });
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

const CHUNK: usize = 1 << 20;

/// Reads one file at loader number `number`, from the cache when its section stands for it.
fn load_one(
    vfs: &dyn Vfs,
    file: &JournalFile,
    project: ProjectId,
    index: Option<&Index>,
    number: u32,
    buffer: &mut Vec<u8>,
) -> io::Result<(FileFacts, Vec<FoldInput>, Arc<dyn ReadHandle>)> {
    let handle: Arc<dyn ReadHandle> = Arc::from(vfs.open(&file.abs)?);
    let size = handle.len();
    let candidate = index
        .and_then(|i| i.section_at(&file.rel))
        .filter(|s| s.key().size == size);
    if let Some(section) = candidate {
        buffer.resize(CHUNK, 0);
        let mut fingerprint = Fingerprinter::new();
        let mut at = 0u64;
        while at < size {
            let n = CHUNK.min((size - at) as usize);
            handle.read_at(&mut buffer[..n], at)?;
            fingerprint.update(&buffer[..n]);
            at += n as u64;
        }
        let key = FileKey {
            path: file.rel.clone().into(),
            size,
            fingerprint: fingerprint.finish(),
        };
        if key == *section.key()
            && let Ok(loaded) = section.load(number)
        {
            let changes = loaded.inputs.len();
            return Ok((
                FileFacts {
                    key,
                    facts: loaded.facts,
                    made: None,
                    cached: true,
                    changes,
                },
                loaded.inputs,
                handle,
            ));
        }
    }
    let mut bytes = vec![0u8; size as usize];
    handle.read_at(&mut bytes, 0)?;
    let loaded = match file.kind {
        JournalKind::Pack => load_pack(&file.rel, &bytes, project, number),
        JournalKind::Pending => load_pending(&file.rel, &bytes, project, number),
    };
    let changes = loaded.inputs.len();
    Ok((
        FileFacts {
            key: loaded.key,
            facts: loaded.facts,
            made: loaded.section,
            cached: false,
            changes,
        },
        loaded.inputs,
        handle,
    ))
}

/// The open's rules need the change ids of pending files and of packs that replace others
/// (§6.1 O7); every other pack's ids would cost 20 MB at a whole Bible's open peak (§14),
/// and the fold knows which pack holds each change anyway.
fn drop_unneeded_ids(facts: &mut Facts) {
    if let Facts::Pack(pack) = facts
        && pack.header.as_ref().is_none_or(|h| h.replaces.is_empty())
    {
        pack.ids = Vec::new();
    }
}

/// Where [`load_streaming`] reads from: the folder's files, of this project, with the fold
/// cache if there is one, on this many threads.
pub struct LoadSetup {
    pub vfs: Arc<dyn Vfs>,
    pub project: ProjectId,
    pub index: Option<Arc<Index>>,
    pub threads: usize,
}

/// Reads every file of `files` on `threads` threads and hands each file's changes to
/// `fold` as they come (see the module documentation); returns what each file gave, in the
/// order of `files`. Each file gets its number from `source` first.
pub fn load_streaming<T>(
    setup: LoadSetup,
    files: Vec<JournalFile>,
    source: &FileSource,
    tracker: &Tracker,
    fold: impl FnOnce(&mut dyn Iterator<Item = FoldInput>) -> T,
) -> (T, Vec<FileRead>) {
    let LoadSetup {
        vfs,
        project,
        index,
        threads,
    } = setup;
    let numbers: Vec<u32> = files.iter().map(|f| source.add(&f.abs)).collect();
    // Largest files first, each to the next free thread.
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(files[*i].size));
    let files = Arc::new(files);
    let order = Arc::new(order);
    let next = Arc::new(AtomicUsize::new(0));
    let threads = threads.max(1);
    // A few files in flight per thread at most: memory holds what is being folded.
    type Read = io::Result<(FileFacts, Vec<FoldInput>, Arc<dyn ReadHandle>)>;
    let (send, receive) = mpsc::sync_channel::<(usize, Read)>(threads * 2);
    for _ in 0..threads {
        let (files, order, next, send) = (files.clone(), order.clone(), next.clone(), send.clone());
        let (vfs, index, tracker) = (vfs.clone(), index.clone(), tracker.clone());
        let numbers = numbers.clone();
        rayon::spawn(move || {
            let mut buffer = Vec::new();
            while let Some(&i) = order.get(next.fetch_add(1, Ordering::Relaxed)) {
                let file = &files[i];
                let result = load_one(
                    &*vfs,
                    file,
                    project,
                    index.as_deref(),
                    numbers[i],
                    &mut buffer,
                )
                .map(|(mut facts, inputs, handle)| {
                    drop_unneeded_ids(&mut facts.facts);
                    (facts, inputs, handle)
                });
                tracker.add(1, file.size);
                if send.send((i, result)).is_err() {
                    return;
                }
            }
        });
    }
    drop(send);
    let mut results: Vec<Option<io::Result<FileFacts>>> = (0..files.len()).map(|_| None).collect();
    let mut pending: std::vec::IntoIter<FoldInput> = Vec::new().into_iter();
    let out = {
        let mut iter = std::iter::from_fn(|| {
            loop {
                if let Some(input) = pending.next() {
                    return Some(input);
                }
                let (i, result) = receive.recv().ok()?;
                match result {
                    Ok((facts, inputs, handle)) => {
                        source.attach(numbers[i], handle);
                        results[i] = Some(Ok(facts));
                        pending = inputs.into_iter();
                    }
                    Err(e) => results[i] = Some(Err(e)),
                }
            }
        });
        let out = fold(&mut iter);
        // Drain anything the fold did not take (it takes every input; this keeps the readers
        // from blocking if it stopped early).
        for _ in iter.by_ref() {}
        out
    };
    let files = Arc::try_unwrap(files).unwrap_or_else(|a| (*a).clone());
    let reads = files
        .into_iter()
        .zip(numbers)
        .zip(results)
        .map(|((file, number), outcome)| FileRead {
            file,
            number,
            outcome: outcome.unwrap_or_else(|| Err(io::Error::other("the file was not read"))),
        })
        .collect();
    (out, reads)
}

/// Where each change's JSON lies in a file, by id, from a read of its bytes.
pub fn spans_of(
    kind: JournalKind,
    rel: &str,
    bytes: &[u8],
    project: ProjectId,
) -> HashMap<ChangeId, (u64, u32)> {
    let (entries, spans) = match kind {
        JournalKind::Pack => {
            let read = tc_journal::pack::read_pack(rel, bytes, project);
            (read.entries, read.spans)
        }
        JournalKind::Pending => {
            let read = tc_journal::pending::read_pending(bytes, project);
            (read.entries, read.spans)
        }
    };
    entries
        .iter()
        .zip(spans)
        .map(|(entry, span)| (entry.id(), (span.offset, span.len)))
        .collect()
}

/// [`relocation`] of several files at once: the new files' spans are read once, not once per
/// file that goes. One map per file of `froms`, in order; `None` where a change of the file is
/// in no file of `to`.
pub fn relocations(
    project: ProjectId,
    froms: &[(&JournalFile, &[u8])],
    to: &[(&JournalFile, u32, &[u8])],
) -> Vec<Option<HashMap<u64, ValueAt>>> {
    let mut new: HashMap<ChangeId, ValueAt> = HashMap::new();
    for (file, number, bytes) in to {
        for (id, (offset, len)) in spans_of(file.kind, &file.rel, bytes, project) {
            new.entry(id).or_insert(ValueAt {
                offset,
                file: *number,
                len,
            });
        }
    }
    froms
        .iter()
        .map(|(file, bytes)| {
            let old = spans_of(file.kind, &file.rel, bytes, project);
            let mut map = HashMap::with_capacity(old.len());
            for (id, (offset, len)) in old {
                let at = new.get(&id)?;
                if at.len != len {
                    return None;
                }
                map.insert(offset, *at);
            }
            Some(map)
        })
        .collect()
}

/// The map that moves the values of file `from` (bytes `from_bytes`) to the files `to`
/// (each with its number and bytes) that hold the same changes: what [`FileSource::moved`]
/// takes before `from` is removed. A change no file of `to` holds is left out; the caller
/// keeps `from` then.
pub fn relocation(
    project: ProjectId,
    from: (&JournalFile, &[u8]),
    to: &[(&JournalFile, u32, &[u8])],
) -> Option<HashMap<u64, ValueAt>> {
    let old = spans_of(from.0.kind, &from.0.rel, from.1, project);
    let mut new: HashMap<ChangeId, ValueAt> = HashMap::new();
    for (file, number, bytes) in to {
        for (id, (offset, len)) in spans_of(file.kind, &file.rel, bytes, project) {
            new.entry(id).or_insert(ValueAt {
                offset,
                file: *number,
                len,
            });
        }
    }
    let mut map = HashMap::with_capacity(old.len());
    for (id, (offset, len)) in old {
        let at = new.get(&id)?;
        if at.len != len {
            return None;
        }
        map.insert(offset, *at);
    }
    Some(map)
}
