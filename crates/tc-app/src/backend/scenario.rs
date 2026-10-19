//! A parity scenario's state, read into the memory backend (APP-DESIGN §4.2, §7.2; feature
//! `fixture-backend`).
//!
//! `parity/tc4-baseline/fixtures/build-state.mjs` lays a scenario out as tC4's mock server
//! keeps it: `repos/_local_/_local_/<project>/`, `repos/_local_/_sideloaded_/<resource>/`,
//! `client_settings/uw-tc4.json` (the install records), `vrs/*.json` and `provenance.json`.
//! Both apps start from the same bytes.
//!
//! - **Projects.** Each is seeded with B1a's `tc4_state_seed` and checked against its files
//!   (AD7). A project whose check fails is refused with every mismatch and shows nowhere; the
//!   others load. A refused project would make a screen look right for the wrong reason, and
//!   failing the whole scenario would load nothing while one sample still fails the check.
//! - **Teammates** are never part of a load: tC4 shows the shared scenario's screens without a
//!   conflict until a screen's steps make teammates change a verse (B10).
//! - **Resources.** The sideloaded burritos install into the app's own resource store under the
//!   pins of their install records, never anything of `bundle/en/` (Q-11).
//! - **No node in cargo tests.** [`ScenarioState::from_repository`] finds every byte of a
//!   scenario by the sha256 `parity/app/state.sha256.json` records for it, among this
//!   repository's committed files.

use std::collections::BTreeMap;
use std::io::{Cursor, Write as _};
use std::path::Path;

use serde::Deserialize;
use tc_api::ids::ProjectRef;
use tc_resources::{Installed, ResourcePin, ResourceStore, install_dcs_zip};

pub use super::Door43Remote;
use super::memory::{MemoryBackend, MemoryProject};
use super::seed::{SeedRefusal, SeedReport, Tc4Folder, fixture_project_id, tc4_state_seed};

const PROJECTS: &str = "repos/_local_/_local_/";
const SIDELOADED: &str = "repos/_local_/_sideloaded_/";
const INSTALL_RECORDS: &str = "client_settings/uw-tc4.json";
const PROVENANCE: &str = "provenance.json";
const VRS: &str = "vrs/";
/// tC4's name of a sideloaded resource in its install records.
const RECORD_PREFIX: &str = "_local_/_sideloaded_/";
/// The largest burrito a scenario installs, unpacked.
const MAX_UNPACKED: u64 = 64 << 20;

/// The folders whose committed files a scenario is assembled from.
const COMMITTED: [&str; 3] = [
    "fixtures/tc4",
    "fixtures/parity-state",
    "parity/tc4-baseline/fixtures/vendor",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScenarioError {
    #[error("{path} cannot be read: {problem}")]
    Io { path: String, problem: String },
    #[error("{path} is not where a scenario keeps anything: {problem}")]
    Layout { path: String, problem: String },
    #[error("the install records do not fit the sideloaded resources: {problem}")]
    Records { problem: String },
    #[error("{path} does not name the project's Door43 remote: {problem}")]
    Remote { path: String, problem: String },
    #[error("{dir} cannot be installed: {problem}")]
    Install { dir: String, problem: String },
    #[error("{path}: {problem}")]
    Digest { path: String, problem: String },
}

/// A parity scenario's files, by `/`-separated path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioState {
    files: BTreeMap<String, Vec<u8>>,
}

/// A project of a scenario: its folder in the app and its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioProject {
    pub folder: ProjectRef,
    /// Paths relative to the project folder.
    pub files: BTreeMap<String, Vec<u8>>,
    pub remote: Option<Door43Remote>,
}

impl ScenarioProject {
    /// The project as the seed reads a tC4 folder.
    pub fn tc4(&self) -> Result<Tc4Folder, SeedRefusal> {
        Tc4Folder::from_files(
            self.folder.as_str(),
            self.files.iter().map(|(p, b)| (p.clone(), b.clone())),
        )
    }
}

/// A sideloaded resource with the pin of its install record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideloadedResource {
    /// Its folder under `repos/_local_/_sideloaded_/`.
    pub dir: String,
    pub pin: ResourcePin,
    /// Paths relative to the resource folder.
    pub files: BTreeMap<String, Vec<u8>>,
}

/// What a scenario's load did with each project.
#[derive(Debug, Clone)]
pub struct ScenarioReport {
    pub loaded: Vec<(ProjectRef, SeedReport)>,
    /// Each project the seed refused, with every reason; none of it is in the backend.
    pub refused: Vec<(ProjectRef, SeedRefusal)>,
}

/// The kind of a path of the state, by its place alone.
fn check_place(path: &str) -> Result<(), ScenarioError> {
    let layout = |problem: &str| ScenarioError::Layout {
        path: path.to_owned(),
        problem: problem.to_owned(),
    };
    let bad = path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if bad {
        return Err(layout("not a relative path with '/' between its parts"));
    }
    let in_folder = |prefix: &str| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.split_once('/').is_some_and(|(dir, _)| !dir.is_empty()))
    };
    let vrs = path
        .strip_prefix(VRS)
        .is_some_and(|name| !name.contains('/') && name.ends_with(".json"));
    if in_folder(PROJECTS)
        || in_folder(SIDELOADED)
        || path == INSTALL_RECORDS
        || path == PROVENANCE
        || vrs
    {
        Ok(())
    } else {
        Err(layout(
            "a scenario holds repos/_local_/_local_/<project>/, repos/_local_/_sideloaded_/<resource>/, \
             client_settings/uw-tc4.json, vrs/*.json and provenance.json",
        ))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every plain file under `root`, by `/`-separated path. A link refuses: a scenario holds
/// only plain files.
fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) -> Result<(), ScenarioError> {
    let io = |path: &str, e: std::io::Error| ScenarioError::Io {
        path: if path.is_empty() {
            root.display().to_string()
        } else {
            path.to_owned()
        },
        problem: e.to_string(),
    };
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    for entry in std::fs::read_dir(&dir).map_err(|e| io(rel, e))? {
        let entry = entry.map_err(|e| io(rel, e))?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| ScenarioError::Io {
            path: format!("{rel}/{}", name.to_string_lossy()),
            problem: "the name is not text".to_owned(),
        })?;
        let path = if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{rel}/{name}")
        };
        let kind = std::fs::symlink_metadata(root.join(&path)).map_err(|e| io(&path, e))?;
        if kind.file_type().is_symlink() {
            return Err(ScenarioError::Io {
                path,
                problem: "a link: a scenario holds only plain files".to_owned(),
            });
        }
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else if kind.is_file() {
            let bytes = std::fs::read(root.join(&path)).map_err(|e| io(&path, e))?;
            out.insert(path, bytes);
        } else {
            return Err(ScenarioError::Io {
                path,
                problem: "not a plain file".to_owned(),
            });
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct Remote {
    name: String,
    url: String,
}

/// `https://<host>/<owner>/<repo>.git`, the `origin` of tC4's mock server's remote list.
fn door43_remote(path: &str, bytes: &[u8]) -> Result<Door43Remote, ScenarioError> {
    let refuse = |problem: String| ScenarioError::Remote {
        path: path.to_owned(),
        problem,
    };
    let remotes: Vec<Remote> = serde_json::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
    let origin = remotes
        .iter()
        .find(|r| r.name == "origin")
        .ok_or_else(|| refuse("no origin".to_owned()))?;
    origin
        .url
        .ends_with(".git")
        .then(|| Door43Remote::parse_url(&origin.url))
        .flatten()
        .ok_or_else(|| {
            refuse(format!(
                "{} is not https://<host>/<owner>/<repo>.git",
                origin.url
            ))
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallRecords {
    installed_resources: BTreeMap<String, ResourcePin>,
}

impl ScenarioState {
    /// The scenario in folder `dir`, as `node parity/app/state.mjs --out <dir>` builds it.
    pub fn read(dir: &Path) -> Result<Self, ScenarioError> {
        let mut files = BTreeMap::new();
        walk(dir, "", &mut files)?;
        Self::from_files(files)
    }

    /// A scenario given in memory, by `/`-separated paths.
    pub fn from_files(
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, ScenarioError> {
        let mut out = BTreeMap::new();
        for (path, bytes) in files {
            check_place(&path)?;
            if out.insert(path.clone(), bytes).is_some() {
                return Err(ScenarioError::Layout {
                    path,
                    problem: "given twice".to_owned(),
                });
            }
        }
        Ok(Self { files: out })
    }

    /// Scenario `scenario` assembled from this repository's committed bytes, exactly as
    /// `parity/app/state.sha256.json` records it: each file is found by its sha256 among
    /// `fixtures/tc4`, `fixtures/parity-state` and `parity/tc4-baseline/fixtures/vendor`. Only
    /// `provenance.json` is left out: it names the build, not the state.
    pub fn from_repository(root: &Path, scenario: &str) -> Result<Self, ScenarioError> {
        let digests = "parity/app/state.sha256.json";
        let recorded: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(digests)).map_err(|e| {
                ScenarioError::Io {
                    path: digests.to_owned(),
                    problem: e.to_string(),
                }
            })?)
            .map_err(|e| ScenarioError::Digest {
                path: digests.to_owned(),
                problem: e.to_string(),
            })?;
        let wanted =
            recorded["scenarios"][scenario]
                .as_object()
                .ok_or_else(|| ScenarioError::Digest {
                    path: digests.to_owned(),
                    problem: format!("no scenario {scenario}"),
                })?;
        let mut by_digest: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for folder in COMMITTED {
            let mut files = BTreeMap::new();
            walk(&root.join(folder), "", &mut files)?;
            for bytes in files.into_values() {
                by_digest.entry(sha256_hex(&bytes)).or_insert(bytes);
            }
        }
        let mut files = Vec::new();
        for (path, sha) in wanted {
            if path == PROVENANCE {
                continue;
            }
            let sha = sha.as_str().unwrap_or_default();
            let bytes = by_digest.get(sha).ok_or_else(|| ScenarioError::Digest {
                path: path.clone(),
                problem: format!("no committed file has the recorded sha256 {sha}"),
            })?;
            files.push((path.clone(), bytes.clone()));
        }
        Self::from_files(files)
    }

    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    /// The files under `prefix`, grouped by the folder right under it.
    fn folders(&self, prefix: &str) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        let mut out: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
        for (path, bytes) in &self.files {
            if let Some((dir, rest)) = path.strip_prefix(prefix).and_then(|r| r.split_once('/')) {
                out.entry(dir.to_owned())
                    .or_default()
                    .insert(rest.to_owned(), bytes.clone());
            }
        }
        out
    }

    /// The projects, each under the folder name it has in the app.
    pub fn projects(&self) -> Result<Vec<ScenarioProject>, ScenarioError> {
        self.folders(PROJECTS)
            .into_iter()
            .map(|(name, files)| {
                let folder = ProjectRef::parse(&name).map_err(|e| ScenarioError::Layout {
                    path: format!("{PROJECTS}{name}"),
                    problem: format!("not a project folder name: {e}"),
                })?;
                let remote = match files.get(".remotes.json") {
                    Some(bytes) => Some(door43_remote(
                        &format!("{PROJECTS}{name}/.remotes.json"),
                        bytes,
                    )?),
                    None => None,
                };
                Ok(ScenarioProject {
                    folder,
                    files,
                    remote,
                })
            })
            .collect()
    }

    /// The sideloaded resources, each with the pin of its install record. A record without
    /// its folder, or a folder without its record, refuses.
    pub fn resources(&self) -> Result<Vec<SideloadedResource>, ScenarioError> {
        let records = |problem: String| ScenarioError::Records { problem };
        let mut folders = self.folders(SIDELOADED);
        let bytes = self
            .files
            .get(INSTALL_RECORDS)
            .ok_or_else(|| records(format!("{INSTALL_RECORDS} is missing")))?;
        let parsed: InstallRecords = serde_json::from_slice(bytes)
            .map_err(|e| records(format!("{INSTALL_RECORDS}: {e}")))?;
        let mut out = Vec::new();
        for (name, pin) in parsed.installed_resources {
            let dir = name
                .strip_prefix(RECORD_PREFIX)
                .ok_or_else(|| records(format!("{name} is not a sideloaded resource")))?;
            let files = folders
                .remove(dir)
                .ok_or_else(|| records(format!("{name} has no folder {SIDELOADED}{dir}")))?;
            out.push(SideloadedResource {
                dir: dir.to_owned(),
                pin,
                files,
            });
        }
        if let Some(dir) = folders.keys().next() {
            return Err(records(format!("{SIDELOADED}{dir} has no install record")));
        }
        Ok(out)
    }
}

/// The resource's files as the burrito zip a person would sideload, `metadata.json` at its
/// root.
fn burrito_zip(resource: &SideloadedResource) -> Result<Vec<u8>, ScenarioError> {
    let failed = |problem: String| ScenarioError::Install {
        dir: resource.dir.clone(),
        problem,
    };
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in &resource.files {
        writer
            .start_file(path.as_str(), options)
            .map_err(|e| failed(e.to_string()))?;
        writer.write_all(bytes).map_err(|e| failed(e.to_string()))?;
    }
    Ok(writer
        .finish()
        .map_err(|e| failed(e.to_string()))?
        .into_inner())
}

/// Installs each resource into `store` under its record's pin, checked as any sideloaded
/// burrito is. One already installed is answered as it is, so a restart over the same app
/// data installs nothing.
pub fn install_resources(
    resources: &[SideloadedResource],
    store: &ResourceStore,
) -> Result<Vec<Installed>, ScenarioError> {
    resources
        .iter()
        .map(|resource| {
            let zip = burrito_zip(resource)?;
            install_dcs_zip(store, &resource.pin, &zip, MAX_UNPACKED).map_err(|e| {
                ScenarioError::Install {
                    dir: resource.dir.clone(),
                    problem: e.to_string(),
                }
            })
        })
        .collect()
}

/// Installs `resources` into the app's own resource store under `app_data`, the store
/// `App::start` opens next (§4.2): what `tc4 serve --scenario` does before the app starts.
pub fn install_resources_into(
    app_data: &Path,
    resources: &[SideloadedResource],
) -> Result<Vec<Installed>, ScenarioError> {
    let dir = app_data.join(crate::RESOURCES_FOLDER);
    let store = ResourceStore::open(&dir).map_err(|e| ScenarioError::Install {
        dir: dir.display().to_string(),
        problem: e.to_string(),
    })?;
    install_resources(resources, &store)
}

impl MemoryBackend {
    /// A memory backend holding the scenario's projects that pass the seed's check, with the
    /// report of what was loaded and what was refused. It adds no teammate.
    pub fn from_scenario(
        state: &ScenarioState,
    ) -> Result<(MemoryBackend, ScenarioReport), ScenarioError> {
        let backend = MemoryBackend::new();
        let mut report = ScenarioReport {
            loaded: Vec::new(),
            refused: Vec::new(),
        };
        for project in state.projects()? {
            let seeded = project
                .tc4()
                .and_then(|tc4| tc4_state_seed(&tc4, fixture_project_id(project.folder.as_str())));
            match seeded {
                Ok(seed) => {
                    let seed_report = seed.report.clone();
                    backend
                        .add(
                            project.folder.clone(),
                            MemoryProject::from_seed(seed, project.remote.clone()),
                        )
                        .map_err(|e| ScenarioError::Layout {
                            path: format!("{PROJECTS}{}", project.folder),
                            problem: e.to_string(),
                        })?;
                    report.loaded.push((project.folder, seed_report));
                }
                Err(refusal) => report.refused.push((project.folder, refusal)),
            }
        }
        Ok((backend, report))
    }
}
