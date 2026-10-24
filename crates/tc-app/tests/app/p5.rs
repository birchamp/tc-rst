//! P5/INV-VALID-BURRITO: every project state a tc-app test's store writes is looked at as a
//! Scripture Burrito without the test asking (`support/store_world.rs`, `BurritoWatch`).

use std::path::{Path, PathBuf};

use tc_app::backend::{CheckpointReason, ProjectStore};

use crate::store_world::{PROJECTS, burrito_probe, files, progress, save, store_world, tit};

#[test]
fn p5_probe_names_a_planted_bad_md5() {
    let mut world = store_world();
    let folder = Path::new(PROJECTS).join(world.bible.as_str());
    let metadata = folder.join("metadata.json");
    let text = String::from_utf8(files(&world.fs, &folder)["metadata.json"].clone()).unwrap();
    let at = text.find("\"md5\": \"").unwrap() + "\"md5\": \"".len();
    let planted = format!("{}{}{}", &text[..at], "0".repeat(32), &text[at + 32..]);
    world.fs.put(&metadata, planted.as_bytes(), None);
    let problems = burrito_probe(&world.fs)(&folder);
    assert!(
        problems.iter().any(|p| p.contains("ChecksumMismatch")),
        "{problems:#?}"
    );
    assert!(
        problems
            .iter()
            .all(|p| p.contains("is not a valid Scripture Burrito"))
    );
    world.watch.stop("this test planted a wrong md5 itself");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[should_panic(expected = "P5/INV-VALID-BURRITO")]
async fn p5_watch_fails_a_test_whose_store_reported_a_problem() {
    let world = store_world();
    plant_problem_in_the_save(&world);
    let handle = world.backend.open(&world.bible, progress()).unwrap();
    handle.wait_idle();
    save(handle.as_ref(), &tit(1, 1), SAVED).await.unwrap();
    // No wait: the watch itself waits for the writer to look at what the save wrote.
    drop(handle);
    drop(world);
}

/// The same with the project still open when the watch looks, as when an app's background
/// task outlives the test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[should_panic(expected = "P5/INV-VALID-BURRITO")]
async fn p5_watch_sees_the_last_save_of_a_project_still_open() {
    let world = store_world();
    plant_problem_in_the_save(&world);
    let handle = world.backend.open(&world.bible, progress()).unwrap();
    handle.wait_idle();
    save(handle.as_ref(), &tit(1, 1), SAVED).await.unwrap();
    // A checkpoint has the writer put the save in the folder, in the background.
    let _ = handle
        .checkpoint(CheckpointReason::Other("the P5 test".to_owned()))
        .await;
    drop(world);
    drop(handle);
}

const SAVED: &str = "a verse the writer then looks at";

/// A problem only in the state a save of [`SAVED`] writes: the watch must see the last write.
fn plant_problem_in_the_save(world: &crate::store_world::StoreWorld) {
    let fs = world.fs.clone();
    world.hooks.check_burrito(move |folder: &Path| {
        // The book file while the project is open holds it only after a leave; the journal
        // under ingredients/ holds it from the save on.
        fs.files_under(&folder.join("ingredients"))
            .iter()
            .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains(SAVED))
            .then(|| format!("{} holds the save, planted by the test", folder.display()))
            .into_iter()
            .collect()
    });
}

/// Every store a tc-app test starts goes through `backend_over` or `watch_over`, or checks a
/// real folder itself, and no test throws its watch away.
#[test]
fn every_store_a_tc_app_test_builds_is_watched() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut problems = Vec::new();
    for file in rust_files(&tests) {
        let rel = file
            .strip_prefix(&tests)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        // The watch itself, and this lint, which names what it looks for.
        if rel == "support/store_world.rs" || rel == "app/p5.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("StoreBackend::start(") || line.contains("Store::start") {
                let near = lines[i.saturating_sub(25)..(i + 25).min(lines.len())].join("\n");
                let watched = near.contains("watch_over(")
                    || near.contains("world.hooks")
                    || near.contains("assert_valid_burrito_files(");
                if !watched {
                    problems.push(format!(
                        "P5/INV-VALID-BURRITO: tests/{rel}:{} starts a store no watch looks at; use backend_over or watch_over",
                        i + 1
                    ));
                }
            }
        }
        // A statement as one line, so a call rustfmt wrapped is still seen whole.
        for statement in text.split(';') {
            let flat = statement.split_whitespace().collect::<Vec<_>>().join(" ");
            let Some(at) = flat.find("backend_over(") else {
                continue;
            };
            let before = &flat[..at];
            let thrown = before.ends_with("let _ = ")
                || before.trim_end().ends_with(", _) =")
                || flat.contains(").0");
            if thrown {
                let start = statement.as_ptr() as usize - text.as_ptr() as usize;
                let call = start + statement.find("backend_over(").unwrap_or(0);
                let line = text[..call].matches('\n').count() + 1;
                problems.push(format!(
                    "P5/INV-VALID-BURRITO: tests/{rel}:{line} throws the store's watch away"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
