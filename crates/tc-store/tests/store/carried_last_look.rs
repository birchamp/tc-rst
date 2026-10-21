//! A carried replacement rechecks bytes after a sharing violation, even with an equal stat.

use crate::common::*;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tc_journal::command::Command;
use tc_journal::record::CarriedValue;
use tc_journal::{Md5, Record, RecordKey, RepoPath};
use tc_store::fault::FaultFs;
use tc_store::vfs::{
    DirEntry, FileMeta, LockHandle, ReadHandle, WatchGuard, WatchSink, WriteHandle,
};
use tc_store::{Leave, OpenOptions, ProjectHandle, Store, Vfs};

const SONG: &str = "ingredients/audio/song.mp3";

#[derive(Debug)]
struct LateEdit {
    path: PathBuf,
    bytes: Vec<u8>,
    mtime_ns: i64,
}

#[derive(Debug)]
struct LateBlobLoss {
    loose: PathBuf,
    target: PathBuf,
    expected: Vec<u8>,
}

#[derive(Debug)]
struct EditOnSharingRetry {
    fs: FaultFs,
    edit: Mutex<Option<LateEdit>>,
    delete_blob: Mutex<Option<LateBlobLoss>>,
    fired: AtomicBool,
}

impl Vfs for EditOnSharingRetry {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.fs.read(path)
    }
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        self.fs.open(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.fs.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        self.fs.read_dir(path)
    }
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        self.fs.create_new(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename(from, to)
    }
    fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename_file_noreplace(from, to)
    }
    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename_dir_noreplace(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.fs.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.fs.remove_dir_all(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.fs.create_dir(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.fs.sync_dir(path)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.fs.sync_file(path)
    }
    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        self.fs.lock(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.fs.canonicalize(path)
    }
    fn secure_dir_chain(&self, path: &Path, made: &[PathBuf]) -> io::Result<()> {
        self.fs.secure_dir_chain(path, made)
    }
    fn is_held(&self, error: &io::Error) -> bool {
        self.fs.is_held(error)
    }
    fn wait_held(&self, delay: Duration) {
        self.fs.wait_held(delay);
        if let Some(loss) = self.delete_blob.lock().unwrap().take() {
            assert_eq!(self.fs.peek(&loss.target), Some(loss.expected));
            assert!(
                loss.loose.is_file(),
                "the first rename attempt still had the backup"
            );
            std::fs::remove_file(&loss.loose).unwrap();
            assert!(!loss.loose.exists());
            self.fs.set_busy(&loss.target, false);
            self.fired.store(true, Ordering::SeqCst);
        }
        let edit = self.edit.lock().unwrap().take();
        if let Some(edit) = edit {
            self.fs
                .put_unseen(&edit.path, &edit.bytes, Some(edit.mtime_ns));
            self.fs.set_busy(&edit.path, false);
            self.fired.store(true, Ordering::SeqCst);
        }
    }
    fn outside_dir(&self, path: &Path) -> io::Result<PathBuf> {
        self.fs.outside_dir(path)
    }
    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        sink: WatchSink,
    ) -> io::Result<Option<WatchGuard>> {
        self.fs.watch(root, recursive, sink)
    }
}

fn song_key() -> RecordKey {
    RecordKey::CarriedFile(RepoPath::parse(SONG).unwrap())
}

fn ingest(rig: &Rig, folder: &Path, handle: &ProjectHandle, bytes: &[u8]) -> tc_journal::ChangeId {
    rig.fs.put_unseen(&folder.join(SONG), bytes, None);
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(heads.len(), 1);
    assert_valid_burrito(&rig.fs, folder);
    heads[0]
}

fn assert_recorded_bytes(rig: &Rig, folder: &Path, handle: &ProjectHandle, bytes: &[u8]) {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let expected_md5 = Md5::of(bytes);
    let oid = fold
        .history(&song_key())
        .iter()
        .find_map(|id| match fold.body(id).map(|body| body.record) {
            Some(Record::CarriedFile {
                value: CarriedValue::Present { oid, md5, size },
                ..
            }) if md5 == expected_md5 && size == bytes.len() as u64 => Some(oid),
            _ => None,
        })
        .expect("History must retain the external recording");
    let repo = repo_of(&rig.fs, folder);
    let git_oid = git2::Oid::from_bytes(oid.as_bytes()).unwrap();
    assert_eq!(repo.find_blob(git_oid).unwrap().content(), bytes);
}

#[test]
fn a_same_stat_carried_edit_during_a_sharing_wait_is_recorded_before_replacement() {
    let fs = FaultFs::new();
    let wrapped = Arc::new(EditOnSharingRetry {
        fs: fs.clone(),
        edit: Mutex::new(None),
        delete_blob: Mutex::new(None),
        fired: AtomicBool::new(false),
    });
    let (mut settings, _) = config(&fs, APP, "host-1");
    settings.vfs = wrapped.clone();
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    let rig = Rig { fs, store };
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let first = b"first recording to restore";
    let second = b"second recording currently on disk";
    let outside = b"edited recording currently on disk";
    assert_eq!(second.len(), outside.len());
    let first_id = ingest(&rig, &folder, &handle, first);
    let second_id = ingest(&rig, &folder, &handle, second);
    let path = folder.join(SONG);
    let old_stat = rig.fs.metadata(&path).unwrap();
    assert_eq!(old_stat.len, outside.len() as u64);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let restored = handle.snapshot().fold().heads(&song_key());
    assert_eq!(restored.len(), 1);
    let restored_id = restored[0];
    assert_ne!(restored_id, first_id);
    assert_ne!(restored_id, second_id);
    *wrapped.edit.lock().unwrap() = Some(LateEdit {
        path: path.clone(),
        bytes: outside.to_vec(),
        mtime_ns: old_stat.mtime_ns,
    });
    rig.fs.set_busy(&path, true);
    let attempt = handle.leave(Leave::Mode).wait();
    // Clear the hold even if a regression never reached the planned rename.
    rig.fs.set_busy(&path, false);
    assert!(
        wrapped.fired.load(Ordering::SeqCst),
        "the edit must occur between the first carried rename and its retry"
    );
    attempt.unwrap();
    handle.wait_idle();
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_recorded_bytes(&rig, &folder, &handle, outside);
    let history = handle.snapshot().fold().history(&song_key());
    assert!(history.contains(&first_id));
    assert!(history.contains(&second_id));
    assert!(history.contains(&restored_id));
    assert_recorded_bytes(&rig, &folder, &handle, first);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_recorded_bytes(&rig, &folder, &handle, outside);
    assert_recorded_bytes(&rig, &folder, &handle, first);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn a_settled_carried_edit_during_a_sharing_wait_is_relisted_before_metadata_changes() {
    let fs = FaultFs::new();
    let wrapped = Arc::new(EditOnSharingRetry {
        fs: fs.clone(),
        edit: Mutex::new(None),
        delete_blob: Mutex::new(None),
        fired: AtomicBool::new(false),
    });
    let (mut settings, _) = config(&fs, APP, "host-1");
    settings.vfs = wrapped.clone();
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    let rig = Rig { fs, store };
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let first = b"first recording to restore";
    let second = b"second recording currently on disk";
    let outside = b"edited recording currently on disk";
    assert_eq!(second.len(), outside.len());
    let first_id = ingest(&rig, &folder, &handle, first);
    let second_id = ingest(&rig, &folder, &handle, second);
    handle.close().unwrap();

    settle_entries(&rig, &folder);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let manifest: serde_json::Value =
        serde_json::from_slice(&rig.fs.peek(&folder.join(".tc/materialized.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["paths"][SONG]["placed"]["settled"], true,
        "the precondition must exercise the settled listing fast path"
    );
    let path = folder.join(SONG);
    let old_stat = rig.fs.metadata(&path).unwrap();
    assert_eq!(rig.fs.peek(&path).as_deref(), Some(second.as_slice()));
    assert_eq!(old_stat.len, outside.len() as u64);

    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let restored = handle.snapshot().fold().heads(&song_key());
    assert_eq!(restored.len(), 1);
    let restored_id = restored[0];
    assert_ne!(restored_id, first_id);
    assert_ne!(restored_id, second_id);
    let setting = RecordKey::Setting(tc_journal::keys::SettingKey::parse("textDirection").unwrap());
    handle
        .submit(Command::SetSetting {
            shown: handle.mint(&setting, None).unwrap(),
            value: Some(tc_journal::Json::from("rtl")),
        })
        .wait()
        .unwrap();

    *wrapped.edit.lock().unwrap() = Some(LateEdit {
        path: path.clone(),
        bytes: outside.to_vec(),
        mtime_ns: old_stat.mtime_ns,
    });
    rig.fs.set_busy(&path, true);
    let attempt = handle.leave(Leave::Mode).wait();
    rig.fs.set_busy(&path, false);
    assert!(
        wrapped.fired.load(Ordering::SeqCst),
        "the edit must occur between the first carried rename and its retry"
    );
    attempt.unwrap();
    handle.wait_idle();
    let problems_after_restore = rig.store.hooks().burrito_problems();

    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_recorded_bytes(&rig, &folder, &handle, outside);
    assert_recorded_bytes(&rig, &folder, &handle, first);
    let history = handle.snapshot().fold().history(&song_key());
    assert!(history.contains(&first_id));
    assert!(history.contains(&second_id));
    assert!(history.contains(&restored_id));
    assert!(
        problems_after_restore.is_empty(),
        "the refused carried replacement left a stale settled listing: {problems_after_restore:#?}"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_recorded_bytes(&rig, &folder, &handle, outside);
    assert_recorded_bytes(&rig, &folder, &handle, first);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn a_carried_replacement_recovers_the_old_blob_lost_during_a_sharing_wait() {
    let fs = FaultFs::new();
    let wrapped = Arc::new(EditOnSharingRetry {
        fs: fs.clone(),
        edit: Mutex::new(None),
        delete_blob: Mutex::new(None),
        fired: AtomicBool::new(false),
    });
    let (mut settings, _) = config(&fs, APP, "host-1");
    settings.vfs = wrapped.clone();
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    let rig = Rig { fs, store };
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let first = b"first recording to restore";
    let second = b"second recording whose only backup will be lost during the wait";
    let first_id = ingest(&rig, &folder, &handle, first);
    let second_id = ingest(&rig, &folder, &handle, second);
    let old_oid = git2::Oid::hash_object(git2::ObjectType::Blob, second).unwrap();
    let hex = old_oid.to_string();
    let loose = git_dir_of(&rig.fs, &folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    assert!(loose.is_file());
    let path = folder.join(SONG);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    *wrapped.delete_blob.lock().unwrap() = Some(LateBlobLoss {
        loose,
        target: path.clone(),
        expected: second.to_vec(),
    });
    rig.fs.set_busy(&path, true);
    let attempt = handle.leave(Leave::Mode).wait();
    rig.fs.set_busy(&path, false);
    assert!(
        wrapped.fired.load(Ordering::SeqCst),
        "the backup vanished after the first checked rename was held"
    );
    attempt.unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&path).as_deref(), Some(first.as_slice()));
    assert_recorded_bytes(&rig, &folder, &handle, second);
    let history = handle.snapshot().fold().history(&song_key());
    assert!(history.contains(&first_id));
    assert!(history.contains(&second_id));
    assert_valid_burrito(&rig.fs, &folder);
    // No checkpoint has run since the backup disappeared: the retry rescued it.
    handle.close().unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_recorded_bytes(&rig, &folder, &handle, second);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}
