//! Production opens projects through tc-store (unit C0): `StoreChoice::OnDisk` builds the
//! store over the chosen projects folder and the app-data folder, on this computer's files.
//! Each case runs on real temporary folders, never the person's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_api::event::SessionMode;
use tc_api::test::SaveVerse;
use tc_api::write::Stored;
use tc_app::StoreChoice;
use tc_app::config::ProjectsFolder;
use tc_app::wire::token::shown;
use tc_journal::BookId;
use tc_journal::fold::ProjectKind;
use tempfile::TempDir;

use super::common::{
    Parts, TestApp, folder, hello_value, open_progress, post, repo_root, rid, tit,
};
use crate::store_world::{assert_valid_burrito_files, sample_records, words_of};

/// Makes the project `name` in `projects` with tc-store on this computer's files: the sample's
/// Titus as records, as the store tests make it.
fn make_project(app_data: &Path, projects: &Path, name: &str) -> PathBuf {
    let store = tc_store::Store::start(tc_store::StoreConfig {
        app_data: app_data.to_owned(),
        projects: projects.to_owned(),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(tc_store::StdPlatform),
        clock: Arc::new(tc_store::SystemClock),
        threads: 2,
    })
    .unwrap();
    store.wait_cleanup();
    let titus = std::fs::read(repo_root().join("fixtures/tc4/sample-burrito/ingredients/TIT.usfm"))
        .unwrap();
    store
        .create(&tc_store::NewProject {
            name: name.to_owned(),
            kind: ProjectKind::Bible,
            records: sample_records(&BTreeMap::from([(BookId::Titus, titus)]), None),
            books: Vec::new(),
        })
        .unwrap()
}

/// Every file under `folder` on disk, by its path relative to it.
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

struct OnDisk {
    dir: TempDir,
    projects: PathBuf,
}

impl OnDisk {
    fn new() -> OnDisk {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        std::fs::create_dir(&projects).unwrap();
        OnDisk { dir, projects }
    }

    fn app_data(&self) -> PathBuf {
        self.dir.path().join("app-data")
    }

    /// An app with test hooks on, its store on this computer's files.
    async fn start(&self) -> TestApp {
        let projects = self.projects.clone();
        TestApp::start_with(self.dir.path(), &Parts::new(), move |config, deps| {
            config.projects = ProjectsFolder::Given(projects);
            deps.store = StoreChoice::OnDisk;
        })
        .await
    }
}

#[tokio::test]
async fn production_store_lists_and_opens_projects_on_disk() {
    let disk = OnDisk::new();
    make_project(&disk.app_data(), &disk.projects, "titus_on_disk");
    let t = disk.start().await;

    let listing = t.app.sessions().store().list().unwrap();
    let names: Vec<&str> = listing.projects.iter().map(|e| e.folder.as_str()).collect();
    assert_eq!(names, ["titus_on_disk"]);
    assert_eq!(listing.projects[0].kind, ProjectKind::Bible);
    assert!(listing.unusable.is_empty());

    let open = t
        .app
        .sessions()
        .open_screen(&folder("titus_on_disk"), open_progress())
        .await
        .unwrap();
    assert_eq!(open.mode, SessionMode::Writable);
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.sessions().settled(&folder("titus_on_disk")).await;
    assert!(t.app.sessions().open_sessions().is_empty());
}

#[tokio::test]
async fn a_save_through_the_app_survives_a_restart() {
    let disk = OnDisk::new();
    let project = make_project(&disk.app_data(), &disk.projects, "titus_on_disk");
    let key = tit(1, 2);
    {
        let t = disk.start().await;
        let open = t
            .app
            .sessions()
            .open_screen(&folder("titus_on_disk"), open_progress())
            .await
            .unwrap();
        let session = t.app.sessions().reader(&open.at).unwrap();
        let minted = session.read().mint(&key, None).unwrap();
        let ack = post(
            &t.app,
            &SaveVerse {
                at: open.at.clone(),
                request: rid(1),
                shown: shown(&minted).unwrap(),
                words: "kept through a restart".to_owned(),
            },
        )
        .await
        .unwrap();
        assert_eq!(ack.stored, Stored::Written);
        t.app.quit().await;
    }
    let t = disk.start().await;
    let open = t
        .app
        .sessions()
        .open_screen(&folder("titus_on_disk"), open_progress())
        .await
        .unwrap();
    let session = t.app.sessions().reader(&open.at).unwrap();
    assert_eq!(
        words_of(&session.handle().snapshot(), &key).as_deref(),
        Some("kept through a restart")
    );
    t.app.quit().await;
    assert_valid_burrito_files(
        &files_on_disk(&project)
            .into_iter()
            .filter(|(rel, _)| !rel.starts_with(".tc/"))
            .collect(),
    );
}

#[tokio::test]
async fn the_app_starts_when_the_projects_folder_holds_no_project() {
    let disk = OnDisk::new();
    let t = disk.start().await;
    let listing = t.app.sessions().store().list().unwrap();
    assert!(listing.projects.is_empty() && listing.unusable.is_empty());
    let hello = hello_value(&t.app).await;
    assert!(hello["sessions"].as_array().unwrap().is_empty());
    assert!(
        disk.app_data().join("installation.json").is_file(),
        "the store keeps its installation secret in app data"
    );
}
