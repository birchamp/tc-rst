//! `tc4 serve --scenario <state dir>` (unit B1b, APP-DESIGN §4.2, §7.1): the real binary starts
//! on a parity scenario's projects, held by the memory backend, with the scenario's sideloaded
//! resources in its own store, and writes no project.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{START, Tc4, http};
use tc_app::backend::scenario::ScenarioState;
use tc_resources::ResourceStore;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `sample` scenario written out as `node parity/app/state.mjs` lays it out.
fn write_sample(dir: &Path) {
    let state = ScenarioState::from_repository(&repo_root(), "sample").unwrap();
    for (path, bytes) in state.files() {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
}

#[test]
fn tc4_serve_with_a_scenario_loads_its_projects_and_installs_its_resources() {
    let work = TempDir::new().unwrap();
    let state = work.path().join("state");
    write_sample(&state);
    let app_data = work.path().join("app-data");
    let projects = work.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());

    let mut tc4 = Tc4::spawn(
        &app_data,
        &ui,
        &[
            "--test-hooks",
            "--projects",
            projects.to_str().unwrap(),
            "--scenario",
            state.to_str().unwrap(),
        ],
    );
    tc4.address(START);
    let quiet = tc4.api("test/quiet", "{}");
    assert_eq!(quiet.status, 200, "{}", quiet.text());
    let hello = tc4.api("app/hello", "{}");
    assert_eq!(hello.status, 200, "{}", hello.text());
    assert_eq!(hello.json()["value"]["sessions"], serde_json::json!([]));

    let stderr = tc4.stderr();
    assert!(
        stderr.contains("loaded sample_burrito (3 declared differences)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("refused sample_burrito_obs, 53 mismatches"),
        "{stderr}"
    );
    assert!(stderr.contains("12 resources installed"), "{stderr}");
    assert!(tc4.quit().success());

    let store = ResourceStore::open_read_only(app_data.join("resources")).unwrap();
    assert_eq!(store.installed_dcs_pins().unwrap().len(), 12);
    assert_eq!(
        std::fs::read_dir(&projects).unwrap().count(),
        0,
        "the memory backend writes no project"
    );
}

#[test]
fn a_missing_scenario_folder_stops_the_start_with_its_path() {
    let work = TempDir::new().unwrap();
    let missing = work.path().join("no-such-state");
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let mut tc4 = Tc4::spawn(
        &work.path().join("app-data"),
        &ui,
        &["--scenario", missing.to_str().unwrap()],
    );
    let status = tc4.wait_exit(Duration::from_secs(20)).expect("tc4 ends");
    assert_eq!(status.code(), Some(4), "the start failed");
    let stdout = tc4.rest_of_stdout();
    assert_eq!(
        stdout.len(),
        1,
        "one error line and nothing else: {stdout:?}"
    );
    let line: serde_json::Value = serde_json::from_str(&stdout[0]).unwrap();
    assert_eq!(line["error"]["type"], "start");
    let reason = line["error"]["value"]["reason"].as_str().unwrap();
    assert!(reason.contains("no-such-state"), "{reason}");
    assert!(tc4.stderr().contains("no-such-state"), "{}", tc4.stderr());
}
