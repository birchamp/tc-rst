//! The committed files the gate reads, parsed into typed values: tC4's record
//! (`parity/tc4-baseline`), the Rust app's (`parity/app`), and the files that say what may
//! differ and who must capture what.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::behaviour::Recording;
use super::design::{Backend, DiffLayer, Layer, UnitId};

/// Where the record lives.
#[derive(Clone, Debug)]
pub struct Paths {
    /// `parity/tc4-baseline`.
    pub baseline: PathBuf,
    /// `parity/app`.
    pub app: PathBuf,
    /// `docs/APP-DESIGN.md`.
    pub doc: PathBuf,
    /// `docs/PLAN.md`.
    pub plan: PathBuf,
    /// tC4's behaviour recordings, `parity/tc4-baseline/behaviour`.
    pub tc4_behaviour: PathBuf,
    /// The tC4 view-state gallery, `parity/view-gallery`: tC4's side of every case.
    pub gallery: PathBuf,
}

impl Paths {
    pub fn committed() -> Self {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Self {
            baseline: repo.join("parity/tc4-baseline"),
            app: repo.join("parity/app"),
            doc: repo.join("docs/APP-DESIGN.md"),
            plan: repo.join("docs/PLAN.md"),
            tc4_behaviour: repo.join("parity/tc4-baseline/behaviour"),
            gallery: repo.join("parity/view-gallery"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Machine {
    pub platform: String,
    pub chromium: String,
    pub fonts: Option<FontList>,
}

impl Machine {
    /// Two captures whose pixels may be compared: the same machine, and one that lists its fonts.
    pub fn comparable(&self, other: &Machine) -> bool {
        self == other && self.fonts.is_some()
    }

    pub fn describe(&self) -> String {
        let fonts = self.fonts.as_ref().map_or_else(
            || "no font list".to_owned(),
            |f| format!("{} fonts {}", f.count, &f.sha256[..12.min(f.sha256.len())]),
        );
        format!("{} Chromium {} with {fonts}", self.platform, self.chromium)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontList {
    pub count: u32,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.width && y - self.y < self.height
    }

    pub fn inside(&self, outer: &Rect) -> bool {
        self.x >= outer.x
            && self.y >= outer.y
            && self.x + self.width <= outer.x + outer.width
            && self.y + self.height <= outer.y + outer.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

// ---- tC4's record -------------------------------------------------------------------------

/// `parity/tc4-baseline/manifest.json`, A6a's: only what the gate reads.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub tc4: Tc4Build,
    pub machine: Option<Machine>,
    pub viewport: Viewport,
    pub screens: Vec<BaselineScreen>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tc4Build {
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineScreen {
    pub name: String,
    pub scenario: String,
    pub locale: Option<String>,
    pub viewport: Option<Viewport>,
    pub sha256: String,
    pub dom: String,
    pub dom_sha256: String,
    #[serde(default)]
    pub home_cards: BTreeMap<String, String>,
    pub sync_toast: Option<SyncToast>,
    pub rebuild_differs: Option<RebuildDiffers>,
}

impl BaselineScreen {
    pub fn locale(&self) -> &str {
        self.locale.as_deref().unwrap_or("en")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RebuildDiffers {
    pub ruling: String,
}

/// The parts of the sync result toast (`observe` in parity/app/observe.mjs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncToast {
    pub ok: bool,
    pub code: Option<String>,
    pub text: String,
    pub received: u64,
    pub newest_wins: u64,
    pub verse_conflicts: u64,
    pub from: Option<String>,
    pub restored: u64,
    pub missing_pins: u64,
    pub links: Vec<String>,
    /// tC4's side only: capture.mjs lists the missing versions by tC4's own rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_versions: Option<Vec<String>>,
}

// ---- the Rust app's record -----------------------------------------------------------------

/// `parity/app/captures.json`, written by capture-rust.mjs.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Captures {
    #[serde(rename = "about")]
    pub _about: String,
    pub machine: Option<Machine>,
    pub build: Option<RustBuild>,
    /// Per scenario, tC4 repo path -> the Rust app's project folder.
    pub projects: BTreeMap<String, BTreeMap<String, String>>,
    pub screens: Vec<Capture>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RustBuild {
    pub ui: Option<String>,
    pub commit: Option<String>,
    pub api: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    Screen,
    Case,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub name: String,
    pub kind: RowKind,
    pub scenario: String,
    pub locale: String,
    pub by: UnitId,
    pub backend: Backend,
    pub layers: Vec<Layer>,
    pub error: Option<String>,
    pub page_errors: Vec<String>,
    pub refused: Vec<String>,
    pub home_cards: BTreeMap<String, String>,
    pub sync_toast: Option<SyncToast>,
    pub mask_rects: BTreeMap<String, Option<Rect>>,
    pub png: String,
    pub sha256: String,
    pub dom: String,
    pub dom_sha256: String,
}

impl Capture {
    pub fn claims(&self, layer: Layer) -> bool {
        self.layers.contains(&layer)
    }
}

// ---- what may differ, and who captures what --------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Masks {
    pub about: String,
    pub masks: Vec<Mask>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mask {
    pub id: String,
    pub ruling: String,
    pub why: String,
    /// tC4's element.
    pub target: TargetSpec,
    /// The rebuild's element when it is not `target`.
    pub rust: Option<TargetSpec>,
    /// Why the rebuild has no such element on these screens yet. The rest of the row's
    /// difference is then a residual's, and the gate fails once the rebuild draws the element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_absent: Option<String>,
    /// The rectangle the mask covers in tC4's screenshot of each screen it applies to.
    pub rects: BTreeMap<String, Rect>,
    pub meaning: Option<MaskMeaning>,
}

impl Mask {
    pub fn rust_target(&self) -> &TargetSpec {
        self.rust.as_ref().unwrap_or(&self.target)
    }
}

/// An element named by its test id, or by the text its first text run starts with.
/// `<OBS>` and `<BIBLE>` stand for the two projects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetSpec {
    Testid(String),
    Text(String),
}

/// Values of what the DOM shows that may differ, as (tC4, rebuild) pairs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskMeaning {
    pub home_cards: BTreeMap<String, (String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Residuals {
    pub about: String,
    pub general: Vec<GeneralResidual>,
    pub rows: BTreeMap<String, RowResidual>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralResidual {
    pub id: String,
    pub why: String,
    pub cites: String,
    pub until: Option<UnitId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowResidual {
    /// Exactly the layers the row differs in.
    pub layers: Vec<DiffLayer>,
    pub why: String,
    pub until: Option<UnitId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Units {
    pub about: String,
    pub landed: Vec<UnitId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scopes {
    pub about: String,
    pub scopes: BTreeMap<String, Scope>,
}

/// The element whose subtree a DOM-first capture compares: the first or last element with
/// `attr="value"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub attr: String,
    pub value: String,
    pub which: Which,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Which {
    First,
    Last,
}

/// `parity/app/cases.json`, written by parity/view-gallery/capture.mjs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cases {
    pub about: String,
    /// The tC4 commit the gallery mounted; none while there is no case.
    pub tc4: Option<Tc4Build>,
    /// The machine that captured tC4's side of every case: a case's pixels compare only with a
    /// Rust capture of the same machine.
    pub machine: Option<Machine>,
    pub cases: Vec<Case>,
}

/// A case of the tC4 view-state gallery (APP-DESIGN 7.4): tC4's side of it, under
/// `parity/view-gallery`, and the units of its §7.4 group, which capture the Rust side.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub group: String,
    /// The recorded state of real tC4 the case starts from (parity/view-gallery/states.json).
    pub state: String,
    pub scenario: String,
    pub locale: String,
    pub viewport: Option<Viewport>,
    /// The top-level keys of tC4's state the case's patch sets; empty when tC4 drew the
    /// recorded state itself.
    pub patch: Vec<String>,
    pub png: String,
    pub sha256: String,
    pub dom: String,
    pub dom_sha256: String,
    pub actions: String,
    pub actions_sha256: String,
    pub dom_unit: UnitId,
    pub pixels_unit: UnitId,
    /// The dialog a DOM-first case compares until its pixels unit captures it whole.
    pub scope: Option<Scope>,
}

impl Case {
    /// The unit that captures each layer of the case. A case has no meaning layer.
    pub fn units(&self) -> BTreeMap<Layer, UnitId> {
        [
            (Layer::Pixels, self.pixels_unit.clone()),
            (Layer::Dom, self.dom_unit.clone()),
        ]
        .into()
    }

    /// A case whose DOM is compared before its pixels: the page behind its dialog is not the
    /// port's yet.
    pub fn dom_first(&self) -> bool {
        self.dom_unit != self.pixels_unit
    }

    /// The case's tC4 files, relative to `parity/view-gallery`, with their sha256.
    pub fn files(&self) -> [(&str, &str); 3] {
        [
            (&self.png, &self.sha256),
            (&self.dom, &self.dom_sha256),
            (&self.actions, &self.actions_sha256),
        ]
    }
}

// ---- loading ---------------------------------------------------------------------------------

/// Everything the gate reads, parsed.
#[derive(Clone, Debug)]
pub struct Record {
    pub paths: Paths,
    pub manifest: Manifest,
    pub captures: Captures,
    pub masks: Masks,
    pub residuals: Residuals,
    pub units: Units,
    pub scopes: Scopes,
    pub cases: Cases,
    pub tc4_behaviour: BTreeMap<String, Recording>,
    pub rust_behaviour: BTreeMap<String, Recording>,
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The recordings in `dir`, by sequence; none when the folder is absent.
fn recordings(dir: &Path) -> Result<BTreeMap<String, Recording>, String> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    for name in sorted_files(dir)? {
        if let Some(sequence) = name.strip_suffix(".json") {
            let recording: Recording = read_json(&dir.join(&name))?;
            if recording.sequence != sequence {
                return Err(format!(
                    "{}: the recording is of sequence {}",
                    dir.join(&name).display(),
                    recording.sequence
                ));
            }
            out.insert(sequence.to_owned(), recording);
        }
    }
    Ok(out)
}

/// The names of the files in `dir`, sorted; none when the folder is absent.
pub fn sorted_files(dir: &Path) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

impl Record {
    pub fn load(paths: Paths) -> Result<Record, String> {
        Ok(Record {
            manifest: read_json(&paths.baseline.join("manifest.json"))?,
            captures: read_json(&paths.app.join("captures.json"))?,
            masks: read_json(&paths.app.join("masks.json"))?,
            residuals: read_json(&paths.app.join("residuals.json"))?,
            units: read_json(&paths.app.join("units.json"))?,
            scopes: read_json(&paths.app.join("scopes.json"))?,
            cases: read_json(&paths.app.join("cases.json"))?,
            tc4_behaviour: recordings(&paths.tc4_behaviour)?,
            rust_behaviour: recordings(&paths.app.join("behaviour"))?,
            paths,
        })
    }

    pub fn capture(&self, name: &str) -> Option<&Capture> {
        self.captures.screens.iter().find(|c| c.name == name)
    }

    pub fn case(&self, name: &str) -> Option<&Case> {
        self.cases.cases.iter().find(|c| c.name == name)
    }

    /// The Rust app's project folders of `scenario` as (folder, repo path) pairs, longest
    /// folder first, so `sample_burrito_obs` is never read as `sample_burrito`.
    pub fn rename(&self, scenario: &str) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .captures
            .projects
            .get(scenario)
            .map(|m| {
                m.iter()
                    .map(|(repo, folder)| (folder.clone(), repo.clone()))
                    .collect()
            })
            .unwrap_or_default();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        pairs
    }

    /// The masks that apply to the screen `name`.
    pub fn masks_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Mask> + 'a {
        self.masks
            .masks
            .iter()
            .filter(move |m| m.rects.contains_key(name))
    }
}
