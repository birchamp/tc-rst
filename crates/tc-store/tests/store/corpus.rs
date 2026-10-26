//! Scale corpora (`docs/PERFORMANCE.md`) written to disk as projects through the real open,
//! seal, consolidation and materialization: the corpus's packs and pending files go into a
//! project folder whose own device is the corpus's device 0, and a checkpoint seals and
//! materializes them.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tc_corpus::{Corpus, Spec};
use tc_journal::format::FormatDoc;
use tc_store::fault::FaultFs;
use tc_store::layout::{Folder, GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{OpenOptions, Store};

/// Writes a corpus's files into `folder` on `fs`, with `project.json`, `format.json` and
/// the folder's device, as an import would leave them before its first open.
pub fn put_corpus(store: &Store, fs: &FaultFs, folder: &Path, corpus: &Corpus) {
    let journal = &corpus.journal;
    let f = Folder::new(folder);
    let project = serde_json::json!({
        "schemaVersion": 1,
        "projectId": journal.project.to_string(),
        "kind": "bible",
        "created": "2026-10-17T00:00:00.000Z",
    });
    fs.put(
        &f.path("ingredients/tc/project.json"),
        project.to_string().as_bytes(),
        None,
    );
    fs.put(
        &f.path("ingredients/tc/format.json"),
        &FormatDoc::current().to_bytes(),
        None,
    );
    fs.put(&f.path(".gitignore"), GITIGNORE_BYTES, None);
    fs.put(&f.path(".gitattributes"), GITATTRIBUTES_BYTES, None);
    for pack in &journal.packs {
        fs.put(&f.path(&pack.path), &pack.bytes, None);
    }
    for pending in &journal.pending {
        fs.put(
            &f.path(&format!(".tc/pending/{}", pending.name)),
            &pending.bytes,
            None,
        );
    }
    // The corpus's device 0 is this folder's device (§6.3).
    let print = tc_store::device::fingerprint(
        store.app_data().secret(),
        "machine-of-host-1",
        &folder.to_string_lossy(),
    );
    let device = serde_json::json!({
        "device": journal.devices[0].to_string(),
        "fingerprint": print.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "created": "2026-10-17T00:00:00.000Z",
    });
    fs.put(
        &f.path(".tc/device.json"),
        device.to_string().as_bytes(),
        None,
    );
}

/// A corpus project on a `FaultFs`: opened once (which materializes every file), sealed
/// and consolidated by a checkpoint, and closed. Each test takes a copy.
pub struct OnDisk {
    pub fs: FaultFs,
    pub folder: PathBuf,
}

fn build(spec: &Spec) -> OnDisk {
    let corpus = Corpus::generate(spec).unwrap();
    let fs = FaultFs::new();
    let rig = rig(fs.clone());
    let folder = PathBuf::from(PROJECTS).join("corpus");
    put_corpus(&rig.store, &fs, &folder, &corpus);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::AfterImport)
        .wait()
        .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
    OnDisk {
        fs: fs.crash(false),
        folder,
    }
}

pub fn psalms() -> &'static OnDisk {
    static ONE: OnceLock<OnDisk> = OnceLock::new();
    ONE.get_or_init(|| build(&Spec::psalms(1)))
}

pub fn psalms_larger() -> &'static OnDisk {
    static ONE: OnceLock<OnDisk> = OnceLock::new();
    ONE.get_or_init(|| build(&Spec::scaled(1, Spec::test_factor())))
}
