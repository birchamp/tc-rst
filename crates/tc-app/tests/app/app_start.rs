//! Starting the app: the projects folder of Q-4, the app-data folder, and the keychain read.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::Value;
use tc_api::account::{AccountView, KeepState};
use tc_app::App;
use tc_app::StartError;
use tc_app::account::{MemKeychain, MemKeychainMode};
use tc_app::config::{ConfigError, HostEnv, HostOs, ProjectsFolder, RootChoice};
use tc_app::reports::{OperationReport, ReportOp, ReportOutcome};

use super::common::{CountingKeychain, Parts, TestApp, hello_value, token, try_start};

fn reports_of(app: &App, op: ReportOp) -> Vec<OperationReport> {
    app.reports()
        .all()
        .into_iter()
        .filter(|r| r.op == op)
        .collect()
}

fn projects_below(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("translationCore4").join("projects")
}

#[tokio::test]
async fn the_first_launch_makes_the_default_projects_folder() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let root = t.app.projects_root();
    assert_eq!(root.path, projects_below(&t.home));
    assert!(root.path.is_dir());
    assert_eq!(root.chosen, RootChoice::FirstLaunch);
    let reports = reports_of(&t.app, ReportOp::ProjectsFolder);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].outcome, ReportOutcome::Ok);
    assert_eq!(reports[0].facts["choice"], "firstLaunch");
    assert_eq!(reports[0].facts["syncedBy"], Value::Null);
    assert_eq!(
        reports[0].facts["path"],
        root.path.display().to_string().as_str()
    );
    assert!(
        !projects_below(&t.local).exists(),
        "the local folder was not made"
    );
}

#[tokio::test]
async fn the_first_launch_uses_the_local_folder_when_the_default_is_synced() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    // A Syncthing marker in the home folder: it copies what is below it.
    std::fs::create_dir(dir.path().join("home").join(".stfolder")).unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let root = t.app.projects_root();
    assert_eq!(root.path, projects_below(&t.local));
    assert!(root.path.is_dir());
    assert!(
        !projects_below(&t.home).exists(),
        "the synced folder was not made"
    );
    assert!(matches!(
        &root.chosen,
        RootChoice::FirstLaunchLocal { default_synced_by } if default_synced_by == "Syncthing"
    ));
    let reports = reports_of(&t.app, ReportOp::ProjectsFolder);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].facts["choice"], "firstLaunchLocal");
    assert_eq!(reports[0].facts["syncedBy"], "Syncthing");
    assert_eq!(
        reports[0].facts["path"],
        root.path.display().to_string().as_str()
    );
}

#[tokio::test]
async fn when_both_folders_are_synced_the_default_is_made_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    for below in ["home", "local"] {
        std::fs::create_dir_all(dir.path().join(below).join(".stfolder")).unwrap();
    }
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    assert_eq!(t.app.projects_root().path, projects_below(&t.home));
    assert!(t.app.projects_root().path.is_dir());
    let reports = reports_of(&t.app, ReportOp::ProjectsFolder);
    assert_eq!(reports[0].facts["choice"], "firstLaunchSynced");
}

#[tokio::test]
async fn a_second_launch_uses_the_folder_the_first_made() {
    for synced in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        if synced {
            std::fs::create_dir_all(dir.path().join("home").join(".stfolder")).unwrap();
        }
        let first = TestApp::start_in(dir.path(), |_, _| {}).await;
        let path = first.app.projects_root().path.clone();
        let second = TestApp::start_in(dir.path(), |_, _| {}).await;
        assert_eq!(second.app.projects_root().path, path, "synced: {synced}");
        assert_eq!(
            second.app.projects_root().chosen,
            if synced {
                RootChoice::ExistingLocal
            } else {
                RootChoice::Existing
            }
        );
        assert!(
            reports_of(&second.app, ReportOp::ProjectsFolder).is_empty(),
            "only a first launch is reported"
        );
    }
}

/// A launch never lists fewer projects than the one before it did (AGENTS.md: never hide a
/// translator's work). The first launch chose the local folder because a client copies the
/// default; if the default folder appears later, by hand or by sync, the local folder is still
/// the one this app made and filled.
#[tokio::test]
async fn a_default_folder_that_appears_later_does_not_replace_the_local_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("home").join(".stfolder")).unwrap();
    let first = TestApp::start_in(dir.path(), |_, _| {}).await;
    let local = first.app.projects_root().path.clone();
    std::fs::create_dir_all(local.join("kept_project")).unwrap();
    // Someone makes the default folder by hand.
    let default = projects_below(&first.home);
    std::fs::create_dir_all(&default).unwrap();

    let second = TestApp::start_in(dir.path(), |_, _| {}).await;
    let root = second.app.projects_root();
    assert_eq!(root.path, local, "the folder that holds the projects");
    assert_eq!(
        root.chosen,
        RootChoice::LocalBesideDefault {
            default: default.clone()
        }
    );
    assert!(!root.chosen.is_first_launch());
    assert!(
        local.join("kept_project").is_dir(),
        "nothing was moved or removed"
    );
    assert_eq!(
        std::fs::read_dir(&default).unwrap().count(),
        0,
        "the default folder was not touched"
    );

    // The report says where the projects are and which folder is not listed, so "Ask for
    // help" explains a person who looks in the other one.
    let reports = reports_of(&second.app, ReportOp::ProjectsFolder);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].outcome, ReportOutcome::Ok);
    assert_eq!(reports[0].facts["choice"], "localBesideDefault");
    assert_eq!(
        reports[0].facts["path"],
        local.display().to_string().as_str()
    );
    assert_eq!(
        reports[0].facts["notListed"],
        default.display().to_string().as_str()
    );
}

#[tokio::test]
async fn a_given_projects_folder_is_used_and_nothing_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let given = dir.path().join("my projects");
    std::fs::create_dir_all(&given).unwrap();
    let t = TestApp::start_in(dir.path(), |config, _| {
        config.projects = ProjectsFolder::Given(given.clone());
    })
    .await;
    assert_eq!(t.app.projects_root().path, given);
    assert_eq!(t.app.projects_root().chosen, RootChoice::Given);
    assert!(!projects_below(&t.home).exists());
    assert!(!projects_below(&t.local).exists());
    assert!(reports_of(&t.app, ReportOp::ProjectsFolder).is_empty());
}

#[tokio::test]
async fn a_given_projects_folder_that_does_not_exist_is_a_start_error() {
    let dir = tempfile::tempdir().unwrap();
    let given = dir.path().join("typo");
    let parts = Parts::new();
    let result = try_start(dir.path(), &parts, |config, _| {
        config.projects = ProjectsFolder::Given(given.clone());
    })
    .await;
    match result {
        Err(StartError::ProjectsFolder { path, reason }) => {
            assert_eq!(path, given);
            assert!(!reason.is_empty());
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
    assert!(!given.exists(), "an empty folder was not made in its place");
}

#[tokio::test]
async fn no_home_is_a_start_error() {
    let dir = tempfile::tempdir().unwrap();
    let result = try_start(dir.path(), &Parts::new(), |_, deps| {
        deps.host = HostEnv::new(HostOs::Windows, []);
    })
    .await;
    assert!(
        matches!(
            result,
            Err(StartError::Config(ConfigError::NoHome {
                variable: "USERPROFILE"
            }))
        ),
        "{:?}",
        result.map(|_| ())
    );
}

#[tokio::test]
async fn an_app_data_folder_that_cannot_be_made_is_a_start_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a file");
    std::fs::write(&file, b"x").unwrap();
    let result = try_start(dir.path(), &Parts::new(), |config, _| {
        config.app_data = file.join("inside");
    })
    .await;
    assert!(
        matches!(&result, Err(StartError::AppData { path, .. }) if *path == file.join("inside")),
        "{:?}",
        result.map(|_| ())
    );
}

async fn account_of(app: &App) -> AccountView {
    app.request(&tc_api::account::State {})
        .await
        .unwrap()
        .into_result()
        .unwrap()
}

#[tokio::test]
async fn account_state_has_no_user_and_no_notice_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    assert_eq!(
        account_of(&t.app).await,
        AccountView {
            user: None,
            kept: KeepState::NotKept,
            notice: None
        }
    );
}

#[tokio::test]
async fn a_kept_token_reads_as_kept_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    parts.keychain.seed(token("secret-token-1"));
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    assert_eq!(account_of(&t.app).await.kept, KeepState::Kept);
    assert!(reports_of(&t.app, ReportOp::Keychain).is_empty());
}

#[tokio::test]
async fn the_keychain_is_read_once_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let counting = Arc::new(CountingKeychain::new(MemKeychain::holding(token("t0ken"))));
    let t = TestApp::start_in(dir.path(), |_, deps| deps.keychain = counting.clone()).await;
    assert_eq!(counting.reads.load(Ordering::SeqCst), 1);
    for _ in 0..5 {
        account_of(&t.app).await;
        hello_value(&t.app).await;
    }
    assert_eq!(
        counting.reads.load(Ordering::SeqCst),
        1,
        "an operating-system keychain can prompt the person"
    );
}

#[tokio::test]
async fn an_unreadable_kept_token_is_reported_and_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = MemKeychain::with_mode(MemKeychainMode::Unreadable);
    keychain.seed(token("abcdef0123456789"));
    let keychain = Arc::new(keychain);
    let t = TestApp::start_in(dir.path(), |_, deps| deps.keychain = keychain.clone()).await;
    assert_eq!(account_of(&t.app).await.kept, KeepState::NotKept);
    let reports = reports_of(&t.app, ReportOp::Keychain);
    assert_eq!(reports.len(), 1);
    let ReportOutcome::Failed { reason } = &reports[0].outcome else {
        panic!("{:?}", reports[0])
    };
    assert!(reason.contains("could not be read"), "{reason}");
    let everything = format!("{reason} {:?}", reports[0].facts);
    assert!(
        !everything.contains("abcdef0123456789"),
        "the token text is in no report: {everything}"
    );
}

#[tokio::test]
async fn no_keychain_reads_as_not_kept_and_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = Arc::new(MemKeychain::with_mode(MemKeychainMode::NoKeychain));
    let t = TestApp::start_in(dir.path(), |_, deps| deps.keychain = keychain.clone()).await;
    assert_eq!(account_of(&t.app).await.kept, KeepState::NotKept);
    assert!(reports_of(&t.app, ReportOp::Keychain).is_empty());
}
