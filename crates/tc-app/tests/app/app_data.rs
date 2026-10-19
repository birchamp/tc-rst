//! The app-data folder on a disk (unit C1a, APP-DESIGN §4.9): every file is replaced by
//! `tc_store::atomic_write` off the runtime, a crash at any write point leaves the old bytes or
//! the new, and what the person chose survives a relaunch (tC4 #525).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tc_api::ids::{LocaleId, ProjectId, WallMs};
use tc_api::prefs::{PlaceChange, PlaceMode, PlaceUpdate, SetLocale};
use tc_app::FilesChoice;
use tc_app::backend::SummaryStamp;
use tc_app::clock::ManualClock;
use tc_app::prefs::{AppDataFiles, AppDataPath, DiskFiles, PrefsWriter, StoredSummary, Written};
use tc_app::reports::{ReportLog, ReportOp};
use tc_store::Vfs;
use tc_store::fault::FaultFs;
use tc_store::vfs::{DirEntry, FileMeta, LockHandle, ReadHandle, WriteHandle};

use super::common::{TestApp, hello_value, last_edit, rid};

const ROOT: &str = "/app-data";

fn root() -> PathBuf {
    PathBuf::from(ROOT)
}

fn disk(fs: &FaultFs) -> Arc<DiskFiles> {
    Arc::new(DiskFiles::open(Arc::new(fs.clone()), &root()))
}

/// Every file under the app-data folder, by its path below it.
fn files_of(fs: &FaultFs) -> Vec<String> {
    fs.files_under(&root())
        .into_keys()
        .map(|p| {
            p.strip_prefix(ROOT)
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('/')
                .to_owned()
        })
        .collect()
}

fn temporary(name: &str) -> bool {
    name.rsplit('/').next().unwrap().starts_with(".~")
}

#[test]
fn disk_files_write_atomically_and_read_back() {
    let fs = FaultFs::new();
    let files = disk(&fs);
    let path = AppDataPath::app_prefs();
    assert_eq!(files.read(&path).unwrap(), None, "missing reads as none");
    assert_eq!(files.write(&path, b"{\"a\":1}").unwrap(), Written::Durable);
    assert_eq!(files.read(&path).unwrap().unwrap(), b"{\"a\":1}");
    assert_eq!(files.write(&path, b"{\"a\":2}").unwrap(), Written::Durable);
    assert_eq!(files.read(&path).unwrap().unwrap(), b"{\"a\":2}");
    assert_eq!(
        fs.peek(Path::new("/app-data/prefs/app.json")).unwrap(),
        b"{\"a\":2}",
        "the file is at prefs/app.json, part by part"
    );
    files.remove(&path).unwrap();
    assert_eq!(files.read(&path).unwrap(), None);
    files.remove(&path).unwrap();
    assert!(
        files_of(&fs).iter().all(|f| !temporary(f)),
        "{:?}",
        files_of(&fs)
    );
    assert!(files.blocks());
}

#[test]
fn a_write_whose_folder_flush_fails_is_written_not_durable() {
    let fs = FaultFs::new();
    let files = disk(&fs);
    let path = AppDataPath::app_prefs();
    files.write(&path, b"{}").unwrap();
    // create, write, flush the file, rename, then the folder's flush.
    fs.fail_op(4, io::ErrorKind::Other);
    assert!(matches!(
        files.write(&path, b"{\"b\":1}").unwrap(),
        Written::NotDurable(_)
    ));
    assert_eq!(files.read(&path).unwrap().unwrap(), b"{\"b\":1}");
    fs.fail_op(0, io::ErrorKind::Other);
    assert!(files.write(&path, b"{\"c\":1}").is_err());
    assert_eq!(
        files.read(&path).unwrap().unwrap(),
        b"{\"b\":1}",
        "a write that failed before its rename changed nothing"
    );
}

#[test]
fn leftover_temporary_files_are_removed_and_others_are_not() {
    let fs = FaultFs::new();
    let planted = [
        "prefs/.~app.json.0123456789abcdef.tmp",
        "summaries/.~0011.json.0123456789abcdef.tmp",
        ".~reports.json.fedcba9876543210.tmp",
        ".~installation.json.0123456789abcdef.tmp",
        "prefs/notes.txt",
        "prefs/app.json",
    ];
    for rel in planted {
        fs.put(&root().join(rel), b"x", None);
    }
    let _files = disk(&fs);
    assert_eq!(
        files_of(&fs),
        [
            ".~installation.json.0123456789abcdef.tmp",
            "prefs/app.json",
            "prefs/notes.txt",
        ],
        "only this writer's leftovers went; tc-store's are its own"
    );
}

async fn wait_for_files(prefs: &PrefsWriter) {
    // A write the task already took is finished once a flush behind it answers.
    prefs.flush().await;
}

/// The write points of one `set_locale` over a file that exists.
async fn locale_write_points() -> u64 {
    let fs = FaultFs::new();
    fs.put(
        Path::new("/app-data/prefs/app.json"),
        br#"{"appLocale":"en"}"#,
        None,
    );
    let prefs = PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
    let before = fs.ops();
    prefs
        .set_locale(LocaleId::parse("ar").unwrap())
        .await
        .unwrap();
    fs.ops() - before
}

#[tokio::test]
async fn a_crash_at_any_write_point_of_a_prefs_file_leaves_the_old_or_the_new_bytes() {
    let points = locale_write_points().await;
    assert!(points >= 4, "create, write, flush, rename, flush: {points}");
    let mut seen = std::collections::BTreeSet::new();
    for n in 0..=points {
        for torn in [false, true] {
            for power_cut in [false, true] {
                let fs = FaultFs::new();
                fs.put(
                    Path::new("/app-data/prefs/app.json"),
                    br#"{"appLocale":"en"}"#,
                    None,
                );
                let prefs =
                    PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
                fs.kill_before(n, torn);
                let _ = prefs.set_locale(LocaleId::parse("ar").unwrap()).await;
                drop(prefs);
                let next = fs.crash(power_cut);
                let reports = ReportLog::new();
                let again =
                    PrefsWriter::start(disk(&next), reports.clone(), Arc::new(ManualClock::new(2)));
                let locale = again.app_locale();
                assert!(
                    ["en", "ar"].contains(&locale.as_str()),
                    "point {n}, torn {torn}, power cut {power_cut}: {locale:?}"
                );
                seen.insert(locale.as_str().to_owned());
                assert!(
                    reports.all().iter().all(|r| r.op != ReportOp::PrefsWrite),
                    "point {n}: the file reads as a JSON object: {:?}",
                    reports.all()
                );
                assert!(
                    files_of(&next).iter().all(|f| !temporary(f)),
                    "point {n}: the next start removed the leftovers: {:?}",
                    files_of(&next)
                );
            }
        }
    }
    assert_eq!(seen.len(), 2, "both outcomes happen: {seen:?}");
}

/// A file system whose file creations wait until the test lets them go.
#[derive(Debug, Clone)]
struct HeldFs {
    inner: FaultFs,
    gate: Arc<(Mutex<(bool, u32)>, Condvar)>,
}

impl HeldFs {
    fn new(inner: FaultFs) -> Self {
        Self {
            inner,
            gate: Arc::new((Mutex::new((true, 0)), Condvar::new())),
        }
    }

    fn release(&self) {
        let (lock, changed) = &*self.gate;
        lock.lock().unwrap().0 = false;
        changed.notify_all();
    }

    fn held(&self) -> u32 {
        self.gate.0.lock().unwrap().1
    }
}

impl Vfs for HeldFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        self.inner.open(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.inner.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        self.inner.read_dir(path)
    }
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        let (lock, changed) = &*self.gate;
        let mut state = lock.lock().unwrap();
        state.1 += 1;
        changed.notify_all();
        while state.0 {
            state = changed.wait(state).unwrap();
        }
        drop(state);
        self.inner.create_new(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }
    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename_dir_noreplace(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_dir(path)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_file(path)
    }
    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        self.inner.lock(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
}

/// A write held on the disk leaves the runtime free: on a runtime of one thread, a timer still
/// fires while `set_locale` waits in its write. A write on the runtime's thread would freeze
/// it, so the test runs the runtime on a thread of its own and fails, rather than hangs, if
/// that thread does not finish in time.
#[test]
fn a_held_disk_write_never_stalls_the_runtime() {
    let held = HeldFs::new(FaultFs::new());
    let (done, finished) = std::sync::mpsc::channel();
    let worker = {
        let held = held.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let files = Arc::new(DiskFiles::open(Arc::new(held.clone()), &root()));
                let prefs =
                    PrefsWriter::start(files, ReportLog::new(), Arc::new(ManualClock::new(1)));
                let saving = {
                    let prefs = prefs.clone();
                    tokio::spawn(
                        async move { prefs.set_locale(LocaleId::parse("ar").unwrap()).await },
                    )
                };
                while held.held() == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                let began = std::time::Instant::now();
                tokio::time::sleep(Duration::from_millis(10)).await;
                let timer = began.elapsed();
                assert!(!saving.is_finished(), "the write is still held");
                held.release();
                saving.await.unwrap().unwrap();
                assert_eq!(prefs.app_locale().as_str(), "ar");
                timer
            })
        })
    };
    let watchdog = std::thread::spawn(move || {
        let timer = worker.join();
        let _ = done.send(timer);
    });
    match finished.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(timer)) => assert!(timer < Duration::from_secs(2), "{timer:?}"),
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => {
            held.release();
            panic!("the runtime froze while a disk write was held");
        }
    }
    watchdog.join().unwrap();
}

#[tokio::test]
async fn the_saved_app_language_survives_a_relaunch() {
    let dir = tempfile::tempdir().unwrap();
    let first = TestApp::start_in(dir.path(), |_, deps| deps.files = FilesChoice::OnDisk).await;
    first
        .app
        .request(&SetLocale {
            request: rid(1),
            locale: LocaleId::parse("ar").unwrap(),
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    first.app.quit().await;
    drop(first);

    let file: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("app-data/prefs/app.json")).unwrap())
            .unwrap();
    assert_eq!(file["appLocale"], "ar", "{file}");

    let second = TestApp::start_in(dir.path(), |_, deps| deps.files = FilesChoice::OnDisk).await;
    assert_eq!(hello_value(&second.app).await["prefs"]["appLocale"], "ar");
}

#[tokio::test]
async fn place_and_the_resume_record_survive_a_relaunch() {
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap();
    let place = PlaceUpdate::Book {
        book: "TIT".parse().unwrap(),
        mode: Some(PlaceMode::Check),
        chapter: Some(2),
        verse: PlaceChange::Set(tc_api::ids::RefPart::parse("5").unwrap()),
        tool: Some(tc_api::check::Tool::TranslationWords),
    };
    let first = TestApp::start_in(dir.path(), |_, deps| deps.files = FilesChoice::OnDisk).await;
    first.app.prefs().record_place(project.clone(), place);
    first
        .app
        .prefs()
        .record_last_edit(project.clone(), last_edit());
    wait_for_files(first.app.prefs()).await;
    let kept_place = first.app.prefs().place(&project, "TIT").unwrap();
    first.app.quit().await;
    drop(first);

    let second = TestApp::start_in(dir.path(), |_, deps| deps.files = FilesChoice::OnDisk).await;
    assert_eq!(second.app.prefs().place(&project, "TIT"), Some(kept_place));
    assert_eq!(second.app.prefs().last_edit(), Some(last_edit()));
}

#[tokio::test]
async fn unknown_members_survive_a_rewrite_on_disk() {
    let fs = FaultFs::new();
    fs.put(
        Path::new("/app-data/prefs/app.json"),
        br#"{"futureMember":{"x":[1,2]},"appLocale":42,"schemaVersion":1}"#,
        None,
    );
    let prefs = PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
    assert_eq!(prefs.app_locale(), LocaleId::ENGLISH);
    prefs
        .set_locale(LocaleId::parse("fr").unwrap())
        .await
        .unwrap();
    prefs.record_last_used(tc_api::ids::ProjectRef::parse("titus").unwrap(), WallMs(7));
    wait_for_files(&prefs).await;
    let file: Value =
        serde_json::from_slice(&fs.peek(Path::new("/app-data/prefs/app.json")).unwrap()).unwrap();
    assert_eq!(file["futureMember"], json!({"x": [1, 2]}));
    assert_eq!(file["appLocale"], "fr");
    assert_eq!(file["lastUsed"]["titus"], 7);
    let keys: Vec<&String> = file.as_object().unwrap().keys().collect();
    assert_eq!(
        keys.first().map(|k| k.as_str()),
        Some("futureMember"),
        "the unknown member keeps its place: {keys:?}"
    );
}

#[tokio::test]
async fn an_unreadable_prefs_file_on_disk_is_kept_aside_before_it_is_replaced() {
    let fs = FaultFs::new();
    fs.put(Path::new("/app-data/prefs/app.json"), b"{broken", None);
    let reports = ReportLog::new();
    let prefs = PrefsWriter::start(disk(&fs), reports.clone(), Arc::new(ManualClock::new(1)));
    prefs
        .set_locale(LocaleId::parse("fr").unwrap())
        .await
        .unwrap();
    assert_eq!(
        fs.peek(Path::new("/app-data/prefs/app.json.unreadable"))
            .unwrap(),
        b"{broken"
    );
    assert_eq!(reports.all()[0].op, ReportOp::PrefsWrite);
}

// --- Home summaries (§4.5, C1a) ----------------------------------------------------------

fn stored(n: u8) -> StoredSummary {
    StoredSummary {
        project: tc_journal::ProjectId::from_bytes([n; 16]),
        stamp: SummaryStamp::of(&[format!("pending-{n}")], Some((u64::from(n), 7))),
        summary: json!({"books": n, "unknownToThisLayer": [1, 2]}),
    }
}

#[tokio::test]
async fn a_summary_is_kept_across_a_relaunch_and_never_used_for_another_folder() {
    let fs = FaultFs::new();
    let titus = tc_api::ids::ProjectRef::parse("titus").unwrap();
    let jonah = tc_api::ids::ProjectRef::parse("jonah").unwrap();
    {
        let prefs = PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
        assert_eq!(prefs.summary(&titus), None);
        prefs.record_summary(titus.clone(), stored(1));
        assert_eq!(
            prefs.summary(&titus),
            Some(stored(1)),
            "readers see it at once"
        );
        wait_for_files(&prefs).await;
    }
    // Titus's file planted under Jonah's name: two folder names whose hashes met.
    let bytes = disk(&fs)
        .read(&AppDataPath::summary(&titus))
        .unwrap()
        .unwrap();
    disk(&fs)
        .write(&AppDataPath::summary(&jonah), &bytes)
        .unwrap();
    let prefs = PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
    assert_eq!(prefs.summary(&titus), Some(stored(1)));
    assert_eq!(
        prefs.summary(&jonah),
        None,
        "never another folder's summary"
    );
}

#[tokio::test]
async fn a_reset_removes_every_summary() {
    let fs = FaultFs::new();
    let titus = tc_api::ids::ProjectRef::parse("titus").unwrap();
    let prefs = PrefsWriter::start(disk(&fs), ReportLog::new(), Arc::new(ManualClock::new(1)));
    prefs.record_summary(titus.clone(), stored(2));
    wait_for_files(&prefs).await;
    let path = AppDataPath::summary(&titus);
    assert!(disk(&fs).read(&path).unwrap().is_some());
    prefs.reset().await;
    assert_eq!(disk(&fs).read(&path).unwrap(), None);
    assert_eq!(prefs.summary(&titus), None);
    assert!(
        !files_of(&fs)
            .iter()
            .any(|f| f.starts_with("summaries/") && !temporary(f)),
        "{:?}",
        files_of(&fs)
    );
}

/// C1a's D7: C1a keeps the summaries across launches, and Home's half (Home keeping its
/// summaries with `PrefsWriter::record_summary`, and §8.2's test of the Home row on the store)
/// is built by whichever of B2 and C1a lands second. This fails the first tree whose
/// `parity/app/units.json` lists both until that half is there.
#[test]
fn home_keeps_its_summaries_once_b2_and_c1a_have_landed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let units: Value = serde_json::from_slice(
        &std::fs::read(manifest.join("../../parity/app/units.json")).unwrap(),
    )
    .unwrap();
    let landed = |unit: &str| {
        units["landed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u == unit)
    };
    if !(landed("B2") && landed("C1a")) {
        return;
    }
    let library = rust_sources(&manifest.join("src"), &manifest.join("src/prefs.rs"));
    let tests = rust_sources(&manifest.join("tests"), Path::new(""));
    let mut owed = Vec::new();
    if !library.contains("record_summary(") {
        owed.push("nothing in tc-app's library calls `PrefsWriter::record_summary`, so every launch computes Home's summaries again");
    }
    // Split so that this file does not name the test it looks for.
    if !tests.contains(concat!("fn home_costs_projects", "_not_history(")) {
        owed.push("§8.2's `home_costs_projects_not_history` on the store is missing");
    }
    assert!(
        owed.is_empty(),
        "B2 and C1a have both landed: whichever landed second owes Home's half of C1a's D7: {owed:#?}"
    );
}

/// Every Rust file under `dir` but `except`, one after the other.
fn rust_sources(dir: &Path, except: &Path) -> String {
    let mut all = String::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all += &rust_sources(&path, except);
        } else if path.extension().is_some_and(|e| e == "rs") && path != except {
            all += &std::fs::read_to_string(&path).unwrap();
        }
    }
    all
}
