//! `parity/ds-gallery` is the record that tc-ui draws what tC4 draws. The browser runs
//! (`capture.mjs`, `capture-rust.mjs`, `dom-diff.mjs`) need Chromium and a tC4 checkout, so CI
//! cannot rerun them; it can check that what they left is whole. A case without tC4's screen, or
//! missing from the committed reports, was never compared, and a gallery without the states the
//! tC4 delta asks for (docs/TC4-DELTA-2026-10-16.md §3.5) proves nothing about them.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

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

#[test]
fn every_case_has_tc4s_screen_and_was_compared_clean() {
    let spec: BTreeSet<String> = spec_cases().into_keys().collect();

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
        .collect();
    assert!(differ.is_empty(), "cases whose DOM differs: {differ:?}");
}
