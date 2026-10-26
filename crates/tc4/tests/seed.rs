//! First-launch seeding through the real binary (APP-DESIGN §10, unit A9): `tc4 serve --bundle`
//! says "preparing" from its first answer, finishes the seed, stops it when the shell closes,
//! and a repeat launch touches no resource file. The bundled store is the four Open Bible
//! Stories resources of `bundle/en` (about 60 files), built once per test run with the
//! code the installer's `bundle_store` example runs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use common::{START, Tc4, http};
use serde_json::{Value, json};
use tc_resources::ResourceStore;
use tempfile::TempDir;

#[allow(dead_code)]
#[path = "../../tc-app/examples/bundle_store/build.rs"]
mod bundle_build;

/// The bundled store of these tests, built once per test run.
fn bundle() -> &'static Path {
    static BUNDLE: OnceLock<PathBuf> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en");
        let obs = ["en_obs", "en_obs-tn", "en_obs-tq", "en_obs-twl"];
        bundle_build::build_once_per_run(
            &PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
            "a9-bundle-tc4",
            &from,
            &|pin| obs.contains(&pin.repo.name().as_str()),
            4,
        )
        .unwrap_or_else(|e| panic!("the test bundle could not be built: {e}"))
    })
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

fn bundled_files() -> u64 {
    files_in(&ResourceStore::open_read_only(bundle()).unwrap())
}

struct Dirs {
    app_data: TempDir,
    _ui_root: TempDir,
    ui: PathBuf,
}

fn dirs() -> Dirs {
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    Dirs {
        app_data: TempDir::new().unwrap(),
        _ui_root: ui_root,
        ui,
    }
}

fn resources(dirs: &Dirs) -> PathBuf {
    dirs.app_data.path().join("resources")
}

/// `tc4 serve --bundle <bundle> <extra>`, with its address line read.
fn start(dirs: &Dirs, bundle: &Path, extra: &[&str]) -> Tc4 {
    let bundle = bundle.display().to_string();
    let mut args = vec!["--bundle", bundle.as_str()];
    args.extend_from_slice(extra);
    let mut tc4 = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &args);
    tc4.address(START);
    tc4
}

fn hello(tc4: &Tc4) -> Value {
    let reply = tc4.api("app/hello", "{}");
    assert_eq!(reply.status, 200, "{}", reply.text());
    let json = reply.json();
    assert_eq!(json["type"], "ok", "{}", reply.text());
    json["value"].clone()
}

/// Polls `hello` until its startup state is not `preparing`, and returns that state.
fn until_startup_ends(tc4: &Tc4) -> Value {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let startup = hello(tc4)["startup"].clone();
        if startup["type"] != "preparing" {
            return startup;
        }
        assert!(Instant::now() < until, "still preparing: {startup}");
        std::thread::sleep(Duration::from_millis(20));
    }
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

fn verified_installs(resources: &Path) -> usize {
    let store = ResourceStore::open_read_only(resources).unwrap();
    let inventory = store.installed().unwrap();
    assert!(inventory.unreadable.is_empty());
    for install in &inventory.installs {
        install.verify().unwrap();
    }
    inventory.installs.len()
}

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

#[test]
fn a_first_launch_says_preparing_at_once_and_then_ready() {
    let dirs = dirs();
    let n = bundled_files();
    // 10 ms before each file keeps the seed running for 630 ms or more, so the first answer
    // finds it under way on any disk.
    let tc4 = start(&dirs, bundle(), &["--test-seed-pace", "10"]);

    let first = hello(&tc4)["startup"].clone();
    assert_eq!(first["type"], "preparing", "{first}");
    assert_eq!(first["value"]["total"], n);

    assert_eq!(until_startup_ends(&tc4), json!({"type": "ready"}));
    assert_eq!(verified_installs(&resources(&dirs)), 4);
    let mut tc4 = tc4;
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn quit_while_preparing_stops_the_seed_and_the_next_launch_finishes_it() {
    let dirs = dirs();
    let n = bundled_files();
    let resources = resources(&dirs);
    let mut tc4 = start(&dirs, bundle(), &["--test-seed-pace", "30"]);
    // Wait until one install is whole, so the stop falls inside a later one.
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let whole = ResourceStore::open_read_only(&resources)
            .unwrap()
            .installed()
            .unwrap()
            .installs
            .len();
        if whole >= 1 {
            break;
        }
        assert!(Instant::now() < until, "no install was ever whole");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(hello(&tc4)["startup"]["type"], "preparing");

    // Close is a quit: `closed` and exit 0 inside the 5 s of the close.
    let began = Instant::now();
    tc4.send_line(r#"{"quit":true}"#);
    assert_eq!(
        tc4.next_line(Duration::from_secs(5)).as_deref(),
        Some(r#"{"closed":true}"#),
        "stderr: {}",
        tc4.stderr()
    );
    let status = tc4.wait_exit(Duration::from_secs(5)).expect("tc4 ended");
    assert_eq!(status.code(), Some(0));
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    drop(tc4);

    // Only whole installs, each checked, and fewer than the bundle holds.
    let kept = ResourceStore::open_read_only(&resources).unwrap();
    let installs = kept.installed().unwrap().installs;
    assert!(
        (1..4).contains(&installs.len()),
        "{} installs",
        installs.len()
    );
    for install in &installs {
        install.verify().unwrap();
    }
    let before: Vec<_> = installs.iter().map(|i| tree(i.dir())).collect();
    let missing = n - files_in(&kept);

    // The next launch copies only what is missing, and leaves the earlier copies alone.
    let mut tc4 = start(&dirs, bundle(), &[]);
    let first = hello(&tc4)["startup"].clone();
    if first["type"] == "preparing" {
        assert_eq!(first["value"]["total"], missing, "{first}");
    }
    assert_eq!(until_startup_ends(&tc4), json!({"type": "ready"}));
    assert_eq!(verified_installs(&resources), 4);
    for tree_before in before {
        for (path, was) in tree_before {
            assert_eq!(
                tree(path.parent().unwrap())[&path],
                was,
                "{}",
                path.display()
            );
        }
    }
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn a_repeat_launch_is_ready_at_once_and_changes_no_resource_file() {
    let dirs = dirs();
    let resources = resources(&dirs);
    let mut first = start(&dirs, bundle(), &[]);
    assert_eq!(until_startup_ends(&first), json!({"type": "ready"}));
    assert_eq!(first.quit().code(), Some(0));
    drop(first);
    let before = tree(&resources);
    assert!(!before.is_empty());

    let mut second = start(&dirs, bundle(), &[]);
    assert_eq!(
        hello(&second)["startup"],
        json!({"type": "ready"}),
        "the first answer of a repeat launch"
    );
    assert_eq!(second.quit().code(), Some(0));
    assert_eq!(tree(&resources), before, "bytes and times stay");
}

#[test]
fn a_damaged_bundle_fails_the_start_and_says_why() {
    let dirs = dirs();
    let damaged = dirs.app_data.path().join("damaged-bundle");
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

    let mut tc4 = start(&dirs, &damaged, &[]);
    let ended = until_startup_ends(&tc4);
    assert_eq!(ended["type"], "failed", "{ended}");
    let reason = ended["value"]["reason"].as_str().unwrap();
    assert!(reason.contains(&name), "{reason}");

    // `tc4` keeps serving, so the window can show the reason; the person closes it.
    assert_eq!(hello(&tc4)["startup"], ended);
    assert_eq!(verified_installs(&resources(&dirs)), 3);
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn a_bundle_that_lost_an_install_fails_the_start_at_its_first_answer() {
    let dirs = dirs();
    let damaged = dirs.app_data.path().join("damaged-bundle");
    copy_tree(bundle(), &damaged);
    let inventory = ResourceStore::open_read_only(&damaged)
        .unwrap()
        .installed()
        .unwrap();
    let lost = &inventory.installs[2];
    let name = lost.dcs_pin().unwrap().repo.name().as_str().to_owned();
    fs::remove_dir_all(lost.dir()).unwrap();

    let mut tc4 = start(&dirs, &damaged, &[]);

    // Known from the plan, so no seed ran: the first answer is the failure.
    let startup = hello(&tc4)["startup"].clone();
    assert_eq!(startup["type"], "failed", "{startup}");
    let reason = startup["value"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("incomplete") && reason.contains(&name),
        "{reason}"
    );
    assert_eq!(
        verified_installs(&resources(&dirs)),
        0,
        "nothing was copied"
    );
    assert_eq!(tc4.quit().code(), Some(0));
}
