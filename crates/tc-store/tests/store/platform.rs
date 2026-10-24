//! INV-SYNCED-FOLDER-DETECTION-IS-EXACT: a folder counts as synced by a cloud client only when
//! that client syncs it. Dropbox's settings folder never marks a project (R28).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

use tc_store::platform::synced_by;

fn env(vars: &[(&str, &Path)]) -> impl Fn(&str) -> Option<OsString> + use<> {
    let vars: BTreeMap<String, OsString> = vars
        .iter()
        .map(|(name, path)| ((*name).to_owned(), path.as_os_str().to_owned()))
        .collect();
    move |name| vars.get(name).cloned()
}

/// Dropbox's `info.json`, naming `root` as the synced folder.
fn info_json(settings: &Path, root: &Path) {
    std::fs::create_dir_all(settings).unwrap();
    std::fs::write(
        settings.join("info.json"),
        format!(r#"{{"personal":{{"path":{:?}}}}}"#, root.to_string_lossy()),
    )
    .unwrap();
}

#[test]
fn a_dropbox_settings_folder_in_home_never_makes_a_project_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    let dropbox = home.join("Dropbox");
    std::fs::create_dir_all(&dropbox).unwrap();
    let project = home.join("translationCore/projects/mine");

    // Linux and macOS: `~/.dropbox` is the settings folder; only the folder it names syncs.
    info_json(&home.join(".dropbox"), &dropbox);
    let unix = env(&[("HOME", &home)]);
    assert_eq!(
        synced_by(&unix, &project),
        None,
        "a project under home is not synced"
    );
    assert_eq!(
        synced_by(&unix, &dropbox.join("Work/project")).as_deref(),
        Some("Dropbox"),
        "a project in the folder info.json names is"
    );

    // Windows: the settings are under %APPDATA% or %LOCALAPPDATA%.
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let windows = tempfile::tempdir().unwrap();
        let app_data = windows.path().canonicalize().unwrap();
        info_json(&app_data.join("Dropbox"), &dropbox);
        let env = env(&[(var, &app_data), ("USERPROFILE", &home)]);
        assert_eq!(synced_by(&env, &project), None, "{var}: home is not synced");
        assert_eq!(
            synced_by(&env, &dropbox.join("p")).as_deref(),
            Some("Dropbox"),
            "{var}: the named folder is"
        );
    }

    // A `.dropbox` marker file in a folder above the project marks a synced root.
    let root = home.join("shared");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".dropbox"), b"").unwrap();
    assert_eq!(
        synced_by(&env(&[]), &root.join("projects/mine")).as_deref(),
        Some("Dropbox")
    );
}
