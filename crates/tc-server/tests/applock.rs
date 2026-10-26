//! The app-data lock of the restart case (APP-DESIGN §5.1, AD39). The operating system's lock
//! conflicts between two handles of one process (`flock`, `LockFileEx`), so these tests run
//! in process and are real. `crates/tc4/tests/lifecycle.rs` runs the wait between processes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::time::{Duration, Instant};

use tc_server::{AppLock, AppLockError, Holder};

fn held(result: Result<AppLock, AppLockError>) -> Holder {
    match result {
        Err(AppLockError::Held(holder)) => holder,
        Err(other) => panic!("not Held: {other}"),
        Ok(_) => panic!("the lock was taken twice"),
    }
}

#[test]
fn a_second_take_waits_then_names_the_holder() {
    let dir = tempfile::tempdir().unwrap();
    let _first = AppLock::take(dir.path(), Duration::ZERO).unwrap();
    let began = Instant::now();
    let holder = held(AppLock::take(dir.path(), Duration::from_millis(300)));
    let waited = began.elapsed();
    assert_eq!(
        holder,
        Holder {
            pid: Some(std::process::id())
        }
    );
    assert!(
        (Duration::from_millis(250)..Duration::from_secs(2)).contains(&waited),
        "waited {waited:?}"
    );
}

#[test]
fn a_take_succeeds_once_the_holder_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let first = AppLock::take(dir.path(), Duration::ZERO).unwrap();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        drop(first);
    });
    let began = Instant::now();
    let second = AppLock::take(dir.path(), Duration::from_secs(5));
    let waited = began.elapsed();
    releaser.join().unwrap();
    assert!(second.is_ok(), "{:?}", second.err());
    assert!(waited < Duration::from_secs(2), "waited {waited:?}");
    assert!(
        waited >= Duration::from_millis(100),
        "did not wait: {waited:?}"
    );
}

#[test]
fn a_missing_or_broken_record_is_unnamed() {
    let dir = tempfile::tempdir().unwrap();
    let _first = AppLock::take(dir.path(), Duration::ZERO).unwrap();
    let record = dir.path().join("app-lock.json");
    for broken in [
        &b"x"[..],
        b"",
        b"{\"pid\":\"seven\",\"at\":1}",
        b"{\"at\":1}",
    ] {
        std::fs::write(&record, broken).unwrap();
        let holder = held(AppLock::take(dir.path(), Duration::from_millis(100)));
        assert_eq!(holder, Holder { pid: None }, "{broken:?}");
    }
    std::fs::remove_file(&record).unwrap();
    let holder = held(AppLock::take(dir.path(), Duration::from_millis(100)));
    assert_eq!(holder, Holder { pid: None }, "no record at all");
}

#[test]
fn the_record_names_the_new_holder() {
    let dir = tempfile::tempdir().unwrap();
    // A record an earlier, dead holder left: it must not outlive the next take.
    std::fs::write(dir.path().join("app-lock.json"), r#"{"pid":1,"at":1}"#).unwrap();
    drop(AppLock::take(dir.path(), Duration::ZERO).unwrap());
    let _again = AppLock::take(dir.path(), Duration::ZERO).unwrap();
    let text = std::fs::read_to_string(dir.path().join("app-lock.json")).unwrap();
    let record: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(record["pid"], std::process::id());
    let at = record["at"].as_i64().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!((0..60_000).contains(&(now - at)), "at {at}, now {now}");
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["app-lock.json", "app.lock"],
        "no temporary file is left"
    );
}

#[test]
fn a_folder_that_is_not_there_is_an_io_error_not_a_wait() {
    let dir = tempfile::tempdir().unwrap();
    let began = Instant::now();
    let result = AppLock::take(&dir.path().join("missing"), Duration::from_secs(10));
    assert!(matches!(result, Err(AppLockError::Io { .. })));
    assert!(began.elapsed() < Duration::from_secs(1), "it waited");
}
