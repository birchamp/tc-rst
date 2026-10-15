// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]
// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod aquifer_fake;

use std::io::Write;
use std::path::PathBuf;

use tc_door43::{Door43Client, InternetSwitch};
use tc_door43_fake::FakeDoor43;
use tc_resources::{RepoPath, ResourcePin, ResourceStore, pin_release};

use tempfile::TempDir;

/// A seeded fake Door43, a client for it with the internet on, and an empty store.
pub struct Rig {
    pub fake: FakeDoor43,
    pub internet: InternetSwitch,
    pub client: Door43Client,
    pub dir: TempDir,
    pub store: ResourceStore,
}

pub async fn rig() -> Rig {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let internet = InternetSwitch::new();
    internet.turn_on();
    let client = Door43Client::new(fake.url().parse().unwrap(), internet.clone()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path().join("resources")).unwrap();
    Rig {
        fake,
        internet,
        client,
        dir,
        store,
    }
}

pub fn repo_path(path: &str) -> RepoPath {
    format!("git.door43.org/{path}").parse().unwrap()
}

/// The pin Door43 names for a release of the seeded fake (D59).
pub async fn release(rig: &Rig, repo: &str, tag: &str) -> ResourcePin {
    pin_release(&rig.client, &repo_path(repo), &tag.parse().unwrap())
        .await
        .unwrap()
}

/// The commit the fake's repository holds at `rev`, as a pin by commit only.
pub async fn commit_pin(rig: &Rig, owner: &str, name: &str, rev: &str) -> ResourcePin {
    let sha = rig.fake.resolve(owner, name, rev).await.unwrap();
    ResourcePin::new(repo_path(&format!("{owner}/{name}")), sha.parse().unwrap())
}

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

pub fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A zip of `files`, deflated, in the given order.
pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

/// Asserts that the store holds no install and no partial one.
pub fn nothing_installed(store: &ResourceStore) {
    assert!(
        store.installed().unwrap().is_empty(),
        "nothing may be installed"
    );
    let partial = store.root().join(".partial");
    let leftovers = std::fs::read_dir(&partial).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftovers, 0, "no partial install may remain");
}
