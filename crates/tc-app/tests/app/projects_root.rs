//! The projects folder of Q-4: the default, the local fallback and the first-launch check.

use std::path::{Path, PathBuf};

use tc_app::config::{
    ConfigError, HostOs, ProjectsFolder, RootChoice, choose_projects_root, default_projects_root,
    local_projects_root,
};

use super::common::{FakeFolders, env};

fn default_in(home: &str) -> PathBuf {
    Path::new(home).join("translationCore4").join("projects")
}

#[test]
fn default_projects_root_is_under_userprofile_on_windows() {
    let host = env(
        HostOs::Windows,
        &[("USERPROFILE", "/x/User"), ("HOME", "/wrong")],
    );
    let root = default_projects_root(&host).unwrap();
    assert_eq!(root, default_in("/x/User"));
    assert!(!root.starts_with("/wrong"));
    // The path is made of components, never a string with a separator in it.
    let parts: Vec<_> = root.components().collect();
    assert_eq!(parts[parts.len() - 2].as_os_str(), "translationCore4");
    assert_eq!(parts[parts.len() - 1].as_os_str(), "projects");
}

#[test]
fn windows_variable_names_ignore_case() {
    let host = env(HostOs::Windows, &[("UserProfile", "/x/User")]);
    assert_eq!(default_projects_root(&host).unwrap(), default_in("/x/User"));
}

#[test]
fn default_projects_root_is_under_home_elsewhere() {
    for os in [HostOs::Linux, HostOs::MacOs] {
        let host = env(os, &[("HOME", "/h/ana"), ("USERPROFILE", "/wrong")]);
        assert_eq!(default_projects_root(&host).unwrap(), default_in("/h/ana"));
    }
}

#[test]
fn no_home_is_a_start_error() {
    let windows = env(HostOs::Windows, &[("HOME", "/h/ana")]);
    assert!(matches!(
        default_projects_root(&windows),
        Err(ConfigError::NoHome {
            variable: "USERPROFILE"
        })
    ));
    let empty = env(HostOs::Linux, &[("HOME", "")]);
    assert!(matches!(
        choose_projects_root(&ProjectsFolder::Default, &empty, &FakeFolders::default()),
        Err(ConfigError::NoHome { variable: "HOME" })
    ));
}

#[test]
fn local_folder_per_os() {
    let windows = env(
        HostOs::Windows,
        &[
            ("USERPROFILE", "/x/User"),
            ("LOCALAPPDATA", "/x/User/Local"),
        ],
    );
    assert_eq!(
        local_projects_root(&windows).unwrap(),
        default_in("/x/User/Local")
    );
    let no_local = env(HostOs::Windows, &[("USERPROFILE", "/x/User")]);
    assert!(matches!(
        local_projects_root(&no_local),
        Err(ConfigError::NoLocalData {
            variable: "LOCALAPPDATA"
        })
    ));

    let mac = env(HostOs::MacOs, &[("HOME", "/Users/ana")]);
    assert_eq!(
        local_projects_root(&mac).unwrap(),
        default_in("/Users/ana/Library/Application Support")
    );

    let xdg = env(
        HostOs::Linux,
        &[("HOME", "/h/ana"), ("XDG_DATA_HOME", "/data/xdg")],
    );
    assert_eq!(local_projects_root(&xdg).unwrap(), default_in("/data/xdg"));
    let relative = env(
        HostOs::Linux,
        &[("HOME", "/h/ana"), ("XDG_DATA_HOME", "data/xdg")],
    );
    assert_eq!(
        local_projects_root(&relative).unwrap(),
        default_in("/h/ana/.local/share")
    );
    let unset = env(HostOs::Linux, &[("HOME", "/h/ana")]);
    assert_eq!(
        local_projects_root(&unset).unwrap(),
        default_in("/h/ana/.local/share")
    );
}

fn linux() -> tc_app::config::HostEnv {
    env(HostOs::Linux, &[("HOME", "/h/ana")])
}

fn local() -> PathBuf {
    default_in("/h/ana/.local/share")
}

#[test]
fn first_launch_creates_the_default_when_no_client_copies_it() {
    let root =
        choose_projects_root(&ProjectsFolder::Default, &linux(), &FakeFolders::default()).unwrap();
    assert_eq!(root.path, default_in("/h/ana"));
    assert_eq!(root.chosen, RootChoice::FirstLaunch);
    assert!(root.chosen.is_first_launch());
}

#[test]
fn first_launch_moves_to_the_local_folder_when_the_default_is_synced() {
    let mut check = FakeFolders::default();
    check
        .synced
        .insert(PathBuf::from("/h/ana"), "OneDrive".into());
    // The local data folder is outside the synced home folder in this setup.
    let host = env(
        HostOs::Linux,
        &[("HOME", "/h/ana"), ("XDG_DATA_HOME", "/var/data")],
    );
    let root = choose_projects_root(&ProjectsFolder::Default, &host, &check).unwrap();
    assert_eq!(root.path, default_in("/var/data"));
    assert_eq!(
        root.chosen,
        RootChoice::FirstLaunchLocal {
            default_synced_by: "OneDrive".into()
        }
    );
}

#[test]
fn first_launch_keeps_the_default_when_both_are_synced() {
    let mut check = FakeFolders::default();
    // The local folder is below the home folder here, so a client copies both.
    check
        .synced
        .insert(PathBuf::from("/h/ana"), "OneDrive".into());
    let root = choose_projects_root(&ProjectsFolder::Default, &linux(), &check).unwrap();
    assert_eq!(root.path, default_in("/h/ana"));
    assert_eq!(
        root.chosen,
        RootChoice::FirstLaunchSynced {
            synced_by: "OneDrive".into()
        }
    );
}

#[test]
fn a_computer_with_no_local_data_folder_keeps_the_default_even_when_synced() {
    let mut check = FakeFolders::default();
    check
        .synced
        .insert(PathBuf::from("/x/User"), "OneDrive".into());
    let host = env(HostOs::Windows, &[("USERPROFILE", "/x/User")]);
    let root = choose_projects_root(&ProjectsFolder::Default, &host, &check).unwrap();
    assert_eq!(root.path, default_in("/x/User"));
    assert!(matches!(root.chosen, RootChoice::FirstLaunchSynced { .. }));
}

#[test]
fn an_existing_default_is_used_without_a_check() {
    let mut check = FakeFolders::default();
    check.folders.insert(default_in("/h/ana"));
    check
        .synced
        .insert(PathBuf::from("/h/ana"), "OneDrive".into());
    let root = choose_projects_root(&ProjectsFolder::Default, &linux(), &check).unwrap();
    assert_eq!(root.path, default_in("/h/ana"));
    assert_eq!(root.chosen, RootChoice::Existing);
    assert!(!root.chosen.is_first_launch());
}

#[test]
fn an_existing_local_folder_is_used_on_later_launches() {
    let mut check = FakeFolders::default();
    check.folders.insert(local());
    // The default is no longer copied, and it still is not created: the local folder holds
    // the person's projects.
    let root = choose_projects_root(&ProjectsFolder::Default, &linux(), &check).unwrap();
    assert_eq!(root.path, local());
    assert_eq!(root.chosen, RootChoice::ExistingLocal);
}

#[test]
fn a_local_folder_beside_a_default_one_is_used_and_names_the_default() {
    // The local folder exists only because an earlier first launch chose it, so it holds this
    // app's projects. A default that appeared later does not replace it.
    let mut check = FakeFolders::default();
    check.folders.insert(local());
    check.folders.insert(default_in("/h/ana"));
    let root = choose_projects_root(&ProjectsFolder::Default, &linux(), &check).unwrap();
    assert_eq!(root.path, local());
    assert_eq!(
        root.chosen,
        RootChoice::LocalBesideDefault {
            default: default_in("/h/ana")
        }
    );
    assert!(!root.chosen.is_first_launch());
}

#[test]
fn a_local_folder_that_is_the_default_folder_is_not_beside_itself() {
    // XDG_DATA_HOME can name the home folder: the two paths are then one folder.
    let host = env(
        HostOs::Linux,
        &[("HOME", "/h/ana"), ("XDG_DATA_HOME", "/h/ana")],
    );
    let mut check = FakeFolders::default();
    check.folders.insert(default_in("/h/ana"));
    let root = choose_projects_root(&ProjectsFolder::Default, &host, &check).unwrap();
    assert_eq!(root.path, default_in("/h/ana"));
    assert_eq!(root.chosen, RootChoice::Existing);
}

#[test]
fn a_given_folder_is_used_as_given() {
    let mut check = FakeFolders::default();
    check
        .synced
        .insert(PathBuf::from("/given"), "OneDrive".into());
    // No home folder is needed, and no check is made.
    let host = env(HostOs::Linux, &[]);
    let root = choose_projects_root(
        &ProjectsFolder::Given(PathBuf::from("/given/projects")),
        &host,
        &check,
    )
    .unwrap();
    assert_eq!(root.path, Path::new("/given/projects"));
    assert_eq!(root.chosen, RootChoice::Given);
}
