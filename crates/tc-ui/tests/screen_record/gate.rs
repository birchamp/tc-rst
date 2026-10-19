//! The gate of P7 (APP-DESIGN 7.5): the rules a committed record must keep. Each failure says
//! what to run or edit.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use regex::Regex;

use super::design::{Backend, Design, DiffLayer, Layer, UnitId};
use super::inputs::{Record, RowKind, sorted_files};
use super::pixels;
use super::report::{FailureKind, Report, Status, file_sha256, to_json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// §7.6 and the baseline's manifest disagree on a screen, its scenario or its language.
    PlanDisagreesWithBaseline,
    /// An id that is no unit of §10.
    UnknownUnit,
    DuplicateUnit,
    /// A baseline screen or gallery case without its row.
    MissingRow,
    /// A row or a capture of no baseline screen or gallery case.
    ExtraRow,
    /// The committed report is not what the committed captures give.
    StaleReport,
    /// A capture error, a page error, a view or boot error, a refused request or the settle
    /// deadline. Nothing excuses it.
    CaptureFailed,
    /// Pixels captured on another machine than tC4's, or on one with no font list.
    OtherMachine,
    DiffersWithoutResidual,
    /// A residual that does not name exactly the layers its row differs in.
    ResidualLayers,
    /// A residual for a row that does not differ.
    StaleResidual,
    /// A committed file is not the one its record names by sha256.
    HashMismatch,
    /// A file no capture, recording or row names.
    StrayFile,
    /// A diff image that is missing or not what the gate draws.
    StaleDiff,
    /// A layer not captured although the unit that captures it has landed.
    LayerOverdue,
    CapturedByUnlandedUnit,
    /// A scripted capture after the unit that must recapture it has landed.
    ScriptedOverdue,
    /// A capture on a backend that §7.6 does not give the screen.
    WrongBackend,
    MaskRuling,
    MaskRect,
    /// The rebuild's masked element lies outside the mask's rectangle.
    MaskMoved,
    MaskTargetMissing,
    /// A DOM-first scope that matches nothing.
    ScopeMissing,
    /// A scope for a screen whose DOM and pixels come together.
    ScopeNotDomFirst,
    /// A Rust recording without tC4's, or of no baseline screen.
    BehaviourUnpaired,
    /// A gallery case whose group §7.4 does not list, or whose units are not its group's.
    CaseGroup,
    /// A DOM-first gallery case without the scope its DOM is compared in, or a scope on a case
    /// whose DOM and pixels one unit captures.
    CaseScope,
    /// Two rows of one name: a case named like a baseline screen or like another case.
    DuplicateRow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub row: Option<String>,
    pub rule: Rule,
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.row {
            Some(row) => write!(f, "{:?} [{row}]: {}", self.rule, self.message),
            None => write!(f, "{:?}: {}", self.rule, self.message),
        }
    }
}

struct Failures(Vec<Failure>);

impl Failures {
    fn add(&mut self, row: Option<&str>, rule: Rule, message: impl Into<String>) {
        self.0.push(Failure {
            row: row.map(str::to_owned),
            rule,
            message: message.into(),
        });
    }
}

const UPDATE: &str =
    "run `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` and review the diff";

/// The rulings docs/PLAN.md lists under "Where the rebuild differs from tC4 4.0 on purpose".
fn plan_rulings(plan: &str) -> BTreeSet<String> {
    let mut lines = plan.lines();
    lines
        .by_ref()
        .find(|l| l.starts_with("### Where the rebuild differs from tC4 4.0 on purpose"));
    let section: String = lines
        .take_while(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    // A constant pattern.
    let ruling = Regex::new(r"\((Q\d+)[,)]").unwrap();
    ruling
        .captures_iter(&section)
        .map(|c| c[1].to_owned())
        .collect()
}

pub fn check(record: &Record, design: &Design, committed: &str, now: &Report) -> Vec<Failure> {
    let mut out = Failures(Vec::new());
    let manifest = &record.manifest;
    let landed: BTreeSet<&UnitId> = record.units.landed.iter().collect();

    // 1. §7.6 against the manifest, and each capture against both.
    for screen in &manifest.screens {
        match design.screen(&screen.name) {
            None => out.add(
                Some(&screen.name),
                Rule::PlanDisagreesWithBaseline,
                "the baseline screen is not in §7.6 of docs/APP-DESIGN.md",
            ),
            Some(plan) if plan.scenario != screen.scenario || plan.locale != screen.locale() => out
                .add(
                    Some(&screen.name),
                    Rule::PlanDisagreesWithBaseline,
                    format!(
                        "§7.6 gives scenario {} in {}, the manifest {} in {}",
                        plan.scenario,
                        plan.locale,
                        screen.scenario,
                        screen.locale()
                    ),
                ),
            Some(_) => {}
        }
    }
    for plan in &design.screens {
        if !manifest.screens.iter().any(|s| s.name == plan.name) {
            out.add(
                Some(&plan.name),
                Rule::PlanDisagreesWithBaseline,
                "§7.6 lists a screen the baseline does not have",
            );
        }
    }

    // 2. Units.
    let mut seen = BTreeSet::new();
    for unit in &record.units.landed {
        if !design.units.contains(unit) {
            out.add(
                None,
                Rule::UnknownUnit,
                format!("units.json lists {unit}, which is no unit of §10"),
            );
        }
        if !seen.insert(unit) {
            out.add(
                None,
                Rule::DuplicateUnit,
                format!("units.json lists {unit} twice"),
            );
        }
    }
    let mut named: Vec<(String, &UnitId)> = Vec::new();
    named.extend(
        record
            .captures
            .screens
            .iter()
            .map(|c| (format!("the capture of {}", c.name), &c.by)),
    );
    named.extend(record.residuals.rows.iter().filter_map(|(n, r)| {
        r.until
            .as_ref()
            .map(|u| (format!("the residual of {n}"), u))
    }));
    named.extend(record.residuals.general.iter().filter_map(|g| {
        g.until
            .as_ref()
            .map(|u| (format!("the residual {}", g.id), u))
    }));
    named.extend(record.cases.cases.iter().flat_map(|c| {
        [
            (format!("the case {}", c.name), &c.dom_unit),
            (format!("the case {}", c.name), &c.pixels_unit),
        ]
    }));
    for (what, unit) in named {
        if !design.units.contains(unit) {
            out.add(
                None,
                Rule::UnknownUnit,
                format!("{what} names {unit}, which is no unit of §10"),
            );
        }
    }

    // 3. One row per screen and case.
    let mut expected: Vec<(RowKind, &str)> = manifest
        .screens
        .iter()
        .map(|s| (RowKind::Screen, s.name.as_str()))
        .collect();
    expected.extend(
        record
            .cases
            .cases
            .iter()
            .map(|c| (RowKind::Case, c.name.as_str())),
    );
    for (kind, name) in &expected {
        if !now.rows.iter().any(|r| r.kind == *kind && r.name == *name) {
            out.add(Some(name), Rule::MissingRow, "no row in the report");
        }
    }
    for row in &now.rows {
        if !expected.contains(&(row.kind, row.name.as_str())) {
            out.add(
                Some(&row.name),
                Rule::ExtraRow,
                "a row of no baseline screen or gallery case",
            );
        }
    }
    for capture in &record.captures.screens {
        if !expected.contains(&(capture.kind, capture.name.as_str())) {
            out.add(
                Some(&capture.name),
                Rule::ExtraRow,
                "captures.json holds a capture of no baseline screen or gallery case",
            );
        }
        if let Some(case) = record
            .case(&capture.name)
            .filter(|_| capture.kind == RowKind::Case)
            && (case.scenario != capture.scenario || case.locale != capture.locale)
        {
            out.add(
                Some(&capture.name),
                Rule::PlanDisagreesWithBaseline,
                format!(
                    "captured in {} ({}); the gallery case is {} ({})",
                    capture.scenario, capture.locale, case.scenario, case.locale
                ),
            );
        }
        if let Some(screen) = manifest
            .screens
            .iter()
            .find(|s| s.name == capture.name && capture.kind == RowKind::Screen)
            && (screen.scenario != capture.scenario || screen.locale() != capture.locale)
        {
            out.add(
                Some(&capture.name),
                Rule::PlanDisagreesWithBaseline,
                format!(
                    "captured in {} ({}); the baseline screen is {} ({})",
                    capture.scenario,
                    capture.locale,
                    screen.scenario,
                    screen.locale()
                ),
            );
        }
    }

    // 3b. Each gallery case belongs to a group of §7.4, takes its units, has the scope a
    // DOM-first group needs, and has a name of its own.
    let mut case_names = BTreeSet::new();
    for case in &record.cases.cases {
        let name = Some(case.name.as_str());
        if manifest.screens.iter().any(|s| s.name == case.name) {
            out.add(
                name,
                Rule::DuplicateRow,
                "a gallery case named like a baseline screen: the two would share parity/app/screens/<name>.png; rename the case in parity/view-gallery/cases.mjs",
            );
        }
        if !case_names.insert(case.name.as_str()) {
            out.add(
                name,
                Rule::DuplicateRow,
                "two gallery cases share this name",
            );
        }
        match design.group(&case.group) {
            None => out.add(
                name,
                Rule::CaseGroup,
                format!(
                    "the case's group {} is not in §7.4 of docs/APP-DESIGN.md",
                    case.group
                ),
            ),
            Some(group) if group.dom != case.dom_unit || group.pixels != case.pixels_unit => out
                .add(
                    name,
                    Rule::CaseGroup,
                    format!(
                        "cases.json gives the DOM to {} and the pixels to {}; §7.4 gives group {} to {} and {}: capture the gallery again",
                        case.dom_unit, case.pixels_unit, group.name, group.dom, group.pixels
                    ),
                ),
            Some(_) => {}
        }
        match (case.dom_first(), &case.scope) {
            (true, None) => out.add(
                name,
                Rule::CaseScope,
                format!(
                    "a DOM-first case ({} before {}) names no scope: give it the dialog its DOM is compared in",
                    case.dom_unit, case.pixels_unit
                ),
            ),
            (false, Some(_)) => out.add(
                name,
                Rule::CaseScope,
                "a scope on a case whose DOM and pixels one unit captures: remove it",
            ),
            _ => {}
        }
    }

    // 4. The committed report.
    if committed != to_json(now) {
        out.add(
            None,
            Rule::StaleReport,
            format!("parity/app/report.json is not what the committed captures give: {UPDATE}"),
        );
    }

    // 5, 6. Failed rows, and rows that differ.
    for row in &now.rows {
        if row.status == Status::Failed {
            let rule = match row.failure {
                Some(FailureKind::Machine) => Rule::OtherMachine,
                Some(FailureKind::Mask) => Rule::MaskTargetMissing,
                Some(FailureKind::Scope) => Rule::ScopeMissing,
                Some(FailureKind::Capture) | None => Rule::CaptureFailed,
            };
            let error = row.error.as_deref().unwrap_or("failed");
            out.add(
                Some(&row.name),
                rule,
                format!("{error}; no residual excuses a failed screen: fix it and capture again"),
            );
        }
        let residual = record.residuals.rows.get(&row.name);
        match (row.status == Status::Differs, residual) {
            (true, None) => out.add(Some(&row.name), Rule::DiffersWithoutResidual, "the row differs: fix the port, or explain the difference in parity/app/residuals.json"),
            (true, Some(r)) => {
                if r.why.trim().is_empty() {
                    out.add(Some(&row.name), Rule::DiffersWithoutResidual, "its residual gives no reason");
                }
                let named: BTreeSet<DiffLayer> = r.layers.iter().copied().collect();
                let differing = row.differing();
                if named != differing {
                    out.add(Some(&row.name), Rule::ResidualLayers, format!("residuals.json explains the layers {}, but the row differs in {}: fix the port, or explain each layer that differs and no other", serde_json::to_string(&named).unwrap_or_default(), serde_json::to_string(&differing).unwrap_or_default()));
                }
            }
            (false, Some(_)) => out.add(Some(&row.name), Rule::StaleResidual, format!("residuals.json explains a row that is {:?}: remove the entry", row.status)),
            (false, None) => {}
        }
    }
    for name in record.residuals.rows.keys() {
        if !now.rows.iter().any(|r| &r.name == name) {
            out.add(
                Some(name),
                Rule::StaleResidual,
                "residuals.json explains a row that does not exist",
            );
        }
    }

    // 7. Hashes.
    let paths = &record.paths;
    for screen in &manifest.screens {
        for (file, want) in [
            (format!("screens/{}.png", screen.name), &screen.sha256),
            (screen.dom.clone(), &screen.dom_sha256),
        ] {
            match file_sha256(&paths.baseline.join(&file)) {
                Ok(have) if &have == want => {}
                Ok(have) => out.add(
                    Some(&screen.name),
                    Rule::HashMismatch,
                    format!(
                        "parity/tc4-baseline/{file} has sha256 {have}, the manifest says {want}"
                    ),
                ),
                Err(e) => out.add(Some(&screen.name), Rule::HashMismatch, e),
            }
        }
    }
    for case in &record.cases.cases {
        for (file, want) in case.files() {
            match file_sha256(&paths.gallery.join(file)) {
                Ok(have) if have == want => {}
                Ok(have) => out.add(Some(&case.name), Rule::HashMismatch, format!("parity/view-gallery/{file} has sha256 {have}, cases.json says {want}: capture the gallery again")),
                Err(e) => out.add(Some(&case.name), Rule::HashMismatch, e),
            }
        }
    }
    for capture in &record.captures.screens {
        for (file, want) in [
            (&capture.png, &capture.sha256),
            (&capture.dom, &capture.dom_sha256),
        ] {
            match file_sha256(&paths.app.join(file)) {
                Ok(have) if &have == want => {}
                Ok(have) => out.add(Some(&capture.name), Rule::HashMismatch, format!("parity/app/{file} has sha256 {have}, captures.json says {want}: capture it again")),
                Err(e) => out.add(Some(&capture.name), Rule::HashMismatch, e),
            }
        }
    }

    // 8. Stray files, and the diff images.
    let mut owned: BTreeSet<String> = BTreeSet::new();
    for capture in &record.captures.screens {
        owned.insert(capture.png.clone());
        owned.insert(capture.dom.clone());
    }
    owned.extend(
        record
            .rust_behaviour
            .keys()
            .map(|s| format!("behaviour/{s}.json")),
    );
    let diffs: BTreeMap<&str, String> = now
        .rows
        .iter()
        .filter_map(|r| {
            r.pixels
                .as_ref()?
                .diff
                .clone()
                .map(|d| (r.name.as_str(), d))
        })
        .collect();
    owned.extend(diffs.values().cloned());
    for dir in ["screens", "dom", "behaviour", "diffs"] {
        match sorted_files(&paths.app.join(dir)) {
            Ok(files) => {
                for file in files {
                    let rel = format!("{dir}/{file}");
                    if !owned.contains(&rel) {
                        out.add(None, Rule::StrayFile, format!("parity/app/{rel} belongs to no capture, recording or row: remove it"));
                    }
                }
            }
            Err(e) => out.add(None, Rule::StrayFile, e),
        }
    }
    let gallery_owned: BTreeSet<&str> = record
        .cases
        .cases
        .iter()
        .flat_map(|c| c.files().map(|(file, _)| file))
        .collect();
    for dir in ["screens", "dom", "actions"] {
        match sorted_files(&paths.gallery.join(dir)) {
            Ok(files) => {
                for file in files {
                    let rel = format!("{dir}/{file}");
                    if !gallery_owned.contains(rel.as_str()) {
                        out.add(None, Rule::StrayFile, format!("parity/view-gallery/{rel} belongs to no gallery case: capture the gallery again, which removes it"));
                    }
                }
            }
            Err(e) => out.add(None, Rule::StrayFile, e),
        }
    }
    match sorted_files(&paths.tc4_behaviour) {
        Ok(files) => {
            for file in files.iter().filter(|f| !f.ends_with(".json")) {
                out.add(
                    None,
                    Rule::StrayFile,
                    format!("parity/tc4-baseline/behaviour/{file} is not a recording"),
                );
            }
        }
        Err(e) => out.add(None, Rule::StrayFile, e),
    }
    for (name, rel) in &diffs {
        let Some(capture) = record.capture(name) else {
            continue;
        };
        let tc4_png = match manifest.screens.iter().find(|s| s.name == *name) {
            Some(_) => paths.baseline.join("screens").join(format!("{name}.png")),
            None => match record.case(name) {
                Some(case) => paths.gallery.join(&case.png),
                None => continue,
            },
        };
        let rects: Vec<_> = record.masks_of(name).map(|m| m.rects[*name]).collect();
        let drawn = pixels::read(&tc4_png)
            .and_then(|a| Ok((a, pixels::read(&paths.app.join(&capture.png))?)))
            .and_then(|(a, b)| pixels::encode(&pixels::diff_image(&a, &b, &rects)));
        match (drawn, std::fs::read(paths.app.join(rel))) {
            (Ok(drawn), Ok(committed)) if drawn == committed => {}
            (Err(e), _) => out.add(Some(name), Rule::StaleDiff, e),
            _ => out.add(
                Some(name),
                Rule::StaleDiff,
                format!(
                    "parity/app/{rel} is missing or not the image of this difference: {UPDATE}"
                ),
            ),
        }
    }

    // 9-11. What the landed units owe.
    for row in &now.rows {
        let which = match row.kind {
            RowKind::Screen => "--only",
            RowKind::Case => "--case",
        };
        for (layer, unit) in &row.missing {
            if landed.contains(unit) {
                out.add(
                    Some(&row.name),
                    Rule::LayerOverdue,
                    format!("{unit} has landed but the {layer:?} layer is not captured: `node parity/app/capture-rust.mjs --unit {unit} {which} {}`, then {UPDATE}", row.name),
                );
            }
        }
    }
    for (sequence, recording) in &record.tc4_behaviour {
        let unit = design
            .screen(&recording.screen)
            .map(|p| p.unit_of(Layer::Dom));
        if let Some(unit) = unit
            && landed.contains(unit)
            && !record.rust_behaviour.contains_key(sequence)
        {
            out.add(Some(&recording.screen), Rule::LayerOverdue, format!("{unit} has landed but sequence {sequence} has no Rust recording: `node parity/app/behaviour.mjs --env rust --only {sequence}`"));
        }
    }
    for capture in &record.captures.screens {
        if !landed.contains(&capture.by) {
            out.add(Some(&capture.name), Rule::CapturedByUnlandedUnit, format!("captured by {}, which units.json does not list as landed: the unit adds itself there", capture.by));
        }
        let Some(plan) = design
            .screen(&capture.name)
            .filter(|_| capture.kind == RowKind::Screen)
        else {
            continue;
        };
        let fits = match plan.backend {
            Backend::MemoryScripted => capture.backend != Backend::Memory,
            Backend::Memory | Backend::Store => capture.backend != Backend::MemoryScripted,
        };
        if !fits {
            out.add(
                Some(&capture.name),
                Rule::WrongBackend,
                format!(
                    "captured on {:?}; §7.6 gives the screen {:?}",
                    capture.backend, plan.backend
                ),
            );
        }
        if capture.backend == Backend::MemoryScripted
            && let Some(until) = &plan.pending_until
            && landed.contains(until)
        {
            out.add(Some(&capture.name), Rule::ScriptedOverdue, format!("still captured on scripted outcomes after {until} landed: {until} captures it again on its final backend"));
        }
    }

    // 12-14. Masks.
    let rulings = std::fs::read_to_string(&paths.plan)
        .map(|p| plan_rulings(&p))
        .unwrap_or_default();
    for mask in &record.masks.masks {
        if mask.ruling != "build" && !rulings.contains(&mask.ruling) {
            out.add(
                None,
                Rule::MaskRuling,
                format!(
                    "mask {} cites {}, which docs/PLAN.md does not list as a difference on purpose",
                    mask.id, mask.ruling
                ),
            );
        }
        if mask.why.trim().is_empty() {
            out.add(
                None,
                Rule::MaskRuling,
                format!("mask {} gives no reason", mask.id),
            );
        }
        for (screen_name, rect) in &mask.rects {
            let view = if let Some(screen) =
                manifest.screens.iter().find(|s| &s.name == screen_name)
            {
                if mask.ruling.starts_with('Q')
                    && screen.rebuild_differs.as_ref().map(|r| &r.ruling) != Some(&mask.ruling)
                {
                    out.add(Some(screen_name), Rule::MaskRuling, format!("mask {} cites {} on a screen the baseline does not mark as differing under it", mask.id, mask.ruling));
                }
                screen.viewport.unwrap_or(manifest.viewport)
            } else if let Some(case) = record.case(screen_name) {
                if mask.ruling.starts_with('Q') {
                    out.add(Some(screen_name), Rule::MaskRuling, format!("mask {} cites {} on a gallery case; an owner ruling's mask covers baseline screens only", mask.id, mask.ruling));
                }
                case.viewport.unwrap_or(manifest.viewport)
            } else {
                out.add(
                    None,
                    Rule::MaskRect,
                    format!(
                        "mask {} names {screen_name}, which is no baseline screen or case",
                        mask.id
                    ),
                );
                continue;
            };
            if rect.width == 0
                || rect.height == 0
                || rect.x + rect.width > view.width
                || rect.y + rect.height > view.height
            {
                out.add(Some(screen_name), Rule::MaskRect, format!("mask {} has an empty rectangle or one outside the {}x{} window: run parity/app/measure-masks.mjs", mask.id, view.width, view.height));
            }
        }
    }
    for screen in &manifest.screens {
        if let Some(differs) = &screen.rebuild_differs
            && !record
                .masks_of(&screen.name)
                .any(|m| m.ruling == differs.ruling)
        {
            out.add(
                Some(&screen.name),
                Rule::MaskRuling,
                format!(
                    "the rebuild differs here under {}, but no mask citing it covers the screen",
                    differs.ruling
                ),
            );
        }
    }
    for row in now
        .rows
        .iter()
        .filter(|r| r.status != Status::Failed && r.pixels.is_some())
    {
        let Some(capture) = record.capture(&row.name) else {
            continue;
        };
        for mask in record.masks_of(&row.name) {
            match capture.mask_rects.get(&mask.id).copied().flatten() {
                Some(rect) if rect.inside(&mask.rects[&row.name]) => {}
                Some(rect) => out.add(
                    Some(&row.name),
                    Rule::MaskMoved,
                    format!("the element of mask {} is at {rect:?}, outside its rectangle {:?}: widen the mask in masks.json, citing its ruling", mask.id, mask.rects[&row.name]),
                ),
                None => out.add(Some(&row.name), Rule::MaskTargetMissing, format!("the capture did not find the element of mask {} on the Rust page: set the mask's `rust` target", mask.id)),
            }
        }
    }

    // 16, 17. Scopes and recordings.
    for name in record.scopes.scopes.keys() {
        match design.screen(name) {
            Some(plan) if plan.dom_first() => {}
            Some(_) => out.add(Some(name), Rule::ScopeNotDomFirst, "scopes.json limits the DOM of a screen whose DOM and pixels are captured together: remove the scope"),
            None => out.add(Some(name), Rule::ScopeNotDomFirst, "scopes.json names no screen of §7.6"),
        }
    }
    for (sequence, recording) in &record.rust_behaviour {
        if !record.tc4_behaviour.contains_key(sequence) {
            out.add(Some(&recording.screen), Rule::BehaviourUnpaired, format!("sequence {sequence} has no tC4 recording: `node parity/app/behaviour.mjs --env tc4 --only {sequence}`"));
        }
    }
    for (sequence, recording) in record.tc4_behaviour.iter().chain(&record.rust_behaviour) {
        if !manifest.screens.iter().any(|s| s.name == recording.screen) {
            out.add(
                None,
                Rule::BehaviourUnpaired,
                format!(
                    "sequence {sequence} starts from {}, which is no baseline screen",
                    recording.screen
                ),
            );
        }
    }

    out.0
        .sort_by(|a, b| (a.rule, &a.row, &a.message).cmp(&(b.rule, &b.row, &b.message)));
    out.0.dedup();
    out.0
}
