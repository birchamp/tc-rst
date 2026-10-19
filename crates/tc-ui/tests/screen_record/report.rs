//! `parity/app/report.json`: what the four layers find for every baseline screen and gallery
//! case, computed from the committed captures alone, so the committed file can be checked
//! byte for byte (APP-DESIGN 7.5).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use super::behaviour::{self, SequenceResult};
use super::design::{Backend, Design, DiffLayer, Layer, UnitId};
use super::dom::{self, DomError, DomMask, Side, Snapshot};
use super::inputs::{
    Capture, Machine, Record, Rect, RowKind, RustBuild, Scope, SyncToast, TargetSpec, read_json,
    sorted_files,
};
use super::meaning::{self, DeclaredCard};
use super::pixels;

/// tC4's repo paths of the two projects (parity/app/screens.mjs), for `<BIBLE>` and `<OBS>`.
pub const BIBLE: &str = "_local_/_local_/sample_burrito";
pub const OBS: &str = "_local_/_local_/sample_burrito_obs";

pub fn resolve(text: &str) -> String {
    text.replace("<OBS>", OBS).replace("<BIBLE>", BIBLE)
}

fn resolve_target(target: &TargetSpec) -> TargetSpec {
    match target {
        TargetSpec::Testid(id) => TargetSpec::Testid(resolve(id)),
        TargetSpec::Text(text) => TargetSpec::Text(resolve(text)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Every compared layer is equal.
    Same,
    /// Every difference lies inside a declared mask.
    Declared,
    /// A compared layer differs: `residuals.json` must explain the row, naming exactly the
    /// layers that differ.
    Differs,
    /// No layer is captured yet: `missing` names the unit that captures each.
    Unported,
    /// The capture or its comparison failed; nothing excuses it.
    Failed,
}

/// Why a row failed, for the gate's rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// A capture error, a page error, a view or boot error, a refused request or the settle
    /// deadline; a screenshot of another size than tC4's; or a DOM file that is not in the
    /// snapshot format.
    Capture,
    /// Pixels from machines whose fingerprints differ, or one with no font list.
    Machine,
    /// A mask's element is not on a side.
    Mask,
    /// A DOM-first capture's scope matches nothing.
    Scope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelsRow {
    pub tc4_sha256: String,
    pub rust_sha256: String,
    pub mismatched: u64,
    pub outside_masks: u64,
    pub masks: Vec<String>,
    pub diff: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomRow {
    pub tc4_sha256: String,
    pub rust_sha256: String,
    pub scope: Option<Scope>,
    pub same: bool,
    pub declared: Vec<String>,
    pub first_diffs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeaningRow {
    pub same: bool,
    pub declared: Vec<String>,
    pub diffs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub kind: RowKind,
    pub scenario: String,
    pub locale: String,
    pub status: Status,
    /// The layers not captured yet, each with the unit that captures it.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub missing: BTreeMap<Layer, UnitId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<UnitId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    /// The unit that must capture a scripted screen again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_until: Option<UnitId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub failure: Option<FailureKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixels: Option<PixelsRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meaning: Option<MeaningRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub behaviour: Vec<SequenceResult>,
}

impl Row {
    /// The layers that differ outside the masks: a row is `differs` when there is one, and its
    /// residual names exactly these.
    pub fn differing(&self) -> BTreeSet<DiffLayer> {
        [
            (
                DiffLayer::Pixels,
                self.pixels.as_ref().is_some_and(|p| p.outside_masks > 0),
            ),
            (
                DiffLayer::Dom,
                self.dom.as_ref().is_some_and(|d| !d.first_diffs.is_empty()),
            ),
            (
                DiffLayer::Meaning,
                self.meaning.as_ref().is_some_and(|m| !m.diffs.is_empty()),
            ),
            (DiffLayer::Behaviour, self.behaviour.iter().any(|b| !b.same)),
        ]
        .into_iter()
        .filter_map(|(layer, differs)| differs.then_some(layer))
        .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineInfo {
    pub tc4_commit: String,
    pub machine: Option<Machine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RustInfo {
    pub machine: Option<Machine>,
    pub build: Option<RustBuild>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub rows: usize,
    pub same: usize,
    pub declared: usize,
    pub differs: usize,
    pub unported: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub about: String,
    pub baseline: BaselineInfo,
    pub rust: Option<RustInfo>,
    pub totals: Totals,
    pub rows: Vec<Row>,
}

pub fn to_json(report: &Report) -> String {
    // The report holds only strings, numbers, booleans and maps with string keys.
    let mut text = serde_json::to_string_pretty(report).unwrap_or_default();
    text.push('\n');
    text
}

const ABOUT: &str = "What the four layers of APP-DESIGN §7.3 find for every baseline screen (manifest order) and then every case of the tC4 view-state gallery (cases.json order, APP-DESIGN §7.4): pixels, the DOM, what the DOM shows (meaning) and behaviour, between tC4's record (parity/tc4-baseline for a screen, parity/view-gallery for a case) and the Rust app's captures (captures.json). A case has no meaning layer. crates/tc-ui/tests/screen_record.rs computes it from the committed files and fails when this file differs; UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record rewrites it. Status: same, declared (every difference inside a mask of masks.json), differs (explained in residuals.json), unported (`missing` names the unit that captures each layer), failed (nothing excuses it).";

/// tC4's side of a row.
struct Reference<'a> {
    png: std::path::PathBuf,
    png_sha256: Option<&'a str>,
    dom: std::path::PathBuf,
    dom_sha256: Option<&'a str>,
    cards: Option<&'a BTreeMap<String, String>>,
    toast: Option<&'a SyncToast>,
    /// The machine that captured tC4's side: the baseline's for a screen, the gallery's for a
    /// case.
    machine: Option<&'a Machine>,
    /// The subtree a DOM-first capture compares: `scopes.json`'s for a screen, the case's own.
    scope: Option<&'a Scope>,
}

fn dom_failure(error: &DomError) -> FailureKind {
    match error {
        DomError::Format(_) => FailureKind::Capture,
        DomError::Scope(_) => FailureKind::Scope,
        DomError::Mask(_) => FailureKind::Mask,
    }
}

pub fn file_sha256(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Compares the layers `capture` claims against `reference`, filling `row`.
fn compare(
    record: &Record,
    row: &mut Row,
    capture: &Capture,
    reference: &Reference<'_>,
    diffs_out: Option<&Path>,
) -> Result<(), String> {
    let name = row.name.clone();
    let app = &record.paths.app;
    let rename = record.rename(&capture.scenario);
    let masks: Vec<_> = record.masks_of(&name).collect();
    if capture.claims(Layer::Pixels) {
        let tc4_machine = reference.machine;
        let rust_machine = record.captures.machine.as_ref();
        match (tc4_machine, rust_machine) {
            (Some(a), Some(b)) if a.comparable(b) => {}
            _ => {
                let say = |m: Option<&Machine>| {
                    m.map_or_else(|| "no machine".to_owned(), Machine::describe)
                };
                row.status = Status::Failed;
                row.failure = Some(FailureKind::Machine);
                row.error = Some(format!(
                    "pixels from another machine: tC4 {}, rust {}; recapture one side on the other's machine",
                    say(tc4_machine),
                    say(rust_machine)
                ));
                return Ok(());
            }
        }
        let rects: Vec<Rect> = masks.iter().map(|m| m.rects[&name]).collect();
        let rust_png = app.join(&capture.png);
        let compared = match pixels::compare_files(&reference.png, &rust_png, &rects)? {
            Ok(compared) => compared,
            Err(size) => {
                row.status = Status::Failed;
                row.failure = Some(FailureKind::Capture);
                row.error = Some(format!(
                    "the screenshot is another size than tC4's: {size}; capture it at the screen's viewport"
                ));
                return Ok(());
            }
        };
        let diff = (compared.count.mismatched > 0).then(|| format!("diffs/{name}.png"));
        if let (Some(dir), Some(_)) = (diffs_out, &diff) {
            let image = pixels::diff_image(
                &pixels::read(&reference.png)?,
                &pixels::read(&rust_png)?,
                &rects,
            );
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{name}.png")), pixels::encode(&image)?)
                .map_err(|e| e.to_string())?;
        }
        row.pixels = Some(PixelsRow {
            tc4_sha256: match reference.png_sha256 {
                Some(sha) => sha.to_owned(),
                None => file_sha256(&reference.png)?,
            },
            rust_sha256: capture.sha256.clone(),
            mismatched: compared.count.mismatched,
            outside_masks: compared.count.outside_masks,
            masks: compared
                .masks_used
                .iter()
                .map(|&i| masks[i].id.clone())
                .collect(),
            diff,
        });
    }
    let scope = if capture.claims(Layer::Pixels) {
        None
    } else {
        reference.scope
    };
    let dom_masks: Vec<DomMask> = masks
        .iter()
        .map(|m| DomMask {
            id: m.id.clone(),
            tc4: resolve_target(&m.target),
            rust: resolve_target(m.rust_target()),
            rust_absent: m.rust_absent.is_some(),
        })
        .collect();
    if capture.claims(Layer::Dom) {
        let tc4_dom: Snapshot = read_json(&reference.dom)?;
        let rust_dom: Snapshot = read_json(&app.join(&capture.dom))?;
        let tc4 = Side {
            name: "tC4",
            snapshot: &tc4_dom,
            rename: &[],
        };
        let rust = Side {
            name: "rust",
            snapshot: &rust_dom,
            rename: &rename,
        };
        match dom::compare(tc4, rust, scope, &dom_masks, &name) {
            Ok(found) => {
                row.dom = Some(DomRow {
                    tc4_sha256: match reference.dom_sha256 {
                        Some(sha) => sha.to_owned(),
                        None => file_sha256(&reference.dom)?,
                    },
                    rust_sha256: capture.dom_sha256.clone(),
                    scope: scope.cloned(),
                    same: found.same,
                    declared: found.declared,
                    first_diffs: found.first_diffs,
                });
            }
            Err(error) => {
                row.status = Status::Failed;
                row.failure = Some(dom_failure(&error));
                row.error = Some(error.message().to_owned());
                return Ok(());
            }
        }
    }
    if capture.claims(Layer::Meaning) {
        let declared: Vec<DeclaredCard> = masks
            .iter()
            .filter_map(|m| m.meaning.as_ref().map(|meaning| (m, meaning)))
            .flat_map(|(m, meaning)| {
                meaning
                    .home_cards
                    .iter()
                    .map(|(repo, (tc4, rust))| DeclaredCard {
                        mask: m.id.clone(),
                        repo: resolve(repo),
                        tc4: tc4.clone(),
                        rust: rust.clone(),
                    })
            })
            .collect();
        let empty = BTreeMap::new();
        let found = meaning::compare(
            reference.cards.unwrap_or(&empty),
            reference.toast,
            &capture.home_cards,
            capture.sync_toast.as_ref(),
            &rename,
            &declared,
        );
        row.meaning = Some(MeaningRow {
            same: found.same,
            declared: found.declared,
            diffs: found.diffs,
        });
    }
    let tc4_masks: Vec<(String, TargetSpec)> = dom_masks
        .iter()
        .map(|m| (m.id.clone(), m.tc4.clone()))
        .collect();
    let rust_masks: Vec<(String, TargetSpec)> = dom_masks
        .iter()
        .map(|m| (m.id.clone(), m.rust.clone()))
        .collect();
    for (sequence, tc4) in record
        .tc4_behaviour
        .iter()
        .filter(|(_, r)| r.screen == name)
    {
        if let Some(rust) = record.rust_behaviour.get(sequence) {
            row.behaviour.push(behaviour::compare(
                tc4,
                rust,
                &rename,
                scope,
                &tc4_masks,
                &rust_masks,
            ));
        }
    }
    let differs = !row.differing().is_empty();
    let declared = row.pixels.as_ref().is_some_and(|p| p.mismatched > 0)
        || row.dom.as_ref().is_some_and(|d| !d.declared.is_empty())
        || row.meaning.as_ref().is_some_and(|m| !m.declared.is_empty());
    row.status = if differs {
        Status::Differs
    } else if declared {
        Status::Declared
    } else {
        Status::Same
    };
    Ok(())
}

fn row_for(
    record: &Record,
    base: Row,
    units: &BTreeMap<Layer, UnitId>,
    capture: Option<&Capture>,
    pending_until: Option<&UnitId>,
    reference: &Reference<'_>,
    diffs_out: Option<&Path>,
) -> Result<Row, String> {
    let mut row = base;
    let Some(capture) = capture else {
        row.status = Status::Unported;
        row.missing = units.clone();
        return Ok(row);
    };
    row.missing = units
        .iter()
        .filter(|(layer, _)| !capture.claims(**layer))
        .map(|(l, u)| (*l, u.clone()))
        .collect();
    row.by = Some(capture.by.clone());
    row.backend = Some(capture.backend);
    row.pending_until = (capture.backend == Backend::MemoryScripted)
        .then(|| pending_until.cloned())
        .flatten();
    let error = capture
        .error
        .clone()
        .or_else(|| {
            capture
                .page_errors
                .first()
                .map(|e| format!("page error: {e}"))
        })
        .or_else(|| {
            capture
                .refused
                .first()
                .map(|u| format!("refused a request to another origin: {u}"))
        })
        .or_else(|| {
            capture
                .layers
                .is_empty()
                .then(|| "the capture claims no layer".to_owned())
        })
        .or_else(|| {
            (capture.kind == RowKind::Case && capture.claims(Layer::Meaning)).then(|| {
                "a gallery case has no meaning layer: capture it with --layers pixels,dom"
                    .to_owned()
            })
        });
    if let Some(error) = error {
        row.status = Status::Failed;
        row.failure = Some(FailureKind::Capture);
        row.error = Some(error);
        return Ok(row);
    }
    compare(record, &mut row, capture, reference, diffs_out)?;
    Ok(row)
}

/// Builds the report from the committed record. With `diffs_out`, also draws the image of
/// every row whose pixels differ there, and removes any other file there.
pub fn build(record: &Record, design: &Design, diffs_out: Option<&Path>) -> Result<Report, String> {
    let mut rows = Vec::new();
    for screen in &record.manifest.screens {
        let plan = design.screen(&screen.name);
        let units: BTreeMap<Layer, UnitId> = plan
            .map(|p| {
                Layer::ALL
                    .iter()
                    .map(|&l| (l, p.unit_of(l).clone()))
                    .collect()
            })
            .unwrap_or_default();
        let base = blank(
            &screen.name,
            RowKind::Screen,
            &screen.scenario,
            screen.locale(),
        );
        let reference = Reference {
            png: record
                .paths
                .baseline
                .join("screens")
                .join(format!("{}.png", screen.name)),
            png_sha256: Some(&screen.sha256),
            dom: record.paths.baseline.join(&screen.dom),
            dom_sha256: Some(&screen.dom_sha256),
            cards: Some(&screen.home_cards),
            toast: screen.sync_toast.as_ref(),
            machine: record.manifest.machine.as_ref(),
            scope: record.scopes.scopes.get(&screen.name),
        };
        let capture = record
            .captures
            .screens
            .iter()
            .find(|c| c.kind == RowKind::Screen && c.name == screen.name);
        rows.push(row_for(
            record,
            base,
            &units,
            capture,
            plan.and_then(|p| p.pending_until.as_ref()),
            &reference,
            diffs_out,
        )?);
    }
    for case in &record.cases.cases {
        let base = blank(&case.name, RowKind::Case, &case.scenario, &case.locale);
        let gallery = &record.paths.gallery;
        let reference = Reference {
            png: gallery.join(&case.png),
            png_sha256: Some(&case.sha256),
            dom: gallery.join(&case.dom),
            dom_sha256: Some(&case.dom_sha256),
            cards: None,
            toast: None,
            machine: record.cases.machine.as_ref(),
            scope: case.scope.as_ref(),
        };
        let capture = record
            .captures
            .screens
            .iter()
            .find(|c| c.kind == RowKind::Case && c.name == case.name);
        rows.push(row_for(
            record,
            base,
            &case.units(),
            capture,
            None,
            &reference,
            diffs_out,
        )?);
    }
    if let Some(dir) = diffs_out {
        let wanted: Vec<String> = rows
            .iter()
            .filter_map(|r| r.pixels.as_ref()?.diff.clone())
            .collect();
        for file in sorted_files(dir)? {
            if !wanted.contains(&format!("diffs/{file}")) {
                std::fs::remove_file(dir.join(&file)).map_err(|e| e.to_string())?;
            }
        }
    }
    let mut totals = Totals {
        rows: rows.len(),
        ..Totals::default()
    };
    for row in &rows {
        match row.status {
            Status::Same => totals.same += 1,
            Status::Declared => totals.declared += 1,
            Status::Differs => totals.differs += 1,
            Status::Unported => totals.unported += 1,
            Status::Failed => totals.failed += 1,
        }
    }
    Ok(Report {
        about: ABOUT.to_owned(),
        baseline: BaselineInfo {
            tc4_commit: record.manifest.tc4.commit.clone(),
            machine: record.manifest.machine.clone(),
        },
        rust: (!record.captures.screens.is_empty()).then(|| RustInfo {
            machine: record.captures.machine.clone(),
            build: record.captures.build.clone(),
        }),
        totals,
        rows,
    })
}

fn blank(name: &str, kind: RowKind, scenario: &str, locale: &str) -> Row {
    Row {
        name: name.to_owned(),
        kind,
        scenario: scenario.to_owned(),
        locale: locale.to_owned(),
        status: Status::Unported,
        missing: BTreeMap::new(),
        by: None,
        backend: None,
        pending_until: None,
        error: None,
        failure: None,
        pixels: None,
        dom: None,
        meaning: None,
        behaviour: Vec::new(),
    }
}
