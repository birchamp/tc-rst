//! First-launch seeding (unit A9): the plan at start, the `Seed` job, the `Startup` events
//! that follow it, and a quit that stops it. The bundled store is the four Open Bible Stories
//! resources of `bundle/en` (about 60 files), built once per test binary with the same code the
//! installer's `bundle_store` example runs. Every test uses real time: the seed does real file
//! I/O on the blocking pool, and a paused clock would jump ahead of it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use tc_api::error::ApiError;
use tc_api::event::{Event, EventFrame, StartupProgress, StartupState};
use tc_api::ids::JobId;
use tc_api::jobs::{Cancel, JobKind, JobOutcome, JobState};
use tc_app::App;
use tc_app::events::{Next, Subscription};
use tc_app::reports::{OperationReport, ReportOp, ReportOutcome};
use tc_resources::{ResourceStore, content_files_opened, content_files_written};

#[allow(dead_code)]
#[path = "../../examples/bundle_store/build.rs"]
pub(super) mod bundle_build;

use super::common::{TestApp, hello_value, rid};

/// The bundled store of these tests, built once per test binary.
fn bundle() -> &'static Path {
    static BUNDLE: OnceLock<PathBuf> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a9-bundle-tc-app");
        let _ = fs::remove_dir_all(&out);
        let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en");
        let obs = ["en_obs", "en_obs-tn", "en_obs-tq", "en_obs-twl"];
        let built =
            bundle_build::build_bundle(&from, &out, &|pin| obs.contains(&pin.repo.name().as_str()))
                .unwrap_or_else(|e| panic!("the test bundle could not be built: {e}"));
        assert_eq!(built.installed.len(), 4, "{built:?}");
        out
    })
}

/// The content files the bundle holds.
fn bundled_files() -> u64 {
    files_in(&ResourceStore::open_read_only(bundle()).unwrap())
}

fn files_in(store: &ResourceStore) -> u64 {
    store
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(|install| install.files().count() as u64)
        .sum()
}

async fn start(dir: &Path, bundle: &Path, pace_ms: Option<u64>) -> TestApp {
    let bundle = bundle.to_owned();
    TestApp::start_in(dir, move |config, _| {
        config.bundle = Some(bundle);
        config.seed_pace = pace_ms.map(Duration::from_millis);
    })
    .await
}

/// Waits, in real time, until the startup state is no longer `Preparing`, and returns it.
async fn until_startup_ends(app: &App) -> StartupState {
    for _ in 0..60_000 {
        let state = app.startup();
        if !matches!(state, StartupState::Preparing(_)) {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the startup never ended: {:?}", app.startup());
}

async fn until_job_ends(app: &App, id: JobId) -> JobState {
    for _ in 0..60_000 {
        if let Some(view) = app.jobs().view(id)
            && tc_app::jobs::JobRegistry::is_ended(&view.state)
        {
            return view.state;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("job {id:?} never ended");
}

async fn next_frame(subscription: &mut Subscription) -> EventFrame {
    match tokio::time::timeout(Duration::from_secs(30), subscription.next()).await {
        Ok(Next::Frame(frame)) => frame,
        other => panic!("no frame: {other:?}"),
    }
}

/// The id of the Seed job, from its first frame.
async fn seed_job_of(subscription: &mut Subscription) -> JobId {
    loop {
        if let Event::Job(view) = next_frame(subscription).await.event {
            assert_eq!(view.kind, JobKind::Seed);
            return view.id;
        }
    }
}

fn seed_reports(app: &App) -> Vec<OperationReport> {
    app.reports()
        .all()
        .into_iter()
        .filter(|report| report.op == ReportOp::Seed)
        .collect()
}

fn startup_frames(app: &App) -> Vec<StartupState> {
    let mut frames = Vec::new();
    let mut subscription = app.events().subscribe(0);
    while let Some(Next::Frame(frame)) = subscription.try_next() {
        if let Event::Startup(state) = frame.event {
            frames.push(state);
        }
    }
    frames
}

fn resources_of(t: &TestApp) -> PathBuf {
    t.app_data.join(tc_app::RESOURCES_FOLDER)
}

/// Every file under `dir` with its bytes and modification time.
fn tree(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let modified = fs::metadata(&path).unwrap().modified().unwrap();
                out.insert(path.clone(), (fs::read(&path).unwrap(), modified));
            }
        }
    }
    out
}

fn counts(app_facts: &serde_json::Map<String, serde_json::Value>, key: &str) -> u64 {
    app_facts[key].as_u64().unwrap()
}

/// A copy of `from`, so a test can damage it.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn without_a_bundle_nothing_is_seeded_and_the_app_is_ready() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;

    assert_eq!(hello_value(&t.app).await["startup"]["type"], "ready");
    assert_eq!(t.app.jobs().live(), 0);
    assert!(startup_frames(&t.app).is_empty(), "no Startup frame");
    assert!(seed_reports(&t.app).is_empty(), "no seed report");
    let resources = resources_of(&t);
    assert!(resources.is_dir(), "the machine's store is opened");
    assert_eq!(fs::read_dir(&resources).unwrap().count(), 0);
}

#[tokio::test]
async fn first_launch_opens_each_bundled_file_once_and_writes_each_once() {
    let dir = tempfile::tempdir().unwrap();
    let n = bundled_files();
    let (opened, written) = (content_files_opened(), content_files_written());

    // 5 ms before each file keeps the seed running for 300 ms or more, so the first answer
    // finds it under way on any disk.
    let t = start(dir.path(), bundle(), Some(5)).await;

    // The plan reads receipts and check records only, and it is done before `start` returns.
    assert_eq!(
        (
            content_files_opened() - opened,
            content_files_written() - written
        ),
        (0, 0)
    );
    assert_eq!(
        hello_value(&t.app).await["startup"],
        serde_json::json!({"type": "preparing", "value": {"done": 0, "total": n}}),
        "hello is exact from the first request"
    );
    assert_eq!(until_startup_ends(&t.app).await, StartupState::Ready);

    // `Ready` means every copy is whole and checked.
    let installs = t.app.resources().installed().unwrap().installs;
    assert_eq!(installs.len(), 4);
    for install in &installs {
        install.verify().unwrap();
    }
    let reports = seed_reports(&t.app);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].outcome, ReportOutcome::Ok);
    let facts = &reports[0].facts;
    assert_eq!(counts(facts, "files"), n);
    assert_eq!(
        counts(facts, "contentFilesOpened"),
        n,
        "each file opened once"
    );
    assert_eq!(
        counts(facts, "contentFilesWritten"),
        n,
        "each file written once"
    );
    assert_eq!(facts["added"].as_array().unwrap().len(), 4);
    assert_eq!(facts["refused"].as_array().unwrap().len(), 0);
    t.app.quit().await;
}

#[tokio::test]
async fn repeat_launch_reads_no_resource_content_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = start(dir.path(), bundle(), None).await;
    assert_eq!(until_startup_ends(&first.app).await, StartupState::Ready);
    first.app.quit().await;
    let resources = resources_of(&first);
    let before = tree(&resources);
    assert!(!before.is_empty());

    let (opened, written) = (content_files_opened(), content_files_written());
    let second = start(dir.path(), bundle(), None).await;
    assert_eq!(
        (
            content_files_opened() - opened,
            content_files_written() - written
        ),
        (0, 0),
        "a repeat launch opens and writes no resource content file"
    );

    assert_eq!(hello_value(&second.app).await["startup"]["type"], "ready");
    assert_eq!(second.app.jobs().live(), 0);
    assert!(second.app.jobs().view(JobId::new(1)).is_none(), "no job");
    assert!(startup_frames(&second.app).is_empty(), "no Startup frame");
    assert!(seed_reports(&second.app).is_empty(), "no seed report");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(tree(&resources), before, "bytes and times stay");
}

#[tokio::test]
async fn startup_frames_follow_the_seed_job_and_end_ready() {
    let dir = tempfile::tempdir().unwrap();
    let n = bundled_files();
    let t = start(dir.path(), bundle(), Some(20)).await;
    let mut subscription = t.app.events().subscribe(0);

    let mut events = Vec::new();
    loop {
        let event = next_frame(&mut subscription).await.event;
        let ready = event == Event::Startup(StartupState::Ready);
        events.push(event);
        if ready {
            break;
        }
    }

    let mut seed_counts = BTreeSet::new();
    let mut last = 0;
    let mut progress_frames = 0;
    for (at, event) in events.iter().enumerate() {
        match event {
            Event::Job(view) => {
                assert_eq!(view.kind, JobKind::Seed);
                if let JobState::Running(progress) = &view.state {
                    seed_counts.insert(progress.done);
                    assert_eq!(progress.total, Some(n));
                }
            }
            Event::Startup(StartupState::Preparing(StartupProgress { done, total })) => {
                assert_eq!(*total, n, "the total never changes");
                assert!(*done >= last, "the count never falls: {events:?}");
                last = *done;
                if at == 0 {
                    assert_eq!(*done, 0, "the first frame is the plan");
                } else {
                    progress_frames += 1;
                    assert!(
                        seed_counts.contains(done),
                        "a Job frame with {done} came first: {seed_counts:?}"
                    );
                }
            }
            Event::Startup(StartupState::Ready) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(matches!(
        events[0],
        Event::Startup(StartupState::Preparing(_))
    ));
    assert!(progress_frames >= 1, "{events:?}");

    // The job's last count comes after `Ready`; it must not show preparing again.
    assert_eq!(
        until_job_ends(&t.app, JobId::new(1)).await,
        JobState::Done(JobOutcome::Finished)
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    while let Some(next) = subscription.try_next() {
        let Next::Frame(frame) = next else {
            panic!("{next:?}")
        };
        assert!(
            !matches!(frame.event, Event::Startup(_)),
            "a Startup frame after Ready: {frame:?}"
        );
    }
    assert_eq!(t.app.startup(), StartupState::Ready);
    t.app.quit().await;
}

#[tokio::test]
async fn quit_while_seeding_stops_the_seed_and_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let n = bundled_files();
    let first = start(dir.path(), bundle(), Some(40)).await;
    let mut subscription = first.app.events().subscribe(0);
    let id = seed_job_of(&mut subscription).await;
    // Wait until one install is whole, so the stop can fall inside a later one.
    let resources = resources_of(&first);
    for _ in 0..60_000 {
        let whole = ResourceStore::open_read_only(&resources)
            .unwrap()
            .installed()
            .unwrap()
            .installs
            .len();
        if whole >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let began = std::time::Instant::now();
    first.app.quit().await;
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(
        first.app.jobs().view(id).unwrap().state,
        JobState::Cancelled
    );
    assert!(matches!(first.app.startup(), StartupState::Failed { .. }));

    // Only whole copies are in the store.
    let kept = ResourceStore::open_read_only(&resources).unwrap();
    let inventory = kept.installed().unwrap();
    assert!(inventory.unreadable.is_empty());
    assert!(
        (1..4).contains(&inventory.installs.len()),
        "{} installs",
        inventory.installs.len()
    );
    for install in &inventory.installs {
        install.verify().unwrap();
    }
    let kept_trees: Vec<_> = inventory
        .installs
        .iter()
        .map(|install| tree(install.dir()))
        .collect();
    let missing = n - files_in(&kept);
    let stale: Vec<_> = fs::read_dir(resources.join(".partial"))
        .map(|entries| entries.map(|e| e.unwrap().file_name()).collect())
        .unwrap_or_default();

    // The next launch clears what the stop left and copies only what is missing.
    let second = start(dir.path(), bundle(), Some(20)).await;
    for name in &stale {
        assert!(
            !resources.join(".partial").join(name).exists(),
            "{name:?} was left by the stop"
        );
    }
    assert_eq!(
        second.app.startup(),
        StartupState::Preparing(StartupProgress {
            done: 0,
            total: missing
        }),
        "the plan counts what is missing"
    );
    assert_eq!(until_startup_ends(&second.app).await, StartupState::Ready);
    let reports = seed_reports(&second.app);
    assert_eq!(reports.len(), 1);
    assert_eq!(counts(&reports[0].facts, "contentFilesOpened"), missing);
    assert_eq!(counts(&reports[0].facts, "contentFilesWritten"), missing);
    let installs = second.app.resources().installed().unwrap().installs;
    assert_eq!(installs.len(), 4);
    for install in &installs {
        install.verify().unwrap();
    }
    for tree_before in kept_trees {
        for (path, was) in tree_before {
            assert_eq!(
                tree(path.parent().unwrap())[&path],
                was,
                "{}",
                path.display()
            );
        }
    }
    second.app.quit().await;
}

#[tokio::test]
async fn cancelling_the_seed_job_leaves_the_startup_failed_until_the_next_launch() {
    let dir = tempfile::tempdir().unwrap();
    let first = start(dir.path(), bundle(), Some(20)).await;
    let mut subscription = first.app.events().subscribe(0);
    let id = seed_job_of(&mut subscription).await;

    first
        .app
        .request(&Cancel {
            request: rid(1),
            job: id,
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();

    assert_eq!(until_job_ends(&first.app, id).await, JobState::Cancelled);
    let StartupState::Failed { reason } = first.app.startup() else {
        panic!("{:?}", first.app.startup())
    };
    assert!(reason.contains("continues at the next start"), "{reason}");
    assert_eq!(hello_value(&first.app).await["startup"]["type"], "failed");
    first.app.quit().await;

    let second = start(dir.path(), bundle(), None).await;
    assert_eq!(until_startup_ends(&second.app).await, StartupState::Ready);
    let installs = second.app.resources().installed().unwrap().installs;
    assert_eq!(installs.len(), 4);
    second.app.quit().await;
}

#[tokio::test]
async fn a_damaged_bundled_install_fails_the_startup_after_copying_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let damaged = dir.path().join("damaged-bundle");
    copy_tree(bundle(), &damaged);
    let inventory = ResourceStore::open_read_only(&damaged)
        .unwrap()
        .installed()
        .unwrap();
    let broken = &inventory.installs[1];
    let name = broken.dcs_pin().unwrap().repo.name().as_str().to_owned();
    let file = broken
        .dir()
        .join("content")
        .join(broken.files().next().unwrap());
    let mut bytes = fs::read(&file).unwrap();
    bytes[0] ^= 1;
    fs::write(&file, bytes).unwrap();

    let t = start(dir.path(), &damaged, None).await;
    let StartupState::Failed { reason } = until_startup_ends(&t.app).await else {
        panic!("a damaged bundled install did not fail the start")
    };
    assert!(reason.contains(&name), "{reason}");

    let installs = t.app.resources().installed().unwrap().installs;
    assert_eq!(installs.len(), 3, "the other installs are copied");
    for install in &installs {
        install.verify().unwrap();
    }
    let reports = seed_reports(&t.app);
    assert_eq!(reports.len(), 1);
    assert!(matches!(&reports[0].outcome, ReportOutcome::Failed { reason: r } if *r == reason));
    assert_eq!(reports[0].facts["added"].as_array().unwrap().len(), 3);
    let refused = reports[0].facts["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 1);
    assert!(refused[0]["install"].as_str().unwrap().contains(&name));
    let JobState::Failed(ApiError::Internal { report }) =
        until_job_ends(&t.app, JobId::new(1)).await
    else {
        panic!("the job did not fail with a report")
    };
    assert_eq!(t.app.reports().get(report).unwrap(), reports[0]);
    t.app.quit().await;
}

#[tokio::test]
async fn a_copy_that_cannot_be_trusted_fails_the_startup_without_a_job() {
    let dir = tempfile::tempdir().unwrap();
    let first = start(dir.path(), bundle(), None).await;
    assert_eq!(until_startup_ends(&first.app).await, StartupState::Ready);
    first.app.quit().await;
    let installs = first.app.resources().installed().unwrap().installs;
    let receipt = installs[0].dir().join("receipt.json");
    fs::write(&receipt, b"{}").unwrap();

    let second = start(dir.path(), bundle(), None).await;

    // At once: no job runs, and the ring holds the frame a page that subscribes sees.
    let StartupState::Failed { reason } = second.app.startup() else {
        panic!("{:?}", second.app.startup())
    };
    assert!(reason.contains("receipt"), "{reason}");
    assert_eq!(hello_value(&second.app).await["startup"]["type"], "failed");
    assert_eq!(second.app.jobs().live(), 0);
    assert!(second.app.jobs().view(JobId::new(1)).is_none());
    assert_eq!(
        startup_frames(&second.app),
        [StartupState::Failed {
            reason: reason.clone()
        }]
    );
    let reports = seed_reports(&second.app);
    assert_eq!(reports.len(), 1);
    assert!(matches!(&reports[0].outcome, ReportOutcome::Failed { reason: r } if *r == reason));
    assert_eq!(
        fs::read(&receipt).unwrap(),
        b"{}",
        "the copy is left as it was"
    );
    second.app.quit().await;
}

/// A folder made read-only, which holds its entries in place; its permissions come back when it
/// is dropped, so the temporary folder can be removed.
#[cfg(unix)]
struct Held(PathBuf);

#[cfg(unix)]
impl Held {
    fn new(dir: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
        Self(dir.to_owned())
    }
}

#[cfg(unix)]
impl Drop for Held {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[cfg(unix)]
fn resources_reports(app: &App) -> Vec<OperationReport> {
    app.reports()
        .all()
        .into_iter()
        .filter(|report| report.op == ReportOp::Resources)
        .collect()
}

#[cfg(unix)]
#[tokio::test]
async fn a_leftover_that_cannot_be_removed_stops_neither_the_start_nor_the_seed() {
    let dir = tempfile::tempdir().unwrap();
    // What a crash left in `resources/.partial/`, with a folder that cannot be emptied (on
    // Windows a scanner holding one of the files does the same).
    let partial = dir.path().join("app-data/resources/.partial");
    let old = partial.join("1-2-3");
    fs::create_dir_all(old.join("content")).unwrap();
    fs::write(old.join("content/a.md"), b"half").unwrap();
    let held = Held::new(&old);
    if fs::write(old.join("probe"), b"").is_ok() {
        eprintln!("the permission is not enforced here (running as root): nothing to prove");
        return;
    }

    // Without a bundle: the app starts, ready, and says what it could not remove.
    let plain = TestApp::start_in(dir.path(), |_, _| {}).await;
    assert_eq!(hello_value(&plain.app).await["startup"]["type"], "ready");
    let reports = resources_reports(&plain.app);
    assert_eq!(reports.len(), 1, "{reports:?}");
    let ReportOutcome::Failed { reason } = &reports[0].outcome else {
        panic!("{:?}", reports[0].outcome)
    };
    assert!(reason.contains(".partial"), "{reason}");
    assert!(old.is_dir(), "the leftover stays until it can go");
    plain.app.quit().await;

    // With a bundle: the first launch is not stopped either, and lands whole installs.
    let seeding = start(dir.path(), bundle(), None).await;
    assert_eq!(until_startup_ends(&seeding.app).await, StartupState::Ready);
    let installs = seeding.app.resources().installed().unwrap().installs;
    assert_eq!(installs.len(), 4);
    for install in &installs {
        install.verify().unwrap();
    }
    assert_eq!(resources_reports(&seeding.app).len(), 1);
    seeding.app.quit().await;

    // Once it can go, the next start removes it and says nothing.
    drop(held);
    let last = TestApp::start_in(dir.path(), |_, _| {}).await;
    assert!(!partial.exists());
    assert!(resources_reports(&last.app).is_empty());
    last.app.quit().await;
}

/// A copy of the test bundle in `to`, without its `nth` install.
fn bundle_without_install(to: &Path, nth: usize) -> String {
    copy_tree(bundle(), to);
    let inventory = ResourceStore::open_read_only(to)
        .unwrap()
        .installed()
        .unwrap();
    let lost = &inventory.installs[nth];
    let name = lost.dcs_pin().unwrap().to_string();
    fs::remove_dir_all(lost.dir()).unwrap();
    assert_eq!(
        ResourceStore::open_read_only(to)
            .unwrap()
            .installed()
            .unwrap()
            .installs
            .len(),
        3
    );
    name
}

#[tokio::test]
async fn a_bundle_that_lost_an_install_fails_the_startup_at_once_and_copies_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let damaged = dir.path().join("damaged-bundle");
    let name = bundle_without_install(&damaged, 1);

    let t = start(dir.path(), &damaged, None).await;

    let StartupState::Failed { reason } = t.app.startup() else {
        panic!(
            "a bundle with a lost install did not fail the start: {:?}",
            t.app.startup()
        )
    };
    assert!(reason.contains("incomplete"), "{reason}");
    assert!(reason.contains(&name), "{reason} does not name {name}");
    assert_eq!(hello_value(&t.app).await["startup"]["type"], "failed");
    assert_eq!(
        t.app.jobs().live(),
        0,
        "no copy is begun from a damaged bundle"
    );
    assert!(t.app.jobs().view(JobId::new(1)).is_none());
    assert_eq!(
        startup_frames(&t.app),
        [StartupState::Failed {
            reason: reason.clone()
        }]
    );
    assert!(t.app.resources().installed().unwrap().installs.is_empty());
    let reports = seed_reports(&t.app);
    assert_eq!(reports.len(), 1);
    assert!(matches!(&reports[0].outcome, ReportOutcome::Failed { reason: r } if *r == reason));
    assert_eq!(reports[0].facts["missing"], serde_json::json!([name]));
    t.app.quit().await;
}

#[tokio::test]
async fn a_folder_that_is_no_bundle_fails_the_startup() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty-bundle");
    fs::create_dir_all(&empty).unwrap();
    let unlisted = dir.path().join("unlisted-bundle");
    copy_tree(bundle(), &unlisted);
    fs::remove_file(unlisted.join(tc_app::bundle::MANIFEST)).unwrap();

    for folder in [&empty, &unlisted] {
        let t = start(dir.path(), folder, None).await;
        let StartupState::Failed { reason } = t.app.startup() else {
            panic!(
                "{} did not fail the start: {:?}",
                folder.display(),
                t.app.startup()
            )
        };
        assert!(reason.contains("no manifest"), "{reason}");
        assert_eq!(t.app.jobs().live(), 0);
        assert!(t.app.resources().installed().unwrap().installs.is_empty());
        t.app.quit().await;
    }
}

#[tokio::test]
async fn an_install_the_bundle_lost_is_no_loss_when_this_computer_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let first = start(dir.path(), bundle(), None).await;
    assert_eq!(until_startup_ends(&first.app).await, StartupState::Ready);
    first.app.quit().await;
    let damaged = dir.path().join("damaged-bundle");
    bundle_without_install(&damaged, 2);

    let second = start(dir.path(), &damaged, None).await;

    assert_eq!(second.app.startup(), StartupState::Ready);
    assert_eq!(second.app.jobs().live(), 0);
    assert!(seed_reports(&second.app).is_empty());
    assert_eq!(
        second.app.resources().installed().unwrap().installs.len(),
        4
    );
    second.app.quit().await;
}
