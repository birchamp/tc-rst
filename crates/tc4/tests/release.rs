//! A release build of `tc4` has no test hook (AD35).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{START, Tc4, http};
use tempfile::TempDir;

/// The words no release binary may hold: every endpoint name of the test service, the
/// application's and the stand-in's alike.
const HOOK_TEXT: [&str; 8] = [
    "test/crash",
    "test/fieldStore",
    "test/fieldLog",
    "test/echo",
    "test/quiet",
    "test/hold",
    "test/release",
    "test/reset",
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
