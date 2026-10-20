//! `tc4 serve --scenario <state dir>` (units B1b and C1a, APP-DESIGN §4.2, §5.1, §7.1): the real
//! binary starts on a parity scenario's projects, written into its empty projects folder through
//! tc-store (or held by the memory backend with `--backend memory`, which writes nothing), with
//! the scenario's sideloaded resources in its own store.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
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

/// Every file under `folder`, by its `/`-separated path relative to it.
fn files_on_disk(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// Starts `tc4 serve --scenario` on the sample over an empty projects folder, with `extra`
/// flags, and checks what every scenario start says: the loaded and the refused project, the
/// installed resources, no session.
fn serve_sample(extra: &[&str]) -> (TempDir, PathBuf, PathBuf, String) {
    let work = TempDir::new().unwrap();
    let state = work.path().join("state");
    write_sample(&state);
    let app_data = work.path().join("app-data");
    let projects = work.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let mut flags = vec![
        "--test-hooks",
        "--projects",
        projects.to_str().unwrap(),
        "--scenario",
        state.to_str().unwrap(),
    ];
    flags.extend_from_slice(extra);
    let mut tc4 = Tc4::spawn(&app_data, &ui, &flags);
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
    // The whole line, so a format string that lost a continuation fails here.
    let refused =
        "refused sample_burrito_obs, 52 mismatches (docs/evidence/state-seed-check-2026-10-18.md):";
    assert!(
        stderr
            .lines()
            .any(|line| line == format!("tc4: scenario {}: {refused}", state.display())),
        "{stderr}"
    );
    assert!(stderr.contains("12 resources installed"), "{stderr}");
    assert!(tc4.quit().success());

    let store = ResourceStore::open_read_only(app_data.join("resources")).unwrap();
    assert_eq!(store.installed_dcs_pins().unwrap().len(), 12);
    (work, state, projects, stderr)
}

/// C1a: on the store the scenario's project is written into the projects folder, holding
/// tC4's own bytes.
#[test]
fn tc4_serve_with_a_scenario_writes_its_projects_to_the_store() {
    let (_work, state, projects, stderr) = serve_sample(&[]);
    let names: Vec<String> = std::fs::read_dir(&projects)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["sample_burrito"]);
    assert!(
        stderr.lines().any(|line| line
            == format!(
                "tc4: scenario {}: 1 projects written to {}",
                state.display(),
                projects.display()
            )),
        "{stderr}"
    );
    let files = files_on_disk(&projects.join("sample_burrito"));
    let titus =
        std::fs::read(state.join("repos/_local_/_local_/sample_burrito/ingredients/TIT.usfm"))
            .unwrap();
    assert_eq!(files.get("ingredients/TIT.usfm"), Some(&titus));
    // The writer validates the folder as a Scripture Burrito before the start goes on
    // (tc-app's tests/app/store_scenario.rs covers a folder that is not one).
    assert!(files.contains_key("metadata.json"));
    assert!(files.contains_key("ingredients/tc/project.json"));
}

/// `--backend memory` keeps phase 1's environment: the projects are held in memory and
/// nothing is written.
#[test]
fn tc4_serve_with_a_scenario_on_the_memory_backend_writes_no_project() {
    let (_work, _state, projects, stderr) = serve_sample(&["--backend", "memory"]);
    assert_eq!(
        std::fs::read_dir(&projects).unwrap().count(),
        0,
        "the memory backend writes no project"
    );
    assert!(!stderr.contains("projects written"), "{stderr}");
}

/// A scenario never writes into a folder that holds anyone's work: the start fails, naming
/// the folder, and the folder keeps every byte.
#[test]
fn a_scenario_refuses_a_projects_folder_that_holds_anything() {
    let work = TempDir::new().unwrap();
    let state = work.path().join("state");
    write_sample(&state);
    let projects = work.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    std::fs::write(projects.join("notes.txt"), b"someone's notes").unwrap();
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let mut tc4 = Tc4::spawn(
        &work.path().join("app-data"),
        &ui,
        &[
            "--projects",
            projects.to_str().unwrap(),
            "--scenario",
            state.to_str().unwrap(),
        ],
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
    let reason = line["error"]["value"]["reason"].as_str().unwrap();
    assert!(
        reason.contains(&projects.display().to_string()) && reason.contains("not empty"),
        "{reason}"
    );
    let names: Vec<String> = std::fs::read_dir(&projects)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["notes.txt"]);
    assert_eq!(
        std::fs::read(projects.join("notes.txt")).unwrap(),
        b"someone's notes"
    );
}

#[test]
fn a_missing_scenario_folder_stops_the_start_with_its_path() {
    let work = TempDir::new().unwrap();
    let missing = work.path().join("no-such-state");
    let projects = work.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let mut tc4 = Tc4::spawn(
        &work.path().join("app-data"),
        &ui,
        &[
            "--projects",
            projects.to_str().unwrap(),
            "--scenario",
            missing.to_str().unwrap(),
        ],
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
