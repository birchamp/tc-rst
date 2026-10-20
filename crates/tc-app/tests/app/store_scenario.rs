//! A parity scenario on the store (unit C1a, APP-DESIGN §4.2): its projects written to disk
//! through tc-store's staging and placing with the seed's own packs, put back by `test/reset`,
//! teammates sealed by their own devices, and J15 on real `FaultFs` delays. Each case runs on
//! tc-store's in-memory fault-injecting file system.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tc_api::test::{Hold, HoldPoint, Release, Reset, SaveVerse};
use tc_api::write::Stored;
use tc_app::StoreChoice;
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::{ScenarioError, ScenarioReport};
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::backend::seed::fixture_project_id;
use tc_app::backend::store::StoreBackend;
use tc_app::backend::{ProjectHandle, ProjectStore};
use tc_app::config::ProjectsFolder;
use tc_app::wire::token::shown;
use tc_store::fault::FaultFs;
use tc_store::{FakePlatform, Hooks, ManualClock, Point, StoreConfig};

use super::common::{
    Parts, TestApp, folder, is_busy, open_progress, post, rid, tit, until_held_count, until_quiet,
};
use super::scenario::scenario;
use super::state_seed::bible_declared;
use crate::store_world::{
    APP, PROJECTS, START_MS, assert_valid_burrito, backend_over, files, pending_files, progress,
    words_of,
};

const SAMPLE: &str = "sample_burrito";
const OBS: &str = "sample_burrito_obs";

/// The scenario `name` written into an empty projects folder on `fs`.
fn scenario_on(fs: &FaultFs, name: &str) -> (ScenarioStore, ScenarioReport) {
    let backend = backend_over(fs, APP, FakePlatform::default(), Arc::default());
    ScenarioStore::write(backend, scenario(name)).unwrap()
}

fn sample_path() -> PathBuf {
    Path::new(PROJECTS).join(SAMPLE)
}

/// Opens the sample through the seam and waits for the open's background work.
fn open_sample(store: &ScenarioStore) -> Arc<dyn ProjectHandle> {
    let handle = store.open(&folder(SAMPLE), progress()).unwrap();
    handle.wait_idle();
    handle
}

#[test]
fn every_scenario_writes_both_its_projects() {
    for name in ["sample", "en-pins", "shared"] {
        let fs = FaultFs::new();
        let (store, report) = scenario_on(&fs, name);
        let loaded: Vec<String> = report.loaded.iter().map(|(f, _)| f.to_string()).collect();
        assert_eq!(loaded, [SAMPLE, OBS], "{name}");
        assert_eq!(report.loaded[0].1.declared, bible_declared(), "{name}");
        assert!(report.refused.is_empty(), "{name}: {:?}", report.refused);
        let listing = store.list().unwrap();
        let listed: Vec<String> = listing
            .projects
            .iter()
            .map(|e| e.folder.to_string())
            .collect();
        assert_eq!(listed, [SAMPLE, OBS], "{name}");
        assert!(
            listing.unusable.is_empty(),
            "{name}: {:?}",
            listing.unusable
        );
        assert_eq!(listing.projects[0].id, fixture_project_id(SAMPLE), "{name}");
        assert_eq!(listing.projects[1].id, fixture_project_id(OBS), "{name}");
        // Nothing but the projects: no staging folder.
        let entries: Vec<String> = fs
            .files_under(Path::new(PROJECTS))
            .keys()
            .filter_map(|p| p.strip_prefix(PROJECTS).ok()?.iter().next())
            .map(|name| name.to_string_lossy().into_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(entries, [SAMPLE, OBS], "{name}");
    }
}

/// The OBS project opens through the store and is a valid Scripture Burrito, with its fifty
/// stories as the files the seed was made from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_obs_project_opens_on_the_store_and_is_a_valid_burrito() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let handle = store.open(&folder(OBS), progress()).unwrap();
    handle.wait_idle();
    assert_eq!(handle.facts().kind, tc_journal::fold::ProjectKind::Stories);
    handle.close().await.unwrap();
    let path = Path::new(PROJECTS).join(OBS);
    let story = fs.peek(&path.join("ingredients/content/01.md")).unwrap();
    assert!(
        String::from_utf8(story)
            .unwrap()
            .starts_with("# 1. La Creación\n")
    );
    assert_valid_burrito(&fs, &path);
}

/// The same seed: every record the memory backend folds from the scenario has the same heads
/// and the same projected value on the store, and the store holds no other record.
#[test]
fn the_store_scenario_holds_exactly_the_seed() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let (memory, _) = MemoryBackend::from_scenario(scenario("sample")).unwrap();
    let want = memory.fold(&folder(SAMPLE)).unwrap();
    let handle = open_sample(&store);
    let snapshot = handle.snapshot();
    let got = snapshot.fold();
    assert_eq!(got.record_keys(), want.record_keys());
    for key in want.record_keys() {
        assert_eq!(got.heads(&key), want.heads(&key), "{key:?}");
        assert_eq!(got.projected(&key), want.projected(&key), "{key:?}");
    }
    assert_eq!(handle.facts().id, fixture_project_id(SAMPLE));
    assert_eq!(got.conflict_count(), 0);
}

/// The folder holds tC4's own bytes for every book and the versification, and it is a valid
/// Scripture Burrito.
#[test]
fn the_scenario_project_on_disk_holds_the_fixture_bytes() {
    let fs = FaultFs::new();
    let _ = scenario_on(&fs, "sample");
    let on_disk = files(&fs, &sample_path());
    let project = scenario("sample")
        .projects()
        .unwrap()
        .into_iter()
        .find(|p| p.folder.as_str() == SAMPLE)
        .unwrap();
    let mut compared = 0;
    for (path, bytes) in &project.files {
        let derived = path.starts_with("ingredients/")
            && (path.ends_with(".usfm") || path.ends_with("vrs.json"));
        if derived {
            assert_eq!(on_disk.get(path), Some(bytes), "{path}");
            compared += 1;
        }
    }
    assert!(
        compared >= 3,
        "the sample has Titus, Jonah and vrs.json: {compared}"
    );
    assert_valid_burrito(&fs, &sample_path());
    let project_json: serde_json::Value =
        serde_json::from_slice(&on_disk["ingredients/tc/project.json"]).unwrap();
    assert_eq!(
        project_json,
        serde_json::json!({
            "schemaVersion": 1,
            "projectId": fixture_project_id(SAMPLE).to_string(),
            "kind": "bible",
            "created": "2026-07-02T12:00:00.000Z",
        })
    );
}

#[test]
fn a_scenario_needs_an_empty_projects_folder() {
    let fs = FaultFs::new();
    let someones = Path::new(PROJECTS).join("someones_work/metadata.json");
    fs.put(&someones, b"{}", None);
    let before = fs.files_under(Path::new(PROJECTS));
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let refused = ScenarioStore::write(backend, scenario("sample")).unwrap_err();
    assert_eq!(
        refused,
        ScenarioError::ProjectsFolderNotEmpty {
            path: PROJECTS.to_owned()
        }
    );
    assert_eq!(fs.files_under(Path::new(PROJECTS)), before);
}

/// tC4 #746: the start cleanup removes a staging folder a crash left, never one this writer
/// is filling. The writer's staging folder is asked for only after the cleanup is over.
#[test]
fn a_scenario_written_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes() {
    let fs = FaultFs::new();
    let left = Path::new(PROJECTS).join(".staging-import-00112233445566ff");
    fs.put(&left.join("metadata.json"), b"{}", None);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let backend = StoreBackend::start(
        StoreConfig {
            app_data: PathBuf::from(APP),
            projects: PathBuf::from(PROJECTS),
            vfs: Arc::new(fs.clone()),
            platform: Arc::new(FakePlatform::default()),
            clock: Arc::new(ManualClock::at(START_MS)),
            threads: 2,
        },
        hooks.clone(),
    )
    .unwrap();
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let writer = std::thread::spawn(move || ScenarioStore::write(backend, scenario("sample")));
    std::thread::sleep(Duration::from_millis(50));
    assert!(!writer.is_finished(), "the writer waits for the cleanup");
    hooks.release(Point::BeforeStagingRemoval);
    let (store, _) = writer.join().unwrap().unwrap();
    assert!(
        fs.files_under(&left).is_empty(),
        "the crash's staging folder is gone"
    );
    let removed = store.backend().removed_at_start();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].kind, Some(tc_store::StagingKind::Import));
    assert_valid_burrito(&fs, &sample_path());
}

/// An app over the sample scenario on `fs`.
async fn app_over(dir: &Path, store: Arc<ScenarioStore>) -> TestApp {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    TestApp::start_with(dir, &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(store);
    })
    .await
}

async fn save_through_the_app(t: &TestApp, words: &str) {
    let open = t
        .app
        .sessions()
        .open_screen(&folder(SAMPLE), open_progress())
        .await
        .unwrap();
    let session = t.app.sessions().reader(&open.at).unwrap();
    let minted = session.read().mint(&tit(1, 1), None).unwrap();
    let ack = post(
        &t.app,
        &SaveVerse {
            at: open.at.clone(),
            request: rid(1),
            shown: shown(&minted).unwrap(),
            words: words.to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reset_puts_back_a_changed_project_and_removes_others() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let store = Arc::new(store);
    let seed_words = words_of(&open_sample(&store).snapshot(), &tit(1, 1));
    assert!(seed_words.is_some());
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), store.clone()).await;
    save_through_the_app(&t, "changed before the reset").await;
    let journey = Path::new(PROJECTS).join("made_by_a_journey");
    fs.put(&journey.join("metadata.json"), b"{}", None);
    fs.put(&Path::new(PROJECTS).join("a_file.txt"), b"x", None);

    post(&t.app, &Reset { request: rid(2) }).await.unwrap();

    assert!(fs.files_under(&journey).is_empty());
    assert_eq!(fs.peek(&Path::new(PROJECTS).join("a_file.txt")), None);
    let listed: Vec<String> = store
        .list()
        .unwrap()
        .projects
        .iter()
        .map(|e| e.folder.to_string())
        .collect();
    assert_eq!(listed, [SAMPLE, OBS]);
    let open = t
        .app
        .sessions()
        .open_screen(&folder(SAMPLE), open_progress())
        .await
        .unwrap();
    let session = t.app.sessions().reader(&open.at).unwrap();
    assert_eq!(
        words_of(&session.handle().snapshot(), &tit(1, 1)),
        seed_words
    );
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.quit().await;
    assert_valid_burrito(&fs, &sample_path());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reset_rewrites_no_project_nobody_opened() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let device = sample_path().join(".tc/device.json");
    let before = fs.peek(&device).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), Arc::new(store)).await;
    post(&t.app, &Reset { request: rid(2) }).await.unwrap();
    assert_eq!(fs.peek(&device).unwrap(), before);
    t.app.quit().await;
}

/// The `shared` scenario's Bible project is shared on Door43 on the store as in tC4's mock, so
/// its Home card offers "Sync with Door43" (B10's screens); the other scenarios' are not.
#[test]
fn a_shared_scenarios_project_keeps_its_door43_remote_on_the_store() {
    for name in ["sample", "en-pins", "shared"] {
        let fs = FaultFs::new();
        let (store, _) = scenario_on(&fs, name);
        let remote = store.peek(&folder(SAMPLE)).unwrap().remote;
        let shared = (name == "shared").then(|| tc_app::backend::Door43Remote {
            host: "git.door43.org".to_owned(),
            owner: "facilitator-zq".to_owned(),
            repo: SAMPLE.to_owned(),
        });
        assert_eq!(remote, shared, "{name}");
        assert_valid_burrito(&fs, &sample_path());
    }
}

#[test]
fn teammates_written_into_a_closed_project_stand_as_a_conflict() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "shared");
    let key = tit(1, 1);
    let ids = store
        .teammates_edit_verse(&folder(SAMPLE), &key, &["Paul, a servant", "Paul, a slave"])
        .unwrap();
    assert_eq!(ids.len(), 2);
    let handle = open_sample(&store);
    assert!(matches!(
        handle.facts().mode,
        tc_api::event::SessionMode::Writable
    ));
    let snapshot = handle.snapshot();
    let mut heads = snapshot.fold().heads(&key);
    heads.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(heads, want);
    let devices: Vec<_> = ids
        .iter()
        .map(|id| snapshot.fold().change(id).unwrap().device)
        .collect();
    use tc_app::backend::memory::teammates::device;
    assert_eq!(devices, [device(1), device(2)]);
    drop(snapshot);
    let packs: Vec<String> = files(&fs, &sample_path())
        .into_keys()
        .filter(|p| p.starts_with("ingredients/tc/changes/") && !p.contains(".~"))
        .collect();
    for n in [1u8, 2] {
        let sealer = device(n).to_string();
        assert!(
            packs.iter().any(|p| p.contains(&sealer)),
            "teammate {n} sealed a pack of their own: {packs:?}"
        );
    }
    drop(handle);
    assert_valid_burrito(&fs, &sample_path());
}

#[test]
fn teammates_are_refused_while_a_session_holds_the_project() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "shared");
    let handle = open_sample(&store);
    let before = files(&fs, &sample_path());
    let refused = store
        .teammates_edit_verse(&folder(SAMPLE), &tit(1, 1), &["held"])
        .unwrap_err();
    assert!(matches!(refused, ScenarioError::Open { .. }), "{refused:?}");
    let after = files(&fs, &sample_path());
    let packs = |files: &std::collections::BTreeMap<String, Vec<u8>>| {
        files
            .keys()
            .filter(|p| p.starts_with("ingredients/tc/changes/"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(packs(&after), packs(&before));
    drop(handle);
}

/// A crash at any write point of a scenario project leaves the whole project or nothing in
/// the projects folder once the next start's cleanup has run.
#[test]
fn a_crash_while_a_scenario_project_is_written_leaves_no_half_project() {
    // The dry run: how many write points the scenario's write has, and what it holds.
    let dry = FaultFs::new();
    let backend = backend_over(&dry, APP, FakePlatform::default(), Arc::default());
    let start = dry.ops();
    let (written, _) = ScenarioStore::write(backend, scenario("sample")).unwrap();
    let points = dry.ops() - start;
    assert!(points > 10, "{points} write points");
    let records_of = |name: &str| {
        let handle = written.open(&folder(name), progress()).unwrap();
        handle.wait_idle();
        handle.snapshot().fold().record_keys().len()
    };
    let wanted = [(SAMPLE, records_of(SAMPLE)), (OBS, records_of(OBS))];
    // Every write point, a stopped write torn at every other one: the seed runs again for
    // each, so both forms at every point would double a long test for little.
    for n in 0..points {
        let torn = n % 2 == 1;
        let fs = FaultFs::new();
        let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
        fs.kill_before(n, torn);
        let _ = ScenarioStore::write(backend, scenario("sample"));
        assert!(fs.is_dead(), "point {n} of {points} was reached");
        let next = fs.crash(false);
        // The next start removes a staging folder the crash left (N1).
        let after = backend_over(&next, APP, FakePlatform::default(), Arc::default());
        let names: std::collections::BTreeSet<String> = next
            .files_under(Path::new(PROJECTS))
            .keys()
            .filter_map(|p| p.strip_prefix(PROJECTS).ok()?.iter().next())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|name| name == SAMPLE || name == OBS),
            "point {n}: only the projects may be left: {names:?}"
        );
        for (name, records) in wanted {
            if !names.contains(name) {
                continue;
            }
            // Placed: the whole project is there, and opens.
            let path = Path::new(PROJECTS).join(name);
            let handle = after
                .store()
                .open(&path, &tc_store::OpenOptions::default())
                .unwrap();
            assert_eq!(handle.facts().id, fixture_project_id(name), "point {n}");
            assert_eq!(
                handle.snapshot().fold().record_keys().len(),
                records,
                "point {n}: {name}"
            );
            handle.wait_idle();
            handle.close().unwrap();
            assert_valid_burrito(&next, &path);
        }
    }
}

// --- the summary stamp (§4.5, D8) ----------------------------------------------------------

/// The stamp is the same while nothing changes; a save (a pending file) changes it, and a
/// checkpoint (which seals it and rewrites the manifest) changes it again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_summary_stamp_follows_saves_and_seals() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let sample = folder(SAMPLE);
    let first = store.backend().summary_stamp(&sample).unwrap();
    assert_eq!(store.backend().summary_stamp(&sample).unwrap(), first);
    let handle = open_sample(&store);
    crate::store_world::save(handle.as_ref(), &tit(1, 1), "a save not sealed yet")
        .await
        .unwrap();
    let saved = store.backend().summary_stamp(&sample).unwrap();
    assert_ne!(saved, first);
    handle
        .checkpoint(tc_app::backend::CheckpointReason::Other("test".into()))
        .await;
    handle.wait_idle();
    let sealed = store.backend().summary_stamp(&sample).unwrap();
    assert_ne!(sealed, saved);
    assert_ne!(sealed, first);
    assert_eq!(
        tc_app::backend::SummaryStamp::parse_hex(&sealed.to_hex()),
        Some(sealed)
    );
}

/// The stamp reads no file: one listing of `.tc/pending/` and one stat of the manifest, the same
/// with a longer history (two teammates' packs more).
#[test]
fn a_summary_stamp_lists_one_folder_and_stats_one_file_at_any_history() {
    let fs = FaultFs::new();
    let counters = Arc::new(tc_store::counters::Counters::default());
    let counted = tc_store::counters::CountingVfs::new(Arc::new(fs.clone()), counters.clone());
    let backend = StoreBackend::start(
        StoreConfig {
            app_data: PathBuf::from(APP),
            projects: PathBuf::from(PROJECTS),
            vfs: Arc::new(counted),
            platform: Arc::new(FakePlatform::default()),
            clock: Arc::new(ManualClock::at(START_MS)),
            threads: 2,
        },
        Arc::default(),
    )
    .unwrap();
    let (store, _) = ScenarioStore::write(backend, scenario("shared")).unwrap();
    let cost = || {
        let before = counters.read();
        store.backend().summary_stamp(&folder(SAMPLE)).unwrap();
        counters.read().since(&before)
    };
    let short = cost();
    assert_eq!((short.listings, short.stats), (1, 1), "{short:?}");
    assert_eq!((short.files_opened, short.bytes_read), (0, 0), "{short:?}");
    store
        .teammates_edit_verse(&folder(SAMPLE), &tit(1, 1), &["one", "two"])
        .unwrap();
    assert_eq!(cost(), short);
}

// --- tC4 #766: the creation pins at the first open ----------------------------------------

/// tC4 #766 (TC4-DELTA-2026-10-19 §3.12.4): an existing project whose pin groups are all empty
/// gets the creation pins at its first open, in one batch of a System cause, never `Edit`, so
/// History does not say the translator changed the resources; a second open writes nothing,
/// and a project with pins keeps its `resources.json` byte for byte. A declared difference
/// from tC4 4.0 at d310bf3, where such a project opens with "No resources pinned".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_writes_creation_pins_only_when_none_exist() {
    use tc_journal::{Cause, PinGroup, RecordKey};
    let groups = [
        PinGroup::Primary,
        PinGroup::Fallback,
        PinGroup::OriginalLanguage,
        PinGroup::Lexicon,
        PinGroup::ExtraScripture,
    ];
    let world = crate::store_world::store_world();
    let made = world
        .backend
        .store()
        .create(&tc_store::NewProject {
            name: "no_pins".to_owned(),
            kind: tc_journal::fold::ProjectKind::Bible,
            records: crate::store_world::sample_records(&world.sources, None),
            books: Vec::new(),
        })
        .unwrap();
    let opened = world.open(&folder("no_pins"));
    let fold = opened.snapshot().fold().clone();
    let mut batches = std::collections::BTreeSet::new();
    for group in groups {
        let key = RecordKey::Pins(group);
        assert!(
            fold.projected(&key).is_some(),
            "{group:?} is pinned at the first open"
        );
        let head = fold.heads(&key)[0];
        let body = fold.body(&head).unwrap();
        assert!(
            matches!(body.cause, Cause::System(_)),
            "{group:?}: {:?}",
            body.cause
        );
        batches.insert(body.batch.id);
    }
    assert_eq!(batches.len(), 1, "one batch");
    assert!(
        world
            .fs
            .peek(&made.join("ingredients/checking/resources.json"))
            .is_some()
    );
    let changes = fold.len();
    opened.close().await.unwrap();
    let again = world.open(&folder("no_pins"));
    assert_eq!(
        again.snapshot().fold().len(),
        changes,
        "the second open writes nothing"
    );
    again.close().await.unwrap();
    // A project with pins keeps them byte for byte.
    let sample = Path::new(PROJECTS).join(SAMPLE);
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let before = fs
        .peek(&sample.join("ingredients/checking/resources.json"))
        .unwrap();
    open_sample(&store).close().await.unwrap();
    assert_eq!(
        fs.peek(&sample.join("ingredients/checking/resources.json"))
            .unwrap(),
        before
    );
}

// --- test/hold and test/quiet on the store ------------------------------------------------

/// `test/hold save` holds tc-store's durability point: the save's reply waits, `test/quiet`
/// says busy and nothing is durable yet; the release lets it finish.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_store_save_holds_the_reply_and_quiet_says_busy() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), Arc::new(store)).await;
    let open = t
        .app
        .sessions()
        .open_screen(&folder(SAMPLE), open_progress())
        .await
        .unwrap();
    until_quiet(&t.app).await;
    let pending_before = pending_files(&fs, &sample_path());
    post(
        &t.app,
        &Hold {
            request: rid(3),
            at: HoldPoint::Save,
        },
    )
    .await
    .unwrap();
    let session = t.app.sessions().reader(&open.at).unwrap();
    let minted = session.read().mint(&tit(1, 1), None).unwrap();
    let save = SaveVerse {
        at: open.at.clone(),
        request: rid(4),
        shown: shown(&minted).unwrap(),
        words: "held at the durability point".to_owned(),
    };
    let app = t.app.clone();
    let saving = tokio::spawn(async move { post(&app, &save).await });
    until_held_count(&t.app, 1).await;
    assert!(is_busy(&t.app).await);
    assert!(!saving.is_finished());
    assert_eq!(
        pending_files(&fs, &sample_path()),
        pending_before,
        "nothing durable yet"
    );
    post(
        &t.app,
        &Release {
            request: rid(5),
            at: HoldPoint::Save,
        },
    )
    .await
    .unwrap();
    let ack = saving.await.unwrap().unwrap();
    assert_eq!(ack.stored, Stored::Written);
    until_quiet(&t.app).await;
    assert_eq!(t.app.holding(), 0);
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.quit().await;
}

/// `test/reset` releases the store's holds with the app's: a save after it is not held.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reset_releases_the_store_holds() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), Arc::new(store)).await;
    post(
        &t.app,
        &Hold {
            request: rid(3),
            at: HoldPoint::Save,
        },
    )
    .await
    .unwrap();
    post(&t.app, &Reset { request: rid(4) }).await.unwrap();
    let saved = tokio::time::timeout(
        Duration::from_secs(20),
        save_through_the_app(&t, "not held after the reset"),
    )
    .await;
    assert!(saved.is_ok(), "the save was held after the reset");
    assert_eq!(t.app.holding(), 0);
    t.app.quit().await;
}

// --- J15 on real FaultFs delays ----------------------------------------------------------

/// The open's progress, sampled every 2 ms on a thread of its own from the moment the open
/// is asked for: (time since then, report).
fn sample_progress(
    sink: &tc_app::jobs::ProgressSink,
    began: Instant,
    done: Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<Vec<(Duration, tc_api::jobs::Progress)>> {
    let sink = sink.clone();
    std::thread::spawn(move || {
        let mut samples = Vec::new();
        loop {
            let finished = done.load(std::sync::atomic::Ordering::SeqCst);
            samples.push((began.elapsed(), sink.snapshot()));
            if finished {
                return samples;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })
}

fn phase_index(phase: &tc_api::jobs::Phase) -> usize {
    use tc_api::jobs::{OpenPhase, Phase};
    match phase {
        Phase::Open(OpenPhase::FinishingWork) => 0,
        Phase::Open(OpenPhase::ReadingChanges) => 1,
        Phase::Open(OpenPhase::Folding) => 2,
        Phase::Open(OpenPhase::CheckingFiles) => 3,
        Phase::Open(OpenPhase::WritingFiles) => 4,
        other => panic!("not an open phase: {other:?}"),
    }
}

/// J15 (STORAGE:1883-1891): on a slow disk the scenario project's open reports from 300 ms,
/// with a total, phases moving forward and counts that never fall.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j15_a_slow_open_reports_from_300_ms_and_counts_up() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), Arc::new(store)).await;
    fs.set_read_delay(Some(Duration::from_millis(60)));
    let sink = open_progress();
    let initial = sink.snapshot();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let began = Instant::now();
    let sampler = sample_progress(&sink, began, done.clone());
    let open = t
        .app
        .sessions()
        .open_screen(&folder(SAMPLE), sink.clone())
        .await
        .unwrap();
    let took = began.elapsed();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let samples = sampler.join().unwrap();
    fs.set_read_delay(None);
    assert!(took > Duration::from_millis(600), "the open took {took:?}");
    let first = samples
        .iter()
        .position(|(_, p)| *p != initial)
        .expect("the open reported");
    let at = samples[first].0;
    // tc-store's own test allows 300-400 ms (tests/store/progress.rs); 20 ms more here for
    // the sampler's 2 ms and the hop to the blocking pool.
    assert!(
        at >= Duration::from_millis(300) && at <= Duration::from_millis(420),
        "the first report came at {at:?}"
    );
    let reported = &samples[first..];
    for (_, report) in reported {
        let total = report.total.expect("every report is determinate");
        assert!(report.done <= total, "{report:?}");
    }
    for pair in reported.windows(2) {
        let (a, b) = (&pair[0].1, &pair[1].1);
        let (pa, pb) = (phase_index(&a.phase), phase_index(&b.phase));
        assert!(pa <= pb, "phases move forward: {a:?} then {b:?}");
        if pa == pb {
            assert!(a.done <= b.done, "counts never fall: {a:?} then {b:?}");
        }
    }
    let last = &reported.last().unwrap().1;
    assert_eq!(
        Some(last.done),
        last.total,
        "the last report is at its total"
    );
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.quit().await;
}

/// J15's other half (tC4 `e2e/j15-slow-open.spec.ts`): nothing is reported before 300 ms. The
/// ruling, not the machine's speed, is the criterion, so a loaded machine cannot make it fail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j15_a_quick_open_reports_nothing_before_300_ms() {
    let fs = FaultFs::new();
    let (store, _) = scenario_on(&fs, "sample");
    let dir = tempfile::tempdir().unwrap();
    let t = app_over(dir.path(), Arc::new(store)).await;
    let sink = open_progress();
    let initial = sink.snapshot();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let began = Instant::now();
    let sampler = sample_progress(&sink, began, done.clone());
    let open = t
        .app
        .sessions()
        .open_screen(&folder(SAMPLE), sink.clone())
        .await
        .unwrap();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let samples = sampler.join().unwrap();
    if let Some((at, report)) = samples.iter().find(|(_, p)| *p != initial) {
        assert!(
            *at >= Duration::from_millis(300),
            "a report at {at:?}: {report:?}"
        );
    }
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.quit().await;
}
