//! The heavy part of a consolidation (`docs/STORAGE.md` §5.12 Q1 to Q3), run on a thread of
//! its own while the actor serves saves between the steps of the seal: the packs of a group
//! are read and verified, merged, the new pack is written in flushed pieces and read back, and
//! the map that moves the fold's values from each old pack to the new one is made. A
//! consolidation of a month of packs held the actor for 1.4 s as one step; the actor now only
//! applies the result (Q4), a few packs a step.

use crate::atomic::{atomic_create_dirs, atomic_write_in_pieces};
use crate::error::StoreError;
use crate::layout::Folder;
use crate::load::{JournalFile, JournalKind, relocations};
use crate::source::FileSource;
use crate::vfs::Vfs;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tc_journal::consolidate::consolidate;
use tc_journal::fold::ValueAt;
use tc_journal::index::{Section, load_pack};
use tc_journal::pack::{PackRead, PackSpec, read_pack};
use tc_journal::{ChangeId, PackId, PackName, ProjectId, Verified};

/// The most one flush carries.
const WRITE_PIECE: usize = 4 * 1024 * 1024;

pub(crate) struct ConsJob {
    pub vfs: Arc<dyn Vfs>,
    pub folder: Folder,
    pub project: ProjectId,
    pub spec: PackSpec,
    /// The repository paths of the group's packs.
    pub rels: Vec<String>,
    pub source: Arc<FileSource>,
    pub hooks: Arc<crate::hooks::Hooks>,
}

/// A pack the worker wrote and read back.
pub(crate) struct NewPack {
    pub rel: String,
    pub id: PackId,
    pub size: u64,
    pub fingerprint: u64,
    pub number: u32,
    pub section: Option<Section>,
    pub replaces: Vec<PackName>,
}

/// An old pack a new pack absorbed.
pub(crate) struct OldPack {
    pub rel: String,
    /// The new pack that holds its changes.
    pub new_rel: String,
    /// Where each of its values lies now; `None` when some change of it is in no new pack and
    /// it stays.
    pub map: Option<HashMap<u64, ValueAt>>,
}

#[derive(Default)]
pub(crate) struct ConsDone {
    pub new: Vec<NewPack>,
    pub old: Vec<OldPack>,
}

pub(crate) fn run(job: ConsJob) -> Result<ConsDone, StoreError> {
    let ConsJob {
        vfs,
        folder,
        project,
        spec,
        rels,
        source,
        hooks,
    } = job;
    let vfs: &dyn Vfs = &*vfs;
    // Q1: read and verify each; a pack that fails stays as it is.
    let mut reads: Vec<(String, Vec<u8>, PackRead)> = Vec::new();
    for rel in rels {
        let Ok(bytes) = vfs.read(&folder.path(&rel)) else {
            continue;
        };
        let read = read_pack(&rel, &bytes, project);
        if read.is_valid() {
            reads.push((rel, bytes, read));
        }
    }
    if reads.len() < 2 {
        return Ok(ConsDone::default());
    }
    let refs: Vec<&PackRead> = reads.iter().map(|(_, _, r)| r).collect();
    let new_packs =
        consolidate(&spec, &refs).map_err(|e| StoreError::Other(format!("consolidation: {e}")))?;
    drop(refs);
    hooks.wait(crate::hooks::Point::BeforeConsolidationWrite);
    let mut done = ConsDone::default();
    for pack in new_packs {
        // Q2, Q3: the packs it absorbed are those whose every change it holds; the pack read
        // back must hold them all, and only packs its header lists go.
        let holds: BTreeSet<ChangeId> = pack.changes.iter().copied().collect();
        let absorbed: Vec<usize> = reads
            .iter()
            .enumerate()
            .filter(|(_, (_, _, r))| r.entries.iter().all(|e| holds.contains(&e.id())))
            .map(|(i, _)| i)
            .collect();
        let expected: BTreeSet<ChangeId> = absorbed
            .iter()
            .flat_map(|&i| reads[i].2.entries.iter().map(Verified::id))
            .collect();
        let rel = pack.path.to_string();
        let (dir, name) = folder.dir_and_name(&rel);
        atomic_create_dirs(vfs, &dir)?;
        atomic_write_in_pieces(vfs, &dir, name, &pack.bytes, WRITE_PIECE)
            .map_err(|e| e.into_inner())?;
        let abs = folder.path(&rel);
        let back = vfs
            .read(&abs)
            .map_err(|e| StoreError::io("read back", &abs, e))?;
        let read = read_pack(&rel, &back, project);
        let ids: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
        let replaces: Vec<PackName> = read
            .header
            .as_ref()
            .map(|h| h.replaces.clone())
            .unwrap_or_default();
        if !read.is_valid() || !expected.is_subset(&ids) {
            // A pack whose bytes on disk are not the ones written is no copy of anything.
            let _ = crate::atomic::atomic_remove(vfs, &abs, crate::atomic::SyncDir::Now);
            return Err(StoreError::Other(format!(
                "the new pack {rel} did not read back as written; the old packs are kept"
            )));
        }
        let number = source.add(&abs);
        let loaded = load_pack(&rel, &back, project, number);
        let replaced: Vec<usize> = absorbed
            .into_iter()
            .filter(|&i| reads[i].2.path.is_some_and(|p| replaces.contains(&p.name)))
            .collect();
        // Where each replaced pack's values lie in this pack, from one read of its spans.
        let to_file = JournalFile {
            rel: rel.clone(),
            abs: abs.clone(),
            kind: JournalKind::Pack,
            size: back.len() as u64,
        };
        let from_files: Vec<JournalFile> = replaced
            .iter()
            .map(|&i| JournalFile {
                rel: reads[i].0.clone(),
                abs: folder.path(&reads[i].0),
                kind: JournalKind::Pack,
                size: reads[i].1.len() as u64,
            })
            .collect();
        let froms: Vec<(&JournalFile, &[u8])> = from_files
            .iter()
            .zip(&replaced)
            .map(|(f, &i)| (f, reads[i].1.as_slice()))
            .collect();
        let maps = relocations(project, &froms, &[(&to_file, number, back.as_slice())]);
        for (&i, map) in replaced.iter().zip(maps) {
            done.old.push(OldPack {
                rel: reads[i].0.clone(),
                new_rel: rel.clone(),
                map,
            });
        }
        done.new.push(NewPack {
            rel,
            id: read.id,
            size: back.len() as u64,
            fingerprint: tc_journal::index::fingerprint(&back),
            number,
            section: loaded.section,
            replaces,
        });
    }
    Ok(done)
}
