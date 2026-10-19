//! `parity/ds-gallery` is the record that tc-ui draws what tC4 draws. The browser runs
//! (`capture.mjs`, `capture-rust.mjs`, `dom-diff.mjs`) need Chromium and a tC4 checkout, so CI
//! cannot rerun them; it can check that what they left is whole. A case without tC4's screen, or
//! missing from the committed reports, was never compared, and a gallery without the states the
//! tC4 delta asks for (docs/TC4-DELTA-2026-10-16.md §3.5) proves nothing about them.
//!
//! A case may differ from tC4 only as `residuals.json` declares it: a ruling docs/PLAN.md lists
//! among the differences from tC4 on purpose, the layers that differ, why, and the exact pairs of
//! DOM lines. A declared case that no longer differs, or differs in more than it declares, fails,
//! as `screen_record` holds `parity/app/residuals.json` (APP-DESIGN §7.5).
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

#[path = "support/plan_rulings.rs"]
mod plan_rulings;
use plan_rulings::plan_rulings;

fn gallery_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/ds-gallery")
}

fn json(name: &str) -> Value {
    let path = gallery_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn cases_of(report: &Value, name: &str) -> Vec<Value> {
    report["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("{name} has no `cases` array"))
        .clone()
}

fn id_of(case: &Value) -> String {
    case["id"]
        .as_str()
        .unwrap_or_else(|| panic!("a case without an id: {case}"))
        .to_string()
}

/// Case id -> `<html lang>` of every case in the spec.
fn spec_cases() -> BTreeMap<String, String> {
    cases_of(&json("gallery-spec.json"), "gallery-spec.json")
        .iter()
        .map(|c| {
            let lang = c["document"]["lang"].as_str().unwrap_or("en").to_string();
            (id_of(c), lang)
        })
        .collect()
}

/// The declared differences of `residuals.json`, by case id; none when the file is absent.
fn residuals() -> BTreeMap<String, Value> {
    if !gallery_dir().join("residuals.json").exists() {
        return BTreeMap::new();
    }
    match json("residuals.json") {
        Value::Object(map) => map.into_iter().collect(),
        other => panic!("residuals.json is not an object: {other}"),
    }
}

/// Whether a residual declares that its case differs in `layer` (`pixels` or `dom`).
fn declares(residual: Option<&Value>, layer: &str) -> bool {
    residual
        .and_then(|r| r["layers"].as_array())
        .is_some_and(|layers| layers.iter().any(|l| l.as_str() == Some(layer)))
}

/// The rulings docs/PLAN.md lists among the differences from tC4 on purpose, read as
/// `screen_record` reads them for its masks.
fn plan_differences() -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/PLAN.md");
    let plan = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let rulings = plan_rulings(&plan);
    assert!(
        !rulings.is_empty(),
        "docs/PLAN.md lists no differences from tC4 on purpose"
    );
    rulings
}

/// One side of a DOM line pair, trimmed; a side with no line reads `(none)`.
fn side(v: &Value) -> String {
    v.as_str()
        .map_or_else(|| "(none)".to_string(), |s| s.trim().to_string())
}

/// Everything wrong with the residual of case `id`, given its row in diff-report.json
/// (`pixels`), its row in dom-report.json (`dom`) and the rulings PLAN lists (`rulings`).
fn residual_problems(
    id: &str,
    residual: &Value,
    pixels: &Value,
    dom: &Value,
    rulings: &BTreeSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    if residual["why"].as_str().is_none_or(|w| w.trim().is_empty()) {
        out.push(format!("{id}: the residual says no why"));
    }
    match residual["ruling"].as_str() {
        None => out.push(format!("{id}: the residual names no ruling")),
        Some(ruling) if !rulings.contains(ruling) => out.push(format!(
            "{id}: docs/PLAN.md does not list ({ruling}) among the differences from tC4"
        )),
        Some(_) => {}
    }
    let layers: Vec<&str> = residual["layers"]
        .as_array()
        .map(|l| l.iter().map(|x| x.as_str().unwrap_or("?")).collect())
        .unwrap_or_default();
    let distinct: BTreeSet<&str> = layers.iter().copied().collect();
    if layers.is_empty()
        || distinct.len() != layers.len()
        || distinct.iter().any(|l| !matches!(*l, "pixels" | "dom"))
    {
        out.push(format!(
            "{id}: layers must be pixels and/or dom, each once: {layers:?}"
        ));
    }
    let mismatched = pixels["mismatched"].as_u64();
    if declares(Some(residual), "pixels") {
        if !mismatched.is_some_and(|n| n > 0) {
            out.push(format!(
                "{id}: declared to differ in pixels, but mismatched is {mismatched:?}"
            ));
        }
    } else if mismatched != Some(0) {
        out.push(format!(
            "{id}: {mismatched:?} pixels differ, and the residual does not declare pixels"
        ));
    }
    if dom["errors"].as_array().is_some_and(|e| !e.is_empty()) {
        out.push(format!(
            "{id}: the DOM run reported errors: {}",
            dom["errors"]
        ));
    }
    let same = dom["same"].as_bool();
    if declares(Some(residual), "dom") {
        if same != Some(false) {
            out.push(format!(
                "{id}: declared to differ in the DOM, but same is {same:?}"
            ));
        }
        let pairs = |v: &Value| -> Vec<(String, String)> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| (side(&p["tc4"]), side(&p["port"])))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (found, declared) = (pairs(&dom["lines"]), pairs(&residual["dom"]));
        if found != declared {
            let undeclared: Vec<_> = found.iter().filter(|p| !declared.contains(p)).collect();
            let gone: Vec<_> = declared.iter().filter(|p| !found.contains(p)).collect();
            out.push(format!(
                "{id}: the DOM differs in {} line pairs, the residual declares {}; \
                 the first undeclared: {:?}; declared but not found: {gone:?}",
                found.len(),
                declared.len(),
                &undeclared[..undeclared.len().min(3)],
            ));
        }
    } else if same != Some(true) {
        out.push(format!(
            "{id}: the DOM differs, and the residual does not declare dom"
        ));
    }
    out
}

/// The states §3.5 asks the design-system gallery to show, with the interface language each
/// is drawn in. The sync result toast (item 7) is not here: tC4 draws it from plain elements,
/// with no design-system component, so it is a tc-app screen case.
const DELTA_CASES: &[(&str, &str)] = &[
    // Item 1: the #737 rule for text selected in a target-language line.
    ("Tokens/selection", "en"),
    // Item 2: tiles, tabs and header exits that an editing card turns off.
    ("BookTile/tile-disabled", "en"),
    ("BookTile/row-disabled", "en"),
    ("BookTile/row-editing", "en"),
    ("Switcher/disabled-option", "en"),
    ("Switcher/pill-editing", "en"),
    ("AppHeader/project-editing", "en"),
    ("AppHeader/project-editing-later", "en"),
    ("AppHeader/project-exits-taken", "en"),
    // Item 3: a button that turns busy, a pill that reaches the end, a dialog that loses its ✕.
    ("Button/sync-busy", "en"),
    ("Button/sync-press", "en"),
    ("Button/nav-to-end", "en"),
    ("Modal/busy", "en"),
    ("Modal/busy-later", "en"),
    // Item 7: ItemNavButton in both directions.
    ("Button/nav-next", "en"),
    ("Button/nav-next-hover", "en"),
    ("Button/nav-next-end", "en"),
    ("Button/nav-prev-rtl", "ar"),
    // Item 7: the Draft section pill.
    ("Button/draft-section", "en"),
    ("Button/draft-section-hover", "en"),
    ("Button/draft-section-focus-visible", "en"),
    ("Button/draft-section-off", "en"),
    // Item 7: Open a project from Door43.
    ("Modal/open-door43-empty", "en"),
    ("Modal/open-door43-filled", "en"),
    ("Modal/open-door43-name-error", "en"),
    ("Modal/open-door43-refused", "en"),
    ("Modal/open-door43-refused-rtl", "ar"),
    ("Modal/open-door43-busy", "en"),
    // Item 7: Add a project with its four cards.
    ("Modal/add-project", "en"),
];

#[test]
fn the_gallery_draws_every_state_the_delta_asks_for() {
    let spec = spec_cases();
    let missing: Vec<_> = DELTA_CASES
        .iter()
        .filter(|(id, _)| !spec.contains_key(*id))
        .map(|(id, _)| *id)
        .collect();
    assert!(
        missing.is_empty(),
        "gallery-spec.json lacks these §3.5 states: {missing:?}"
    );
    for (id, lang) in DELTA_CASES {
        assert_eq!(
            spec[*id], *lang,
            "{id} is drawn in the wrong interface language"
        );
    }
}

/// The states a tC4 issue asks the gallery to show (docs/TC4-ISSUES.md §1), with the
/// interface language each is drawn in. Each differs from tC4 on purpose, so each has a residual.
const ISSUE_CASES: &[(&str, &str)] = &[
    // Row 13, tC4 #552: a Select turned off while its list is open, then its label pressed.
    ("Select/disabled-while-open", "en"),
    ("Select/disabled-while-open-label", "en"),
    // The same click dispatched by script, in one task with the switch-off: only the Select's
    // own check of `disabled` at the choice keeps the value.
    ("Select/disabled-while-open-script", "en"),
];

#[test]
fn the_gallery_draws_every_state_a_tc4_issue_asks_for() {
    let spec = spec_cases();
    let residuals = residuals();
    for (id, lang) in ISSUE_CASES {
        assert_eq!(
            spec.get(*id).map(String::as_str),
            Some(*lang),
            "{id} is missing or drawn in the wrong interface language"
        );
        assert!(
            residuals.contains_key(*id),
            "{id} differs from tC4 on purpose, and residuals.json does not declare it"
        );
    }
}

#[test]
fn every_declared_difference_names_exactly_what_differs() {
    let spec = spec_cases();
    let row_of = |name: &str| -> BTreeMap<String, Value> {
        cases_of(&json(name), name)
            .into_iter()
            .map(|c| (id_of(&c), c))
            .collect()
    };
    let (pixels, dom) = (row_of("diff-report.json"), row_of("dom-report.json"));
    let rulings = plan_differences();
    let mut problems = Vec::new();
    for (id, residual) in residuals() {
        if !spec.contains_key(&id) {
            problems.push(format!(
                "{id}: a residual for a case the spec does not have"
            ));
            continue;
        }
        problems.extend(residual_problems(
            &id,
            &residual,
            pixels.get(&id).unwrap_or(&Value::Null),
            dom.get(&id).unwrap_or(&Value::Null),
            &rulings,
        ));
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_declared_difference_that_is_gone_or_wider_fails() {
    use serde_json::json;
    let residual = json!({
        "ruling": "#552",
        "layers": ["pixels", "dom"],
        "why": "tC4 takes the choice; the port does not.",
        "dom": [{ "tc4": "\"Chosen: EST\"", "port": "\"Chosen: none\"" }],
    });
    let pixels = json!({ "id": "Select/x", "mismatched": 40 });
    let dom = json!({
        "id": "Select/x",
        "same": false,
        "errors": [],
        "lines": [{ "line": 15, "tc4": "      \"Chosen: EST\"", "port": "      \"Chosen: none\"" }],
    });
    let plan = &plan_rulings(&format!(
        "{DIFFERENCES}\n\n- A Select that turns disabled takes no choice (#552).\n"
    ));
    let problems = |r: &Value, p: &Value, d: &Value, plan: &BTreeSet<String>| {
        residual_problems("Select/x", r, p, d, plan)
    };
    assert_eq!(
        problems(&residual, &pixels, &dom, plan),
        Vec::<String>::new()
    );

    // The difference is gone: the port fell back to tC4's behaviour.
    let gone_dom = json!({ "id": "Select/x", "same": true, "errors": [], "diffs": [] });
    let gone_pixels = json!({ "id": "Select/x", "mismatched": 0 });
    assert!(!problems(&residual, &gone_pixels, &gone_dom, plan).is_empty());
    assert!(!problems(&residual, &pixels, &gone_dom, plan).is_empty());
    assert!(!problems(&residual, &gone_pixels, &dom, plan).is_empty());

    // The difference is wider: the port also left the list open.
    let mut wider = dom.clone();
    wider["lines"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "line": 16, "tc4": null, "port": "    <div role=\"listbox\">" }));
    assert!(!problems(&residual, &pixels, &wider, plan).is_empty());

    // A ruling PLAN does not list, a residual with no why, and an undeclared layer.
    let other = &plan_rulings(&format!("{DIFFERENCES}\n\n- Something else (#553).\n"));
    assert!(!problems(&residual, &pixels, &dom, other).is_empty());
    let mut no_why = residual.clone();
    no_why["why"] = json!(" ");
    assert!(!problems(&no_why, &pixels, &dom, plan).is_empty());
    let mut pixels_only = residual.clone();
    pixels_only["layers"] = json!(["pixels"]);
    assert!(!problems(&pixels_only, &pixels, &dom, plan).is_empty());
    let mut unknown_layer = residual.clone();
    unknown_layer["layers"] = json!(["pixels", "dom", "dom"]);
    assert!(!problems(&unknown_layer, &pixels, &dom, plan).is_empty());
}

/// The heading of the section of docs/PLAN.md that lists the differences from tC4 on purpose.
const DIFFERENCES: &str = "### Where the rebuild differs from tC4 4.0 on purpose";

#[test]
fn both_gates_read_an_owner_answer_and_a_tc4_issue_as_a_ruling() {
    let plan = format!(
        "## The goal\n\nNot a ruling here (Q9).\n\n{DIFFERENCES}\n\n\
         - A check conflict is shown (Q1, AGENTS.md).\n\
         - The aligner keeps verse order (Q15).\n\
         - Urdu (Devanagari) reads left to right (#701).\n\
         - A disabled Select takes no choice (#552, a declared difference).\n\
         - Mentioned in passing: issue #12, Q13 and (see #14 below).\n\n\
         ## Definition of done\n\nNot a ruling either (#99).\n"
    );
    let found: Vec<String> = plan_rulings(&plan).into_iter().collect();
    assert_eq!(found, ["#552", "#701", "Q1", "Q15"]);
    let real = plan_differences();
    for ruling in ["Q2", "Q3", "#549", "#552", "#701"] {
        assert!(
            real.contains(ruling),
            "docs/PLAN.md does not list {ruling}: {real:?}"
        );
    }
}

#[test]
fn every_case_has_tc4s_screen_and_was_compared_clean() {
    let spec: BTreeSet<String> = spec_cases().into_keys().collect();
    let residuals = residuals();

    let screens_dir = gallery_dir().join("screens");
    let mut screens = BTreeSet::new();
    for component in std::fs::read_dir(&screens_dir).expect("screens/ is readable") {
        let component = component.unwrap();
        for file in std::fs::read_dir(component.path()).unwrap() {
            let name = file.unwrap().file_name().into_string().unwrap();
            let state = name
                .strip_suffix(".png")
                .unwrap_or_else(|| panic!("not a screen: {name}"));
            screens.insert(format!(
                "{}/{state}",
                component.file_name().into_string().unwrap()
            ));
        }
    }
    assert_eq!(
        spec.difference(&screens).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "cases without tC4's screen: run capture.mjs"
    );
    assert_eq!(
        screens.difference(&spec).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "screens of no case"
    );

    let pixels = cases_of(&json("diff-report.json"), "diff-report.json");
    let pixel_ids: BTreeSet<String> = pixels.iter().map(id_of).collect();
    assert_eq!(
        pixel_ids, spec,
        "diff-report.json is not of this spec: run capture-rust.mjs"
    );
    let off: Vec<_> = pixels
        .iter()
        .filter(|c| c["mismatched"].as_u64() != Some(0))
        .map(id_of)
        .filter(|id| !declares(residuals.get(id), "pixels"))
        .collect();
    assert!(off.is_empty(), "cases with mismatched pixels: {off:?}");

    let dom = cases_of(&json("dom-report.json"), "dom-report.json");
    let dom_ids: BTreeSet<String> = dom.iter().map(id_of).collect();
    assert_eq!(
        dom_ids, spec,
        "dom-report.json is not of this spec: run dom-diff.mjs"
    );
    let differ: Vec<_> = dom
        .iter()
        .filter(|c| c["same"].as_bool() != Some(true))
        .map(id_of)
        .filter(|id| !declares(residuals.get(id), "dom"))
        .collect();
    assert!(differ.is_empty(), "cases whose DOM differs: {differ:?}");
}
