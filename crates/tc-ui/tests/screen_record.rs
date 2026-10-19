//! The gate of P7 for the app's screens (APP-DESIGN §7.5). Every run recomputes the four layers
//! (pixels, DOM, what the DOM shows, behaviour) from the committed captures of tC4
//! (`parity/tc4-baseline`) and of the Rust app (`parity/app`), checks that the committed
//! `parity/app/report.json` is exactly what they give, and applies the rules of `gate.rs`:
//! every baseline screen has a row, a row that differs is explained in `residuals.json` with
//! exactly the layers it differs in, a failed screen fails whatever is written there, and a
//! unit that has landed has captured its layers. `UPDATE_REPORT=1 cargo test -p tc-ui --test
//! screen_record` rewrites the report and its diff images. The planted tests (`planted.rs`)
//! prove that a planted pixel, DOM, meaning or behaviour difference and a screen that reached
//! the settle deadline fail it.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod screen_record {
    pub mod behaviour;
    pub mod design;
    pub mod dom;
    pub mod gate;
    pub mod inputs;
    pub mod meaning;
    pub mod pixels;
    mod planted;
    pub mod report;
}

use screen_record::design::{self, Backend, Layer};
use screen_record::inputs::{Paths, Record};
use screen_record::{gate, report};

fn doc(paths: &Paths) -> design::Design {
    let text = std::fs::read_to_string(&paths.doc).unwrap();
    design::parse(&text).unwrap_or_else(|e| panic!("docs/APP-DESIGN.md: {e}"))
}

#[test]
fn the_committed_record_passes_the_gate() {
    let paths = Paths::committed();
    let record = Record::load(paths.clone()).unwrap_or_else(|e| panic!("{e}"));
    let design = doc(&paths);
    let file = paths.app.join("report.json");
    let update = std::env::var_os("UPDATE_REPORT").is_some();
    let diffs = paths.app.join("diffs");
    let now = report::build(&record, &design, update.then_some(diffs.as_path()))
        .unwrap_or_else(|e| panic!("{e}"));
    if update {
        std::fs::write(&file, report::to_json(&now)).unwrap();
    }
    let committed = std::fs::read_to_string(&file).unwrap_or_default();
    let failures = gate::check(&record, &design, &committed, &now);
    let t = &now.totals;
    assert!(
        failures.is_empty(),
        "{} failures of the parity gate ({} rows: {} same, {} declared, {} differ, {} unported, {} failed):\n{}",
        failures.len(),
        t.rows,
        t.same,
        t.declared,
        t.differs,
        t.unported,
        t.failed,
        failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn section_7_6_names_every_baseline_screen_with_its_units() {
    let paths = Paths::committed();
    let design = doc(&paths);
    let record = Record::load(paths).unwrap();
    let plan: Vec<_> = design
        .screens
        .iter()
        .map(|s| (s.name.as_str(), s.scenario.as_str(), s.locale.as_str()))
        .collect();
    let mut baseline: Vec<_> = record
        .manifest
        .screens
        .iter()
        .map(|s| (s.name.as_str(), s.scenario.as_str(), s.locale()))
        .collect();
    let mut sorted_plan = plan.clone();
    sorted_plan.sort_unstable();
    baseline.sort_unstable();
    assert_eq!(sorted_plan, baseline);
    assert_eq!(plan.len(), 62);

    let menu = design.screen("account-menu").unwrap();
    assert_eq!(
        (
            menu.unit_of(Layer::Dom).as_str(),
            menu.unit_of(Layer::Pixels).as_str()
        ),
        ("A5", "B2")
    );
    assert_eq!(menu.unit_of(Layer::Meaning).as_str(), "B2");
    let ok = design.screen("sync-ok").unwrap();
    assert_eq!(ok.backend, Backend::MemoryScripted);
    assert_eq!(ok.pending_until.as_ref().map(|u| u.as_str()), Some("E1a"));
    assert_eq!(
        design
            .screen("home")
            .unwrap()
            .recapture
            .as_ref()
            .map(|u| u.as_str()),
        Some("C1a")
    );
    assert_eq!(design.screen("ar-check-tw-session").unwrap().locale, "ar");
    let scripted: Vec<_> = design
        .screens
        .iter()
        .filter(|s| s.backend == Backend::MemoryScripted)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        scripted,
        [
            "open-door43-refused",
            "sync-failed",
            "sync-ok",
            "sync-ok-teammates",
            "sync-ok-restored"
        ]
    );
    let dom_first: Vec<_> = design
        .screens
        .iter()
        .filter(|s| s.dom_first())
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        dom_first,
        [
            "account-menu",
            "app-language",
            "help",
            "about",
            "feedback",
            "share-internet-consent",
            "share-signin",
            "open-door43-internet-consent"
        ]
    );
    for unit in ["A1", "A4a", "A6b", "B1a", "C7b", "E1a", "G0", "G2"] {
        assert!(
            design.units.iter().any(|u| u.as_str() == unit),
            "§10 has no {unit}"
        );
    }
    assert_eq!(design.units.len(), 48);
}
