//! What `tc4 serve` tells the app at start, and the rule for the projects folder
//! (APP-DESIGN §4.9, owner's answer Q-4).
//!
//! The rule is pure: [`choose_projects_root`] reads the computer through a [`FolderCheck`] and
//! a [`HostEnv`] and says which folder to use and why. `App::start` creates the folder when
//! the rule says this is a first launch.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use tc_api::app::BuildInfo;
use tc_door43::ServerUrl;

use crate::folders::FolderCheck;

/// The folder that holds the profile of tC4 and of this rebuild alike.
const PROFILE_FOLDER: &str = "translationCore4";
const PROJECTS_FOLDER: &str = "projects";

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Everything the app keeps for this person on this computer (§4.9).
    pub app_data: PathBuf,
    pub projects: ProjectsFolder,
    /// The Door43 server (`--door43`); tests give the fake's address.
    pub door43: ServerUrl,
    pub build: BuildInfo,
    /// `--bundle <dir>`: the bundled resource store the installer ships, read only. `None`:
    /// nothing is seeded (development, tests, the parity captures, Q-11).
    pub bundle: Option<PathBuf>,
    /// `tc4 serve --test-hooks`: whether the `test` service answers.
    #[cfg(feature = "test-hooks")]
    pub test_hooks: bool,
    /// `--test-seed-pace <ms>`: the seed waits this long before each file, so a test can
    /// watch it and close it on any disk.
    #[cfg(feature = "test-hooks")]
    pub seed_pace: Option<std::time::Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectsFolder {
    /// `--projects <dir>`: the person or the harness named it, so it is used as given.
    Given(PathBuf),
    /// The default of Q-4.
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Windows,
    MacOs,
    /// Every other Unix follows Linux's rule.
    Linux,
}

/// The environment variables of the computer and its kind. A value the app reads comes
/// from here, never from `std::env` directly, so a test on Linux can describe a Windows
/// computer.
#[derive(Debug, Clone)]
pub struct HostEnv {
    os: HostOs,
    vars: BTreeMap<String, OsString>,
}

impl HostEnv {
    pub fn current() -> Self {
        let os = if cfg!(windows) {
            HostOs::Windows
        } else if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else {
            HostOs::Linux
        };
        // A name that is not UTF-8 is none the app reads.
        let vars =
            std::env::vars_os().filter_map(|(name, value)| Some((name.into_string().ok()?, value)));
        Self::new(os, vars)
    }

    pub fn new(os: HostOs, vars: impl IntoIterator<Item = (String, OsString)>) -> Self {
        let vars = vars
            .into_iter()
            .map(|(name, value)| (Self::key(os, &name), value))
            .collect();
        Self { os, vars }
    }

    pub fn os(&self) -> HostOs {
        self.os
    }

    /// A variable's value; an empty value counts as unset, as it does for tc-store's cloud
    /// detection. Windows names ignore letter case.
    pub fn var(&self, name: &str) -> Option<&OsStr> {
        self.vars
            .get(&Self::key(self.os, name))
            .map(OsString::as_os_str)
            .filter(|value| !value.is_empty())
    }

    fn key(os: HostOs, name: &str) -> String {
        match os {
            HostOs::Windows => name.to_ascii_uppercase(),
            HostOs::MacOs | HostOs::Linux => name.to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("this computer does not say where the home folder is: {variable} is not set")]
    NoHome { variable: &'static str },
    #[error("this computer does not say where local app data is: {variable} is not set")]
    NoLocalData { variable: &'static str },
}

/// The home folder: `%USERPROFILE%` on Windows, `$HOME` elsewhere.
pub fn home_dir(env: &HostEnv) -> Result<PathBuf, ConfigError> {
    let variable = match env.os() {
        HostOs::Windows => "USERPROFILE",
        HostOs::MacOs | HostOs::Linux => "HOME",
    };
    env.var(variable)
        .map(PathBuf::from)
        .ok_or(ConfigError::NoHome { variable })
}

fn projects_below(profile_parent: PathBuf) -> PathBuf {
    profile_parent.join(PROFILE_FOLDER).join(PROJECTS_FOLDER)
}

/// The default projects folder (Q-4): `translationCore4/projects` in the home folder. It is
/// not under Documents, which OneDrive redirects on Windows (STORAGE §5.0).
pub fn default_projects_root(env: &HostEnv) -> Result<PathBuf, ConfigError> {
    home_dir(env).map(projects_below)
}

/// The projects folder used when a cloud client copies the default one: below this
/// computer's local data folder, which no client copies (OneDrive's Known Folder Move,
/// iCloud Drive's Desktop and Documents and roaming profiles leave it out).
pub fn local_projects_root(env: &HostEnv) -> Result<PathBuf, ConfigError> {
    let local_data = match env.os() {
        HostOs::Windows => {
            env.var("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or(ConfigError::NoLocalData {
                    variable: "LOCALAPPDATA",
                })?
        }
        HostOs::MacOs => home_dir(env)?.join("Library").join("Application Support"),
        HostOs::Linux => match env.var("XDG_DATA_HOME").map(PathBuf::from) {
            // The XDG rule: a relative value is invalid and ignored.
            Some(dir) if dir.is_absolute() => dir,
            _ => home_dir(env)?.join(".local").join("share"),
        },
    };
    Ok(projects_below(local_data))
}

/// The folder to use and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectsRoot {
    pub path: PathBuf,
    pub chosen: RootChoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootChoice {
    /// `--projects`.
    Given,
    /// The default folder was already there; no check is made, because nothing moves and
    /// each project's open gates a synced folder anyway (STORAGE §5.0).
    Existing,
    /// The local folder was already there: an earlier first launch chose it.
    ExistingLocal,
    /// The local folder exists and so does the default one. The local folder is there only
    /// because an earlier first launch chose it and filled it, so it is used and the default
    /// one, which appeared later, is named in the report. Using the default instead would
    /// stop listing projects the person already works in.
    LocalBesideDefault { default: PathBuf },
    /// First launch: no client copies the default folder, so it is created.
    FirstLaunch,
    /// First launch: a client copies the default folder and none copies the local one.
    FirstLaunchLocal { default_synced_by: String },
    /// First launch: both are copied. The default is created, and every project then opens
    /// read-only with STORAGE's copy offer, which tells the person why.
    FirstLaunchSynced { synced_by: String },
}

impl RootChoice {
    /// Whether the folder was chosen because none existed, so a later launch finds it.
    pub fn is_first_launch(&self) -> bool {
        matches!(
            self,
            Self::FirstLaunch | Self::FirstLaunchLocal { .. } | Self::FirstLaunchSynced { .. }
        )
    }
}

/// Which projects folder to use. A folder that exists is always used, so this rule never
/// moves a project and a launch never lists fewer projects than the one before it; only a
/// first launch, when neither default exists, looks at the cloud detection.
///
/// When both exist, the local folder wins: no app path creates it except a first launch that
/// found the default synced, so it holds the projects earlier launches listed, and a default
/// folder made later by hand or by a sync client does not replace it. The default folder's
/// projects, if it has any, are not listed; [`RootChoice::LocalBesideDefault`] names it so
/// the report says so.
pub fn choose_projects_root(
    folder: &ProjectsFolder,
    env: &HostEnv,
    check: &dyn FolderCheck,
) -> Result<ProjectsRoot, ConfigError> {
    let default = match folder {
        ProjectsFolder::Given(path) => {
            return Ok(ProjectsRoot {
                path: path.clone(),
                chosen: RootChoice::Given,
            });
        }
        ProjectsFolder::Default => default_projects_root(env)?,
    };
    // A computer that cannot name its local data folder keeps the default.
    let local = local_projects_root(env).ok();

    // `XDG_DATA_HOME` can name the home folder, which makes the two paths one folder.
    let local_folder = local
        .as_ref()
        .filter(|local| **local != default && check.is_folder(local));
    if check.is_folder(&default) {
        return Ok(match local_folder {
            Some(local) => ProjectsRoot {
                path: local.clone(),
                chosen: RootChoice::LocalBesideDefault { default },
            },
            None => ProjectsRoot {
                path: default,
                chosen: RootChoice::Existing,
            },
        });
    }
    if let Some(local) = local_folder {
        return Ok(ProjectsRoot {
            path: local.clone(),
            chosen: RootChoice::ExistingLocal,
        });
    }
    let Some(default_synced_by) = check.synced_by(&default) else {
        return Ok(ProjectsRoot {
            path: default,
            chosen: RootChoice::FirstLaunch,
        });
    };
    match local {
        Some(local) if check.synced_by(&local).is_none() => Ok(ProjectsRoot {
            path: local,
            chosen: RootChoice::FirstLaunchLocal { default_synced_by },
        }),
        _ => Ok(ProjectsRoot {
            path: default,
            chosen: RootChoice::FirstLaunchSynced {
                synced_by: default_synced_by,
            },
        }),
    }
}
