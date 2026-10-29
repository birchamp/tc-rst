//! #705 repro: how audio ingredients sync between translators, through Door43. Ana and Juan
//! each put a recording of TIT 1:1 at the same path (another program writes it into the
//! project folder), both sync. What does each computer hold after, and what is on Door43?
//!
//! Goes at crates/tc-sync/tests/repro_705.rs. Run:
//!   tc-heavy cargo test -p tc-sync --test repro_705 repro_705 -- --nocapture

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code, unused_imports)]

#[path = "sync/door43.rs"]
mod door43;

use door43::*;
use std::path::Path;
use tc_door43_fake::seeded;

fn door43_audio(door43: &Door43, name: &str) -> Vec<(String, usize)> {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let tree = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    let mut out = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        let path = format!("{dir}{}", entry.name().unwrap_or_default());
        if path.starts_with("ingredients/audio/") && entry.kind() == Some(git2::ObjectType::Blob)
        {
            out.push((path, bare.find_blob(entry.id()).unwrap().content().len()));
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    out
}

fn audio_files(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let dir = folder.join("ingredients/audio");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .map(|r| {
            r.map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn put(handle: &tc_store::ProjectHandle, folder: &Path, rel: &str, bytes: &[u8]) {
    std::fs::create_dir_all(folder.join(rel).parent().unwrap()).unwrap();
    std::fs::write(folder.join(rel), bytes).unwrap();
    tokio::task::block_in_place(|| handle.focus().wait().unwrap());
    handle.wait_idle();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_705_two_recordings_of_one_verse_through_door43() {
    let name = "titus-recordings";
    let door43 = door43(name).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, name);
    synced(&sync(&a, &door43).await);
    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/{name}", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(&juan.store, &door43.client, Some(&door43.session), &repo, "Titus")
        .await
        .unwrap()
        .folder;
    let b = open(&juan, &folder_b);

    let song = "ingredients/audio/TIT_01_01.wav";
    let anas: Vec<u8> = (0..30_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let juans: Vec<u8> = (0..45_000u32).map(|i| (i * 13 % 241) as u8).collect();
    put(&a, &folder_a, song, &anas);
    put(&b, &folder_b, song, &juans);
    synced(&sync(&a, &door43).await);
    println!("#705 Door43 after Ana: {:?}", door43_audio(&door43, name));
    let report = sync(&b, &door43).await;
    synced(&report);
    println!("#705 Juan's sync: new_conflicts={}", report.new_conflicts.len());
    println!("#705 Door43 after Juan: {:?}", door43_audio(&door43, name));
    synced(&sync(&a, &door43).await);

    let on_a = audio_files(&folder_a);
    let on_b = audio_files(&folder_b);
    let show = |files: &[(String, Vec<u8>)]| -> Vec<(String, usize)> {
        files.iter().map(|(n, b)| (n.clone(), b.len())).collect()
    };
    println!("#705 Ana holds: {:?}", show(&on_a));
    println!("#705 Juan holds: {:?}", show(&on_b));
    assert_eq!(on_a, on_b, "both computers hold the same recordings, byte for byte");
    assert_eq!(on_a.len(), 2, "both recordings are kept");
    assert!(on_a.iter().any(|(_, bytes)| *bytes == anas));
    assert!(on_a.iter().any(|(_, bytes)| *bytes == juans));
    assert!(on_a.iter().any(|(n, _)| n == "TIT_01_01.wav"), "the newest keeps the name");
    let metadata = std::fs::read_to_string(folder_a.join("metadata.json")).unwrap();
    for (n, _) in &on_a {
        assert!(metadata.contains(&format!("ingredients/audio/{n}")), "{n} is listed");
    }
    // A third computer opening from Door43 gets both.
    let carla = computer(dir.path(), "carla");
    let folder_c = tc_sync::join(&carla.store, &door43.client, Some(&door43.session), &repo, "Titus")
        .await
        .unwrap()
        .folder;
    let c = open(&carla, &folder_c);
    println!("#705 Carla (joined) holds: {:?}", show(&audio_files(&folder_c)));
    assert_eq!(audio_files(&folder_c), on_a);
    a.close().unwrap();
    b.close().unwrap();
    c.close().unwrap();
}
