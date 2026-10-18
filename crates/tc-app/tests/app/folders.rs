//! The cloud-folder detection, driven by a described computer and real temporary folders.

use std::path::Path;

use tc_app::config::HostOs;
use tc_app::folders::{FolderCheck, HostFolders};

use super::common::env;

fn projects_below(dir: &Path) -> std::path::PathBuf {
    dir.join("translationCore4").join("projects")
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn host_folders_find_a_onedrive_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("OneDrive");
    std::fs::create_dir(&root).unwrap();
    let host = env(HostOs::Linux, &[("OneDrive", text(&root))]);
    let folders = HostFolders::new(host);
    // The projects folder is not there yet, as at a first launch.
    let wanted = projects_below(&root.join("Documents"));
    assert!(!wanted.exists());
    assert_eq!(folders.synced_by(&wanted).as_deref(), Some("OneDrive"));
    assert_eq!(folders.synced_by(&projects_below(tmp.path())), None);
}

#[test]
fn the_other_onedrive_variables_count_too() {
    let tmp = tempfile::tempdir().unwrap();
    for var in ["OneDriveConsumer", "OneDriveCommercial"] {
        let root = tmp.path().join(var);
        let folders = HostFolders::new(env(HostOs::Linux, &[(var, text(&root))]));
        assert_eq!(
            folders.synced_by(&projects_below(&root)).as_deref(),
            Some("OneDrive")
        );
    }
}

#[cfg(unix)]
#[test]
fn a_root_behind_a_symbolic_link_still_matches() {
    // macOS keeps temporary and home folders behind links (/var -> /private/var).
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let host = env(HostOs::Linux, &[("OneDrive", text(&link))]);
    let folders = HostFolders::new(host);
    assert_eq!(
        folders.synced_by(&projects_below(&real)).as_deref(),
        Some("OneDrive")
    );
    assert_eq!(
        folders.synced_by(&projects_below(&link)).as_deref(),
        Some("OneDrive")
    );
}

#[test]
fn host_folders_find_a_syncthing_marker() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".stfolder")).unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[]));
    let wanted = projects_below(&tmp.path().join("a").join("b"));
    assert_eq!(folders.synced_by(&wanted).as_deref(), Some("Syncthing"));
}

#[test]
fn host_folders_read_dropbox_info_json() {
    let tmp = tempfile::tempdir().unwrap();
    let dropbox = tmp.path().join("Dropbox");
    std::fs::create_dir_all(tmp.path().join(".dropbox")).unwrap();
    std::fs::write(
        tmp.path().join(".dropbox").join("info.json"),
        format!(r#"{{"personal":{{"path":{:?}}}}}"#, text(&dropbox)),
    )
    .unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[("HOME", text(tmp.path()))]));
    assert_eq!(
        folders
            .synced_by(&projects_below(&dropbox.join("Work")))
            .as_deref(),
        Some("Dropbox")
    );
}

#[test]
fn dropbox_settings_folder_in_home_does_not_mark_the_home_folder() {
    // `~/.dropbox` is Dropbox's settings folder, not the marker of a synced folder.
    let tmp = tempfile::tempdir().unwrap();
    let dropbox = tmp.path().join("Dropbox");
    std::fs::create_dir_all(tmp.path().join(".dropbox")).unwrap();
    std::fs::write(
        tmp.path().join(".dropbox").join("info.json"),
        format!(r#"{{"personal":{{"path":{:?}}}}}"#, text(&dropbox)),
    )
    .unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[("HOME", text(tmp.path()))]));
    assert_eq!(folders.synced_by(&projects_below(tmp.path())), None);
}

#[test]
fn a_dropbox_marker_in_an_ancestor_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Dropbox");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".dropbox"), b"").unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[]));
    assert_eq!(
        folders.synced_by(&projects_below(&root)).as_deref(),
        Some("Dropbox")
    );
}

#[test]
fn host_folders_find_icloud_under_home() {
    let tmp = tempfile::tempdir().unwrap();
    let icloud = tmp
        .path()
        .join("Library")
        .join("Mobile Documents")
        .join("com~apple~CloudDocs");
    let folders = HostFolders::new(env(HostOs::MacOs, &[("HOME", text(tmp.path()))]));
    assert_eq!(
        folders
            .synced_by(&projects_below(&icloud.join("Documents")))
            .as_deref(),
        Some("iCloud Drive")
    );
    let storage = tmp.path().join("Library").join("CloudStorage").join("Box");
    assert_eq!(
        folders.synced_by(&projects_below(&storage)).as_deref(),
        Some("a cloud storage client")
    );
    let drive = tmp.path().join("Google Drive");
    assert_eq!(
        folders.synced_by(&projects_below(&drive)).as_deref(),
        Some("Google Drive")
    );
}

#[test]
fn a_plain_home_folder_is_not_synced() {
    let tmp = tempfile::tempdir().unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[("HOME", text(tmp.path()))]));
    assert_eq!(folders.synced_by(&projects_below(tmp.path())), None);
}

#[test]
fn is_folder_is_true_for_folders_only() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    let folders = HostFolders::new(env(HostOs::Linux, &[]));
    assert!(folders.is_folder(tmp.path()));
    assert!(!folders.is_folder(&file));
    assert!(!folders.is_folder(&tmp.path().join("missing")));
}
