//! The scenario loader (unit B1b, APP-DESIGN §4.2, §7.2): a parity scenario assembled from the
//! repository's committed bytes, its projects seeded and checked, a project the check refuses
//! left out with every mismatch, and its sideloaded resources installed under their records.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use tc_app::backend::ProjectStore;
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::{Door43Remote, ScenarioError, ScenarioState, install_resources};
use tc_app::backend::seed::SeedRefusal;
use tc_app::config::ProjectsFolder;
use tc_resources::ResourceStore;

use super::common::{Parts, TestApp, folder, open_progress, repo_root, tit};
use super::state_seed::{bible_declared, obs_written_up, tuple};

const SCENARIOS: [&str; 3] = ["sample", "en-pins", "shared"];

/// Each scenario, assembled once per test binary.
fn scenario(name: &str) -> &'static ScenarioState {
    static STATES: OnceLock<BTreeMap<&'static str, ScenarioState>> = OnceLock::new();
    &STATES.get_or_init(|| {
        SCENARIOS
            .into_iter()
            .map(|s| (s, ScenarioState::from_repository(&repo_root(), s).unwrap()))
            .collect()
    })[name]
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn every_scenario_is_assembled_from_committed_bytes() {
    let recorded: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("parity/app/state.sha256.json")).unwrap(),
    )
    .unwrap();
    for name in SCENARIOS {
        let want: BTreeMap<String, String> = recorded["scenarios"][name]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(path, _)| path.as_str() != "provenance.json")
            .map(|(path, sha)| (path.clone(), sha.as_str().unwrap().to_owned()))
            .collect();
        let got: BTreeMap<String, String> = scenario(name)
            .files()
            .iter()
            .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn a_scenario_read_from_disk_equals_the_one_assembled_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    for (path, bytes) in scenario("shared").files() {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    assert_eq!(
        &ScenarioState::read(dir.path()).unwrap(),
        scenario("shared")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            dir.path().join("client_settings/uw-tc4.json"),
            dir.path().join("vrs/link.json"),
        )
        .unwrap();
        let refused = ScenarioState::read(dir.path()).unwrap_err();
        assert!(
            matches!(&refused, ScenarioError::Io { path, .. } if path == "vrs/link.json"),
            "{refused:?}"
        );
    }
}

/// The OBS sample fails B1a's check with the 53 mismatches the storage team owns
/// (`docs/evidence/state-seed-check-2026-10-18.md`). This test changes the day they fix them:
/// then the OBS project loads, and Home shows both cards (B2).
#[test]
fn every_scenario_loads_its_bible_project_and_refuses_its_obs_project() {
    for name in SCENARIOS {
        let (backend, report) = MemoryBackend::from_scenario(scenario(name)).unwrap();
        let loaded: Vec<String> = report.loaded.iter().map(|(f, _)| f.to_string()).collect();
        assert_eq!(loaded, ["sample_burrito"], "{name}");
        assert_eq!(report.loaded[0].1.declared, bible_declared(), "{name}");
        assert_eq!(report.refused.len(), 1, "{name}");
        let (refused, why) = &report.refused[0];
        assert_eq!(refused.as_str(), "sample_burrito_obs", "{name}");
        let SeedRefusal::Mismatches(mismatches) = why else {
            panic!("{name}: {why}")
        };
        let tuples: Vec<_> = mismatches.iter().map(tuple).collect();
        assert_eq!(tuples, obs_written_up(), "{name}");
        let listed: Vec<String> = backend
            .list()
            .unwrap()
            .projects
            .into_iter()
            .map(|e| e.folder.to_string())
            .collect();
        assert_eq!(
            listed,
            ["sample_burrito"],
            "{name}: nothing of a refused project shows"
        );
    }
}

#[test]
fn only_the_shared_scenario_names_a_door43_remote() {
    for name in SCENARIOS {
        let remotes: Vec<(String, Option<Door43Remote>)> = scenario(name)
            .projects()
            .unwrap()
            .into_iter()
            .map(|p| (p.folder.to_string(), p.remote))
            .collect();
        let shared = |repo: &str| {
            (name == "shared").then(|| Door43Remote {
                owner: "facilitator-zq".to_owned(),
                repo: repo.to_owned(),
            })
        };
        assert_eq!(
            remotes,
            [
                ("sample_burrito".to_owned(), shared("sample_burrito")),
                (
                    "sample_burrito_obs".to_owned(),
                    shared("sample_burrito_obs")
                ),
            ],
            "{name}"
        );
    }
    let (backend, _) = MemoryBackend::from_scenario(scenario("shared")).unwrap();
    assert_eq!(
        backend.project(&folder("sample_burrito")).unwrap().remote,
        Some(Door43Remote {
            owner: "facilitator-zq".to_owned(),
            repo: "sample_burrito".to_owned()
        })
    );
}

#[test]
fn a_scenario_loads_no_teammate_and_no_conflict() {
    for name in SCENARIOS {
        let (backend, _) = MemoryBackend::from_scenario(scenario(name)).unwrap();
        let fold = backend.fold(&folder("sample_burrito")).unwrap();
        assert_eq!(fold.conflict_count(), 0, "{name}");
        assert_eq!(fold.heads(&tit(1, 1)).len(), 1, "{name}");
    }
}

#[test]
fn scenario_resources_install_exactly_the_install_records() {
    let resources = scenario("sample").resources().unwrap();
    assert_eq!(resources.len(), 12);
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path().join("resources")).unwrap();
    let written = tc_resources::content_files_written();
    let installed = install_resources(&resources, &store).unwrap();
    assert_eq!(installed.len(), 12);
    let first = tc_resources::content_files_written() - written;
    assert!(first > 100, "every content file is written once: {first}");

    let identity = |pin: &tc_resources::ResourcePin| (pin.repo.to_string(), pin.sha.to_string());
    let recorded: BTreeSet<_> = resources.iter().map(|r| identity(&r.pin)).collect();
    let present: BTreeSet<_> = store
        .installed_dcs_pins()
        .unwrap()
        .iter()
        .map(identity)
        .collect();
    assert_eq!(present, recorded, "nothing of bundle/en is installed");
    for pin in store.installed_dcs_pins().unwrap() {
        let record = resources
            .iter()
            .find(|r| identity(&r.pin) == identity(&pin))
            .unwrap();
        assert_eq!(pin.flavor, record.pin.flavor, "{}", pin.repo);
    }

    // A restart over the same app data installs nothing.
    let before = tc_resources::content_files_written();
    let again = install_resources(&resources, &store).unwrap();
    assert_eq!(tc_resources::content_files_written(), before);
    assert_eq!(
        again.iter().map(|i| i.dcs_pin()).collect::<Vec<_>>(),
        installed.iter().map(|i| i.dcs_pin()).collect::<Vec<_>>()
    );
}

fn without(state: &ScenarioState, drop: impl Fn(&str) -> bool) -> Vec<(String, Vec<u8>)> {
    state
        .files()
        .iter()
        .filter(|(path, _)| !drop(path))
        .map(|(p, b)| (p.clone(), b.clone()))
        .collect()
}

#[test]
fn a_state_with_an_unknown_entry_or_an_unmatched_record_is_refused() {
    let sample = scenario("sample");
    let mut files = without(sample, |_| false);
    files.push(("notes/readme.txt".to_owned(), b"x".to_vec()));
    let refused = ScenarioState::from_files(files).unwrap_err();
    assert!(
        matches!(&refused, ScenarioError::Layout { path, .. } if path == "notes/readme.txt"),
        "{refused:?}"
    );

    let no_folder = ScenarioState::from_files(without(sample, |p| {
        p.starts_with("repos/_local_/_sideloaded_/en_tn/")
    }))
    .unwrap();
    let refused = no_folder.resources().unwrap_err();
    assert!(
        matches!(&refused, ScenarioError::Records { problem } if problem.contains("en_tn")),
        "{refused:?}"
    );

    let mut records: serde_json::Value =
        serde_json::from_slice(&sample.files()["client_settings/uw-tc4.json"]).unwrap();
    records["installedResources"]
        .as_object_mut()
        .unwrap()
        .remove("_local_/_sideloaded_/en_ta")
        .unwrap();
    let mut files = without(sample, |p| p == "client_settings/uw-tc4.json");
    files.push((
        "client_settings/uw-tc4.json".to_owned(),
        serde_json::to_vec(&records).unwrap(),
    ));
    let refused = ScenarioState::from_files(files)
        .unwrap()
        .resources()
        .unwrap_err();
    assert!(
        matches!(&refused, ScenarioError::Records { problem } if problem.contains("en_ta")),
        "{refused:?}"
    );
}

#[test]
fn a_project_folder_that_is_no_project_ref_is_refused() {
    let state = ScenarioState::from_files([(
        "repos/_local_/_local_/a:b/metadata.json".to_owned(),
        b"{}".to_vec(),
    )])
    .unwrap();
    let refused = state.projects().unwrap_err();
    assert!(
        matches!(&refused, ScenarioError::Layout { path, .. } if path.ends_with("a:b")),
        "{refused:?}"
    );
}

#[test]
fn a_remote_list_without_an_origin_is_refused() {
    let mut files = without(scenario("shared"), |_| false);
    for (path, bytes) in &mut files {
        if path == "repos/_local_/_local_/sample_burrito/.remotes.json" {
            *bytes = br#"[{"name":"upstream","url":"https://git.door43.org/a/b.git"}]"#.to_vec();
        }
    }
    let refused = ScenarioState::from_files(files)
        .unwrap()
        .projects()
        .unwrap_err();
    assert!(
        matches!(refused, ScenarioError::Remote { .. }),
        "{refused:?}"
    );
}

/// Phase 1 writes no project (§10): the memory backend holds its folds in memory, so the
/// app's projects folder stays empty through an open and a close.
#[tokio::test]
async fn the_projects_folder_stays_empty() {
    let (backend, _) = MemoryBackend::from_scenario(scenario("sample")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = tc_app::StoreChoice::Given(std::sync::Arc::new(backend));
    })
    .await;
    let opened = t
        .app
        .sessions()
        .open_screen(&folder("sample_burrito"), open_progress())
        .await
        .unwrap();
    t.app.sessions().leave_screen(&opened.at).unwrap();
    t.app.sessions().settled(&folder("sample_burrito")).await;
    assert_eq!(entries(&projects), 0);
}

fn entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().count()
}
