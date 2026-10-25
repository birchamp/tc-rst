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
pub(super) fn make_project(app_data: &Path, projects: &Path, name: &str) -> PathBuf {
    let store = tc_store::Store::start(tc_store::StoreConfig {
        app_data: app_data.to_owned(),
        projects: projects.to_owned(),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(tc_store::StdPlatform),
        clock: Arc::new(tc_store::SystemClock),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    let titus = std::fs::read(repo_root().join("fixtures/tc4/sample-burrito/ingredients/TIT.usfm"))
        .unwrap();
    let folder = store
        .create(&tc_store::NewProject {
            name: name.to_owned(),
            kind: ProjectKind::Bible,
            records: sample_records(&BTreeMap::from([(BookId::Titus, titus)]), None),
            books: Vec::new(),
        })
        .unwrap();
    // P5/INV-VALID-BURRITO on the real filesystem, which no FaultFs watch sees.
    assert_valid_burrito_files(&files_on_disk(&folder));
    folder
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
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

/// D1 step 3: a book file another program changed is asked about once, kept, and listed until
/// it is imported or dismissed (owner ruling OE 3; delta §2.3, §2.11), on the store world.
mod outside {
    use std::path::PathBuf;

    use tc_api::ids::{RequestId, Scope};
    use tc_api::session::{Focus, ImportKept, KeptWhy, NotNow, OutsideEdits};
    use tc_journal::BookId;

    use super::super::checkpoints::store_app;
    use super::super::common::{TestApp, open_progress, post};
    use crate::store_world::{StoreWorld, save, store_world, tit, words_of};

    const TIT: &str = "ingredients/TIT.usfm";
    const JON: &str = "ingredients/JON.usfm";

    fn request(n: u8) -> RequestId {
        RequestId::from_bytes([n; 16])
    }

    fn path(world: &StoreWorld, file: &str) -> PathBuf {
        world.folder(&world.bible).join(file)
    }

    /// `bytes` with the text of verse c:v replaced.
    fn set_verse(bytes: &[u8], c: u16, v: u16, text: &str) -> Vec<u8> {
        let s = String::from_utf8(bytes.to_vec()).unwrap();
        let chapter = s.find(&format!("\\c {c}")).unwrap();
        let marker = format!("\\v {v} ");
        let at = chapter + s[chapter..].find(&marker).unwrap() + marker.len();
        let end = at + s[at..].find('\n').unwrap();
        format!("{}{}{}", &s[..at], text, &s[end..]).into_bytes()
    }

    /// Another program saves verse c:v of `file`; the bytes it wrote.
    fn edit(world: &StoreWorld, file: &str, c: u16, v: u16, text: &str) -> Vec<u8> {
        let written = world.fs.peek(&path(world, file)).unwrap();
        let edited = set_verse(&written, c, v, text);
        world.fs.put(&path(world, file), &edited, None);
        edited
    }

    async fn opened(world: &StoreWorld) -> (tempfile::TempDir, TestApp, Scope) {
        let (dir, t) = store_app(world).await;
        let open = t
            .app
            .sessions()
            .open_screen(&world.bible, open_progress())
            .await
            .unwrap();
        t.app.read(&open.at).unwrap().handle().wait_idle();
        (dir, t, open.at)
    }

    async fn focus(t: &TestApp, at: &Scope, n: u8) -> OutsideEdits {
        post(
            &t.app,
            &Focus {
                request: request(n),
                at: at.clone(),
            },
        )
        .await
        .unwrap()
    }

    /// What `hello` lists for the open project.
    async fn hello_outside(t: &TestApp, at: &Scope) -> OutsideEdits {
        post(&t.app, &tc_api::app::Hello {})
            .await
            .unwrap()
            .sessions
            .into_iter()
            .find(|s| s.at == *at)
            .unwrap()
            .outside
    }

    fn words(t: &TestApp, at: &Scope, v: u16) -> Option<String> {
        let session = t.app.read(at).unwrap();
        words_of(&session.handle().snapshot(), &tit(1, v))
    }

    /// INV-A-CHANGED-BOOK-IS-ASKED-NOT-TAKEN: two books changed together are one question that
    /// names both, and nothing of them is in the project yet; a reload asks it again.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_changed_books_are_asked_in_one_question() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        let before = words(&t, &at, 1);
        edit(
            &world,
            TIT,
            1,
            1,
            "Paul, a servant of God, from another program",
        );
        edit(&world, JON, 1, 1, "The word came, from another program");
        let outside = focus(&t, &at, 1).await;
        let asked: Vec<_> = outside
            .waiting
            .iter()
            .map(|w| (w.file.as_str(), w.book, w.added))
            .collect();
        assert_eq!(
            asked,
            [
                (JON, Some(BookId::Jonah), false),
                (TIT, Some(BookId::Titus), false)
            ],
            "{outside:?}"
        );
        assert_eq!(words(&t, &at, 1), before, "nothing enters before Import");
        assert_eq!(hello_outside(&t, &at).await, outside, "a reload asks again");
    }

    /// *Not now* keeps the project's text, writes the file from it, and lists the copy in the
    /// notices, where it has *Import* (OE 3.4).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn not_now_keeps_the_text_and_lists_the_copy_with_import() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        let before = words(&t, &at, 1);
        let written = world.fs.peek(&path(&world, TIT)).unwrap();
        edit(&world, TIT, 1, 1, "Paul, from another program");
        let copy = focus(&t, &at, 1).await.waiting[0].copy.clone();
        let outside = post(
            &t.app,
            &NotNow {
                request: request(2),
                at: at.clone(),
                file: TIT.into(),
            },
        )
        .await
        .unwrap();
        assert!(outside.waiting.is_empty(), "{outside:?}");
        assert_eq!(words(&t, &at, 1), before);
        t.app.read(&at).unwrap().handle().wait_idle();
        assert_eq!(world.fs.peek(&path(&world, TIT)), Some(written));
        assert_eq!(outside.kept.len(), 1, "{outside:?}");
        assert_eq!(outside.kept[0].copy, copy);
        assert_eq!(outside.kept[0].file, TIT);
        assert_eq!(outside.kept[0].why, KeptWhy::Changed, "listed with Import");
        // Its Import still takes the copy in.
        let report = post(
            &t.app,
            &ImportKept {
                request: request(3),
                at: at.clone(),
                copy,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.imported, 1, "{report:?}");
        assert_eq!(
            words(&t, &at, 1).as_deref(),
            Some("Paul, from another program")
        );
    }

    /// The question's *Import* reports what came in and what is to review, and the changes to
    /// review are conflicts the review's Text tab lists.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn import_from_the_question_reports_counts_and_links_the_review() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        edit(&world, TIT, 1, 1, "Paul, from another program");
        edit(&world, TIT, 1, 2, "in hope, from another program");
        let waiting = focus(&t, &at, 1).await.waiting;
        let session = t.app.read(&at).unwrap();
        save(session.handle().as_ref(), &tit(1, 2), "in hope, typed here")
            .await
            .unwrap();
        let report = post(
            &t.app,
            &ImportKept {
                request: request(2),
                at: at.clone(),
                copy: waiting[0].copy.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(report.file, TIT);
        assert_eq!(report.book, Some(BookId::Titus));
        assert_eq!((report.imported, report.to_review), (1, 1), "{report:?}");
        assert!(report.outside.waiting.is_empty());
        assert_eq!(
            words(&t, &at, 1).as_deref(),
            Some("Paul, from another program")
        );
        let review = post(
            &t.app,
            &tc_api::conflicts::List {
                at: at.clone(),
                tab: tc_api::conflicts::ReviewTab::Text,
            },
        )
        .await
        .unwrap();
        let text = review
            .tabs
            .iter()
            .find(|c| c.tab == tc_api::conflicts::ReviewTab::Text)
            .unwrap();
        assert_eq!(text.count, report.to_review, "the link's review lists them");
    }

    /// INV-A-KEPT-COPY-STAYS-ON-A-SCREEN, review R2: a second outside save keeps the first copy
    /// listed beside the newest, which the question imports.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_outside_save_keeps_the_first_copy_in_the_notices() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        edit(&world, TIT, 1, 1, "Paul, the first save");
        let first = focus(&t, &at, 1).await;
        world.fs.advance(std::time::Duration::from_secs(5));
        edit(&world, TIT, 1, 2, "in hope, the second save");
        let second = focus(&t, &at, 2).await;
        assert_eq!(second.waiting.len(), 1, "one question names the book once");
        assert_eq!(second.kept.len(), 2, "{second:?}");
        assert_eq!(second.kept[0].copy, first.waiting[0].copy);
        assert_eq!(second.waiting[0].copy, second.kept[1].copy, "the newest");
        assert_eq!(hello_outside(&t, &at).await.kept, second.kept);
    }

    /// A reload or a `Resync` reads `hello` again: the copy given *Not now* is still listed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_kept_copy_shows_again_after_a_resync() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        edit(&world, TIT, 1, 1, "Paul, from another program");
        focus(&t, &at, 1).await;
        let outside = post(
            &t.app,
            &NotNow {
                request: request(2),
                at: at.clone(),
                file: TIT.into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(outside.kept.len(), 1);
        let again = hello_outside(&t, &at).await;
        assert_eq!(again.kept, outside.kept);
        assert!(again.waiting.is_empty());
    }

    /// The world again over `fs`: the same projects, a new store on those files.
    fn world_over(world: &StoreWorld, fs: tc_store::fault::FaultFs) -> StoreWorld {
        use crate::store_world::{APP, backend_over};
        let hooks = std::sync::Arc::new(tc_store::Hooks::default());
        let (backend, watch) =
            backend_over(&fs, APP, tc_store::FakePlatform::default(), hooks.clone());
        StoreWorld {
            fs,
            hooks,
            backend: std::sync::Arc::new(backend),
            watch,
            bible: world.bible.clone(),
            conflicted: world.conflicted.clone(),
            other: world.other.clone(),
            sources: world.sources.clone(),
        }
    }

    /// H22 part 1: a save the project's records lost (its pending file deleted before a
    /// crash), still in the book file the app wrote, is listed as lost work, asked nothing and
    /// not taken in; *Bring it back* is the copy's import, with the same report.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_lost_work_copy_offers_bring_it_back() {
        const SAVED: &str = "saved before the file was deleted";
        let world = store_world();
        let folder = world.folder(&world.bible);
        // The store itself, so a mode leave writes the book file and seals nothing.
        let handle = world
            .backend
            .store()
            .open(&folder, &tc_store::OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let before = crate::store_world::pending_files(&world.fs, &folder);
        let shown = handle.mint(&tit(1, 1), None).unwrap();
        handle
            .submit(tc_journal::command::Command::SaveVerse {
                shown,
                words: SAVED.to_owned(),
            })
            .wait()
            .unwrap();
        handle.leave(tc_store::Leave::Mode).wait().unwrap();
        let pending: Vec<_> = crate::store_world::pending_files(&world.fs, &folder)
            .into_iter()
            .filter(|p| !before.contains(p))
            .collect();
        assert_eq!(pending.len(), 1, "{pending:?}");
        world.fs.delete(&folder.join(&pending[0])).unwrap();
        assert!(
            String::from_utf8_lossy(&world.fs.peek(&path(&world, TIT)).unwrap()).contains(SAVED)
        );
        let world = world_over(&world, world.fs.crash(false));
        drop(handle);
        let (_dir, t, at) = opened(&world).await;
        let outside = hello_outside(&t, &at).await;
        assert!(outside.waiting.is_empty(), "nothing is asked: {outside:?}");
        assert_eq!(outside.kept.len(), 1, "{outside:?}");
        assert_eq!(outside.kept[0].why, KeptWhy::Lost);
        assert_eq!(outside.kept[0].file, TIT);
        assert_ne!(
            words(&t, &at, 1).as_deref(),
            Some(SAVED),
            "nothing comes back on its own"
        );
        let report = post(
            &t.app,
            &ImportKept {
                request: request(1),
                at: at.clone(),
                copy: outside.kept[0].copy.clone(),
            },
        )
        .await
        .unwrap();
        // On this sample the lost save comes back as a version to review beside the
        // project's text (the store's plan against the copy's base), never lost and never
        // taken in unasked.
        assert_eq!(report.imported + report.to_review, 1, "{report:?}");
        let review = post(
            &t.app,
            &tc_api::conflicts::List {
                at: at.clone(),
                tab: tc_api::conflicts::ReviewTab::Text,
            },
        )
        .await
        .unwrap();
        let text = review
            .tabs
            .iter()
            .find(|c| c.tab == tc_api::conflicts::ReviewTab::Text)
            .unwrap();
        assert_eq!(text.count, report.to_review, "the report's review lists it");
        assert!(report.outside.kept.is_empty());
        // A second *Bring it back* of the same copy is refused, never a 0/0 report.
        let again = post(
            &t.app,
            &ImportKept {
                request: request(2),
                at: at.clone(),
                copy: outside.kept[0].copy.clone(),
            },
        )
        .await;
        assert_eq!(
            again,
            Err(tc_api::ApiError::NotSaved(tc_api::NotSaved::Refused(
                tc_api::write::SaveRefusal::NotKept
            )))
        );
    }

    /// H22 part 4: a project opened read-only keeps no copy and asks nothing; the review's
    /// notices name the changed book.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_read_only_open_asks_nothing_and_shows_a_notice() {
        let world = store_world();
        edit(&world, TIT, 1, 2, "in hope, from another program");
        world.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
        let (_dir, t, at) = opened(&world).await;
        let outside = hello_outside(&t, &at).await;
        world.fs.fail_writes(None);
        assert_eq!(outside, OutsideEdits::default());
        let review = post(
            &t.app,
            &tc_api::conflicts::List {
                at: at.clone(),
                tab: tc_api::conflicts::ReviewTab::Notices,
            },
        )
        .await
        .unwrap();
        let tc_api::conflicts::ReviewEntries::Notices(notices) = review.entries else {
            panic!("{:?}", review.entries)
        };
        assert!(
            notices.iter().any(|n| n.notice
                == tc_api::conflicts::NoticeView::ChangedWhileReadOnly { file: TIT.into() }),
            "{notices:?}"
        );
    }

    /// Two outside edits of the header, the first given *Not now* and the second imported:
    /// the first's *Import* is a header conflict, and neither version reads empty in the
    /// review (the header's lines, as the file holds them).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_header_conflict_shows_both_versions() {
        let world = store_world();
        let (_dir, t, at) = opened(&world).await;
        let written = String::from_utf8(world.fs.peek(&path(&world, TIT)).unwrap()).unwrap();
        let header = |h: &str| written.replacen("\\h Tito", &format!("\\h {h}"), 1);
        world
            .fs
            .put(&path(&world, TIT), header("Tito A").as_bytes(), None);
        let first = focus(&t, &at, 1).await.waiting[0].copy.clone();
        post(
            &t.app,
            &NotNow {
                request: request(2),
                at: at.clone(),
                file: TIT.into(),
            },
        )
        .await
        .unwrap();
        t.app.read(&at).unwrap().handle().wait_idle();
        world.fs.advance(std::time::Duration::from_secs(5));
        world
            .fs
            .put(&path(&world, TIT), header("Tito B").as_bytes(), None);
        let second = focus(&t, &at, 3).await.waiting[0].copy.clone();
        for (n, copy) in [(4, second), (5, first)] {
            post(
                &t.app,
                &ImportKept {
                    request: request(n),
                    at: at.clone(),
                    copy,
                },
            )
            .await
            .unwrap();
        }
        let review = post(
            &t.app,
            &tc_api::conflicts::List {
                at: at.clone(),
                tab: tc_api::conflicts::ReviewTab::Text,
            },
        )
        .await
        .unwrap();
        let tc_api::conflicts::ReviewEntries::Conflicts(entries) = review.entries else {
            panic!("{:?}", review.entries)
        };
        let headers: Vec<_> = entries
            .iter()
            .filter(|e| format!("{:?}", e.record).contains("bookHeader"))
            .collect();
        assert!(!headers.is_empty(), "{entries:?}");
        for version in headers.iter().flat_map(|e| &e.versions) {
            assert!(
                matches!(&version.value, tc_api::conflicts::VersionValue::Text(text) if !text.is_empty()),
                "{version:?}"
            );
        }
    }
}
