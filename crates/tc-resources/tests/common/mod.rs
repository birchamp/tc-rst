// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]
// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

pub mod aquifer_fake;
pub mod bundled;

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
        store.installed().unwrap().installs.is_empty(),
        "nothing may be installed"
    );
    let partial = store.root().join(".partial");
    let leftovers = std::fs::read_dir(&partial).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftovers, 0, "no partial install may remain");
}

pub const AQUIFER_TAG: &str = "v2026-10-12";

/// The commit each fixture repository's tag names (fixtures/aquifer/README.md).
pub fn aquifer_commit(repo: &str) -> &'static str {
    match repo {
        "UWTranslationNotes" => "dbfb46e027ee0807563918f4b95c80879cc32c60",
        "UWTranslationQuestions" => "f8cc284556dbf4b1bfbd5d8c7bdd7d41ee8775e9",
        "UWTranslationWords" => "f8ebe522a769f0f5311a5915d1d61f9e28e6b617",
        "UWTranslationManual" => "b993c3bc252262eca6162f5cea2a019b2e069dc4",
        other => panic!("no fixture for {other}"),
    }
}

pub fn asset_name(language: &str) -> String {
    match language {
        "eng" => "English-json.zip".to_owned(),
        "por" => "Portuguese-json.zip".to_owned(),
        other => panic!("no asset name for {other}"),
    }
}

/// A release asset built from the fixture files, laid out as Aquifer's
/// `<Language>-json.zip` is: `metadata.json` and `json/` at the root.
pub fn aquifer_zip(repo: &str, language: &str) -> Vec<u8> {
    aquifer_zip_with(repo, language, &[])
}

/// [`aquifer_zip`] with more entries after the fixture's own.
pub fn aquifer_zip_with(repo: &str, language: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let dir = fixtures().join("aquifer").join(repo).join(language);
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in std::fs::read_dir(dir.join("json")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push((format!("json/{name}"), std::fs::read(entry.path()).unwrap()));
    }
    entries.sort();
    entries.push((
        "metadata.json".to_owned(),
        std::fs::read(dir.join("metadata.json")).unwrap(),
    ));
    entries.extend(
        extra
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec())),
    );
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    zip(&refs)
}

pub fn aquifer_pin(repo: &str, language: &str, bytes: &[u8]) -> tc_resources::AquiferPin {
    tc_resources::AquiferPin {
        repo: tc_resources::AquiferRepo::bible_aquifer(repo).unwrap(),
        tag: AQUIFER_TAG.parse().unwrap(),
        commit: aquifer_commit(repo).parse().unwrap(),
        language: language.parse().unwrap(),
        asset: asset_name(language).parse().unwrap(),
        sha256: tc_resources::Sha256::of(bytes),
    }
}

/// A fake GitHub holding the four fixture repositories' `v2026-10-12` releases in
/// English and Portuguese, a client for it with the internet on, and an empty store.
pub struct AquiferRig {
    pub github: aquifer_fake::FakeGithub,
    pub internet: InternetSwitch,
    pub client: tc_resources::AquiferClient,
    pub dir: TempDir,
    pub store: ResourceStore,
}

pub const AQUIFER_REPOS: [&str; 4] = [
    "UWTranslationNotes",
    "UWTranslationQuestions",
    "UWTranslationWords",
    "UWTranslationManual",
];

pub async fn aquifer_rig() -> AquiferRig {
    let github = aquifer_fake::FakeGithub::start().await;
    for repo in AQUIFER_REPOS {
        github.add_release(aquifer_fake::FakeRelease {
            owner: "BibleAquifer".to_owned(),
            repo: repo.to_owned(),
            tag: AQUIFER_TAG.to_owned(),
            commit: aquifer_commit(repo).to_owned(),
            published_at: "2026-10-12T00:00:00Z".to_owned(),
            assets: ["eng", "por"]
                .into_iter()
                .map(|lang| aquifer_fake::FakeAsset {
                    name: asset_name(lang),
                    bytes: aquifer_zip(repo, lang),
                    listed_digest: true,
                })
                .collect(),
        });
    }
    let internet = InternetSwitch::new();
    internet.turn_on();
    let client = tc_resources::AquiferClient::with_servers(
        github.url(),
        github.url(),
        internet.clone(),
        tc_resources::AquiferLimits::default(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path().join("resources")).unwrap();
    AquiferRig {
        github,
        internet,
        client,
        dir,
        store,
    }
}

/// The DCS burrito export of a fixture folder under `fixtures/tc4/test-fixtures/resources`,
/// rebuilt as Door43 serves it: `<repo>/metadata.json` and each fixture file at its
/// export path. The fixture files are byte-identical to the export's, so they match the
/// checksums the unmodified metadata lists.
pub fn fixture_export(dir: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let base = format!("tc4/test-fixtures/resources/{dir}");
    let repo = dir.split('@').next().unwrap();
    let mut entries = vec![(
        format!("{repo}/metadata.json"),
        fixture(&format!("{base}/metadata.json")),
    )];
    for (file, path) in files {
        entries.push((format!("{repo}/{path}"), fixture(&format!("{base}/{file}"))));
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    zip(&refs)
}

/// The pin of a fixture export: its declared revision, under the registry owner.
pub fn fixture_pin(dir: &str, repo: &str, tag: &str) -> ResourcePin {
    let metadata: serde_json::Value = serde_json::from_slice(&fixture(&format!(
        "tc4/test-fixtures/resources/{dir}/metadata.json"
    )))
    .unwrap();
    let dcs = metadata["identification"]["primary"]["dcs"]
        .as_object()
        .unwrap();
    let revision = dcs.values().next().unwrap()["revision"].as_str().unwrap();
    ResourcePin::new(repo_path(repo), revision.parse().unwrap()).with_version(tag.parse().unwrap())
}

/// Installs a fixture export through the sideload path, offline.
pub fn install_fixture(
    store: &ResourceStore,
    dir: &str,
    repo: &str,
    tag: &str,
    files: &[(&str, &str)],
) -> ResourcePin {
    let pin = fixture_pin(dir, repo, tag);
    tc_resources::install_dcs_zip(store, &pin, &fixture_export(dir, files), 1 << 30).unwrap();
    pin
}
