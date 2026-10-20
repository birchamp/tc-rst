//! Production opens projects through tc-store (unit C0): `StoreChoice::OnDisk` builds the
//! store over the chosen projects folder and the app-data folder, on this computer's files.
//! Each case runs on real temporary folders, never the person's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_api::editor::SaveVerse;
use tc_api::event::SessionMode;
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
pub(super) fn files_on_disk(folder: &Path) -> BTreeMap<String, Vec<u8>> {
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

pub(super) struct OnDisk {
    pub(super) dir: TempDir,
    pub(super) projects: PathBuf,
}

impl OnDisk {
    pub(super) fn new() -> OnDisk {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        std::fs::create_dir(&projects).unwrap();
        OnDisk { dir, projects }
    }

    pub(super) fn app_data(&self) -> PathBuf {
        self.dir.path().join("app-data")
    }

    /// An app with test hooks on, its store on this computer's files.
    pub(super) async fn start(&self) -> TestApp {
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

/// A link in the projects folder to a project elsewhere is listed as unusable with its reason,
/// never hidden and never opened: tc-store reads no link as project data, and its checks of a
/// folder would be about the link. A link to a file, or to nothing, holds no project.
#[cfg(unix)]
#[tokio::test]
async fn a_link_to_a_project_elsewhere_is_listed_as_unusable() {
    use std::os::unix::fs::symlink;
    let disk = OnDisk::new();
    let elsewhere = disk.dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let target = make_project(&disk.app_data(), &elsewhere, "titus_elsewhere");
    symlink(&target, disk.projects.join("linked")).unwrap();
    let file = disk.dir.path().join("a_file.txt");
    std::fs::write(&file, b"x").unwrap();
    symlink(&file, disk.projects.join("link_to_a_file")).unwrap();
    symlink(
        disk.dir.path().join("nothing"),
        disk.projects.join("dangling"),
    )
    .unwrap();
    let t = disk.start().await;

    let store = t.app.sessions().store();
    let listing = store.list().unwrap();
    assert!(listing.projects.is_empty(), "{:?}", listing.projects);
    let unusable: Vec<&str> = listing.unusable.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(unusable, ["linked"]);
    assert_eq!(
        listing.unusable[0].reason,
        tc_app::backend::UnusableReason::Link
    );
    assert!(matches!(
        store.open(&folder("linked"), open_progress()),
        Err(tc_app::backend::OpenFailure::NotAProject)
    ));
}

/// A launch never lists fewer projects than the one before it. The default folder is copied by
/// a client, so its project opens read-only and the copy goes into the local folder, which that
/// copy makes; the next launch finds both folders and makes the local one the root. Home still
/// lists the default folder's project, and both open as they did.
#[tokio::test]
async fn a_launch_after_the_first_copy_lists_the_default_folder_too() {
    use tc_api::event::SessionMode;
    use tc_api::write::ReadOnlyReason;
    use tc_app::config::RootChoice;
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let default = home.join("translationCore4").join("projects");
    make_project(&dir.path().join("app-data"), &default, "original_a");
    // Then a Syncthing marker in the home folder: it copies what is below it.
    std::fs::create_dir_all(home.join(".stfolder")).unwrap();
    let start = || {
        TestApp::start_in(dir.path(), |_, deps| {
            deps.store = StoreChoice::OnDisk;
        })
    };
    let names = |t: &TestApp| -> Vec<String> {
        let listing = t.app.sessions().store().list().unwrap();
        listing
            .projects
            .iter()
            .map(|e| e.folder.as_str().to_owned())
            .collect()
    };

    let t = start().await;
    assert_eq!(t.app.projects_root().path, default);
    assert_eq!(names(&t), ["original_a"]);
    let open = t
        .app
        .sessions()
        .open_screen(&folder("original_a"), open_progress())
        .await
        .unwrap();
    assert_eq!(
        open.mode,
        SessionMode::ReadOnly(ReadOnlyReason::CloudSynced)
    );
    let copy = post(
        &t.app,
        &tc_api::session::CopyProject {
            at: open.at.clone(),
            request: rid(1),
        },
    )
    .await
    .unwrap();
    assert_eq!(copy.project.as_str(), "original_a - Copy");
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.sessions().settled(&folder("original_a")).await;
    t.app.quit().await;
    drop(t);

    let t = start().await;
    assert_eq!(
        t.app.projects_root().chosen,
        RootChoice::LocalBesideDefault {
            default: default.clone()
        }
    );
    assert_eq!(names(&t), ["original_a", "original_a - Copy"]);
    for (name, mode) in [
        (
            "original_a",
            SessionMode::ReadOnly(ReadOnlyReason::CloudSynced),
        ),
        ("original_a - Copy", SessionMode::Writable),
    ] {
        let open = t
            .app
            .sessions()
            .open_screen(&folder(name), open_progress())
            .await
            .unwrap();
        assert_eq!(open.mode, mode, "{name}");
        t.app.sessions().leave_screen(&open.at).unwrap();
        t.app.sessions().settled(&folder(name)).await;
    }
    t.app.quit().await;
}

/// tc-store's start removes the staging folders that an interrupted create, copy, join or
/// import left (STORAGE §6.4 N1). The app records each in the report log, so "Ask for help"
/// carries it and E1b and F1 can tell the person; a project beside them is untouched. An
/// import and a join are reported as tC4's start reports them
/// (`tC4:src/data/journal/opsLog.ts:603-614, 677-691`): under their own operation, interrupted
/// and rolled back, the import with tC4's refusal. A create, which tC4 has no staging folder
/// for, is this app's `staging-removed`.
#[tokio::test]
async fn a_staging_folder_removed_at_start_is_reported() {
    use tc_app::reports::{ReportOp, ReportOutcome};
    let disk = OnDisk::new();
    make_project(&disk.app_data(), &disk.projects, "titus_on_disk");
    let join = disk.projects.join(".staging-join-feedfacefeedface");
    std::fs::create_dir_all(join.join("ingredients")).unwrap();
    std::fs::write(join.join("ingredients/TIT.usfm"), b"\\id TIT\n").unwrap();
    let import = disk.projects.join(".staging-import-0123456789abcdef");
    std::fs::create_dir(&import).unwrap();
    let create = disk.projects.join(".staging-create-00000000deadbeef");
    std::fs::create_dir(&create).unwrap();
    let t = disk.start().await;

    let ops = [
        ReportOp::Import,
        ReportOp::OpenDoor43,
        ReportOp::StagingRemoved,
    ];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut removed = loop {
        let removed: Vec<_> = t
            .app
            .reports()
            .all()
            .into_iter()
            .filter(|r| ops.contains(&r.op))
            .collect();
        if removed.len() >= 3 || std::time::Instant::now() > deadline {
            break removed;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    removed.sort_by_key(|r| r.facts["folder"].as_str().unwrap().to_owned());
    let seen: Vec<_> = removed
        .iter()
        .map(|r| {
            (
                r.op,
                r.outcome.clone(),
                serde_json::Value::Object(r.facts.clone()),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (
                ReportOp::StagingRemoved,
                ReportOutcome::Failed {
                    reason: "creating a project did not finish, so its part copy was removed"
                        .to_owned()
                },
                serde_json::json!({
                    "folder": ".staging-create-00000000deadbeef",
                    "kind": "create",
                }),
            ),
            (
                ReportOp::Import,
                ReportOutcome::Refused {
                    code: Some("import.write-failed".to_owned()),
                    key: "refusal.import.write-failed".to_owned(),
                },
                serde_json::json!({
                    "folder": ".staging-import-0123456789abcdef",
                    "interrupted": true,
                    "rolledBack": true,
                }),
            ),
            (
                ReportOp::OpenDoor43,
                ReportOutcome::Failed {
                    reason: "the open from Door43 was interrupted; no project was kept".to_owned()
                },
                serde_json::json!({
                    "folder": ".staging-join-feedfacefeedface",
                    "interrupted": true,
                    "rolledBack": true,
                }),
            ),
        ]
    );
    assert!(!join.exists() && !import.exists() && !create.exists());
    let listing = t.app.sessions().store().list().unwrap();
    let names: Vec<&str> = listing.projects.iter().map(|e| e.folder.as_str()).collect();
    assert_eq!(names, ["titus_on_disk"]);
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

/// A project the open from Door43 adds must not land on anything of the projects folder
/// (APP-DESIGN B9b D5), as tC4 checks `listLocalRepos()`: the translator's notes folder, an
/// empty folder and a plain file are refused as a project is, without letter case, though Home
/// lists none of them.
#[tokio::test]
async fn the_open_refuses_a_name_any_entry_of_the_projects_folder_has() {
    use tc_api::door43::{OpenDoor43, ProjectName, RepoSource};
    use tc_api::refusal::OpenDoor43Refusal;
    use tc_api::{ApiError, Refusal};
    let disk = OnDisk::new();
    make_project(&disk.app_data(), &disk.projects, "titus_on_disk");
    std::fs::create_dir(disk.projects.join("Notes")).unwrap();
    std::fs::write(disk.projects.join("Notes/my-notes.txt"), b"mine").unwrap();
    std::fs::create_dir(disk.projects.join("empty")).unwrap();
    std::fs::write(disk.projects.join("afile"), b"mine").unwrap();
    let t = disk.start().await;
    post(
        &t.app,
        &tc_api::net::Set {
            request: rid(1),
            on: true,
        },
    )
    .await
    .unwrap();

    for (id, name) in [
        (2, "notes"),
        (3, "empty"),
        (4, "afile"),
        (5, "TITUS_ON_DISK"),
    ] {
        let open = OpenDoor43 {
            request: rid(id),
            repo: RepoSource::new("jdoe", "es-419_tit_text_reg").unwrap(),
            name: ProjectName::parse(name).unwrap(),
        };
        assert_eq!(
            post(&t.app, &open).await,
            Err(ApiError::Refused(Refusal::OpenDoor43(
                OpenDoor43Refusal::NameExists
            ))),
            "{name}"
        );
    }
    assert!(t.app.jobs().view(tc_api::ids::JobId::new(1)).is_none());
    // The dialog checks the same names before Open is pressed.
    let home = post(&t.app, &tc_api::home::List {}).await.unwrap();
    assert_eq!(home.others, ["Notes", "afile", "empty"]);
}
