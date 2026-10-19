//! A release build of `tc4` has no test hook (AD35).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{START, Tc4, http};
use tempfile::TempDir;

/// The words no release binary may hold: every endpoint name of the test service, the
/// application's and the stand-in's alike, every flag only a test build has, and the batch
/// tag of the fixture backend's seed (feature `fixture-backend`). Since B1b `tc4 serve
/// --scenario` calls the seed in a build with the feature, so the tag and the flag guard the
/// release binary as well as `release_build_resolves_no_fixture_backend_and_no_test_hook`.
const HOOK_TEXT: [&str; 14] = [
    "test/crash",
    "test/fieldStore",
    "test/saveVerse",
    "test/fieldLog",
    "test/fieldDraft",
    "test/echo",
    "test/quiet",
    "test/hold",
    "test/release",
    "test/reset",
    "test/keychain",
    "--test-seed-pace",
    "--scenario",
    "tc4-seed-batch",
];

fn release_binary() -> PathBuf {
    let Some(given) = std::env::var_os("TC4_RELEASE_BIN") else {
        panic!(
            "set TC4_RELEASE_BIN to a release build of tc4, made without test hooks:\n  \
             cargo build --release -p tc4 -j 2\n  \
             TC4_RELEASE_BIN=target/release/tc4 cargo test -p tc4 --test release -j 2 -- --ignored"
        );
    };
    let given = PathBuf::from(given);
    if given.is_absolute() {
        given
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(given)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The features `cargo build --release -p tc4` resolves, as `cargo tree` gives them without
/// dev-dependencies: no crate of the workspace has `fixture-backend` or `test-hooks` (AD35).
/// A dev-dependency turns them on in test builds only, so this checks what the release
/// build compiles, whether or not the binary keeps a trace of it. `cargo tree` reads the
/// manifests and the lock file: it builds nothing and, offline, reaches no network.
#[test]
fn release_build_resolves_no_fixture_backend_and_no_test_hook() {
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "tree",
            "-p",
            "tc4",
            "-e",
            "no-dev",
            "--offline",
            "--locked",
            "--prefix",
            "none",
            "-f",
            "{p}|{f}",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    let mut crates = BTreeSet::new();
    for line in tree.lines() {
        let line = line.trim_end_matches(" (*)");
        let Some((package, features)) = line.split_once('|') else {
            continue;
        };
        let name = package.split_whitespace().next().unwrap_or_default();
        crates.insert(name.to_owned());
        for feature in features.split(',') {
            assert!(
                !["fixture-backend", "test-hooks"].contains(&feature),
                "a release build of tc4 turns on {name}/{feature}:\n{tree}"
            );
        }
    }
    // The tree is the one meant: tc4 and the crates that have the two features.
    for name in ["tc4", "tc-app", "tc-server", "tc-api"] {
        assert!(crates.contains(name), "{name} is not in:\n{tree}");
    }
}

/// Checks the release binary itself, so the pipeline checks what it ships. Run after
/// `cargo build --release -p tc4 -j 2` (not `--features test-hooks`):
/// `TC4_RELEASE_BIN=target/release/tc4 cargo test -p tc4 --test release -j 2 -- --ignored`.
/// It starts no nested build: that would race the test run's own lock on the target folder.
#[test]
#[ignore = "needs a release build of tc4; see the doc comment"]
fn release_has_no_test_hooks() {
    let binary = release_binary();
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let app_data = TempDir::new().unwrap();

    // No flag: it is not an option of this build.
    let refused = Command::new(&binary)
        .args(["serve", "--app-data"])
        .arg(app_data.path())
        .arg("--ui")
        .arg(&ui)
        .arg("--test-hooks")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("unknown flag --test-hooks"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(refused.stdout.is_empty());

    // The pace of the seed is a test flag as well: a release build seeds as fast as it can.
    let refused = Command::new(&binary)
        .args(["serve", "--app-data"])
        .arg(app_data.path())
        .arg("--ui")
        .arg(&ui)
        .args(["--test-seed-pace", "30"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("unknown flag --test-seed-pace"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(refused.stdout.is_empty());

    // A scenario is the fixture backend's: a release build opens only real projects.
    let refused = Command::new(&binary)
        .args(["serve", "--app-data"])
        .arg(app_data.path())
        .arg("--ui")
        .arg(&ui)
        .args(["--scenario", "state"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("unknown flag --scenario"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(refused.stdout.is_empty());

    // No route: the test service answers 404 and the product 200.
    let mut tc4 = Tc4::spawn_binary(&binary, app_data.path(), &ui, &[]);
    tc4.address(START);
    for path in ["test/quiet", "test/crash", "test/echo"] {
        assert_eq!(tc4.api(path, "{}").status, 404, "{path}");
    }
    assert_eq!(tc4.api("app/hello", "{}").status, 200);
    assert_eq!(tc4.quit().code(), Some(0));
    drop(tc4);
    std::thread::sleep(Duration::from_millis(50));

    // No text: not the routes, not the crash exit's hooks, not the names.
    let bytes = std::fs::read(&binary).unwrap();
    for text in HOOK_TEXT {
        assert_eq!(
            find(&bytes, text.as_bytes()),
            None,
            "the release binary holds {text:?}"
        );
    }
}
