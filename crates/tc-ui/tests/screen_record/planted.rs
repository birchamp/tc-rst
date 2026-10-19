//! The planted tests: the gate run on the real tC4 baseline, docs and masks, with Rust captures
//! planted in a temporary `parity/app`. An identity capture is tC4's own screenshot, and tC4's
//! DOM rewritten as the Rust app writes it (project folders for repo paths, other generated id
//! numbers, its own asset paths); it must come out `same`, and one planted difference must
//! fail the gate.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use super::behaviour::{Recording, Step};
use super::design::{self, Design, Layer};
use super::dom::{self, DomError, DomMask, Side, Snapshot};
use super::gate::{self, Failure, Rule};
use super::inputs::{Paths, Record, Scope, TargetSpec, Which, read_json};
use super::pixels;
use super::report::{self, Report, Row, Status, file_sha256};

static DESIGN: LazyLock<Design> = LazyLock::new(|| {
    design::parse(&std::fs::read_to_string(Paths::committed().doc).unwrap()).unwrap()
});

/// The Rust app's folders for tC4's two projects (parity/app/env-rust.mjs `PROJECTS`).
const FOLDERS: [(&str, &str); 2] = [
    ("_local_/_local_/sample_burrito", "sample_burrito"),
    ("_local_/_local_/sample_burrito_obs", "sample_burrito_obs"),
];

static DLG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"tc-dlg(\d+)").unwrap());
static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"tc-f(\d+)").unwrap());

/// tC4's DOM file as the Rust app would write the same screen.
fn as_rust_writes(tc4: &str) -> String {
    let text = tc4.replace("_local_/_local_/", "");
    let text = DLG.replace_all(&text, |c: &regex::Captures<'_>| {
        format!("tc-dlg{}", c[1].parse::<u32>().unwrap() + 4)
    });
    let text = FIELD.replace_all(&text, |c: &regex::Captures<'_>| {
        format!("tc-f{}", c[1].parse::<u32>().unwrap() + 7)
    });
    text.replace("/clients/uw-tc4/assets/", "/ui/0123456789abcdef/images/")
        .replace("/clients/uw-tc4/help/", "/help/")
}

fn rename_keys(cards: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (repo, action) in cards.as_object().into_iter().flatten() {
        let folder = FOLDERS
            .iter()
            .find(|(r, _)| r == repo)
            .map_or(repo.as_str(), |(_, f)| f);
        out.insert(folder.to_owned(), action.clone());
    }
    Value::Object(out)
}

struct Planted {
    dir: tempfile::TempDir,
    committed: Paths,
    manifest: Value,
    captures: Value,
    report_override: Option<String>,
}

struct Outcome {
    report: Report,
    failures: Vec<Failure>,
}

impl Outcome {
    fn row(&self, name: &str) -> &Row {
        self.report.rows.iter().find(|r| r.name == name).unwrap()
    }

    fn rules(&self, name: &str) -> Vec<Rule> {
        self.failures
            .iter()
            .filter(|f| f.row.as_deref() == Some(name))
            .map(|f| f.rule)
            .collect()
    }

    fn global(&self) -> Vec<&Failure> {
        self.failures.iter().filter(|f| f.row.is_none()).collect()
    }

    fn assert_clean(&self, names: &[&str]) {
        let mine: Vec<_> = self
            .failures
            .iter()
            .filter(|f| f.row.is_none() || names.contains(&f.row.as_deref().unwrap_or_default()))
            .collect();
        assert!(mine.is_empty(), "failures: {mine:#?}");
    }
}

impl Planted {
    fn new() -> Self {
        let committed = Paths::committed();
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "units.json",
            "cases.json",
            "scopes.json",
            "masks.json",
            "residuals.json",
        ] {
            std::fs::copy(committed.app.join(file), dir.path().join(file)).unwrap();
        }
        for sub in ["screens", "dom"] {
            std::fs::create_dir(dir.path().join(sub)).unwrap();
        }
        let manifest: Value = read_json(&committed.baseline.join("manifest.json")).unwrap();
        let projects: serde_json::Map<String, Value> = FOLDERS
            .iter()
            .map(|(r, f)| ((*r).to_owned(), json!(f)))
            .collect();
        let captures = json!({
            "about": "planted",
            "machine": manifest["machine"].clone(),
            "build": { "ui": "0123456789abcdef", "commit": "planted", "api": 1 },
            "projects": { "sample": projects, "en-pins": projects, "shared": projects },
            "screens": [],
        });
        Self {
            dir,
            committed,
            manifest,
            captures,
            report_override: None,
        }
    }

    fn app(&self) -> &Path {
        self.dir.path()
    }

    fn paths(&self) -> Paths {
        Paths {
            app: self.app().to_owned(),
            tc4_behaviour: self.app().join("tc4-behaviour"),
            ..self.committed.clone()
        }
    }

    fn baseline(&self, name: &str) -> Value {
        self.manifest["screens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
            .clone()
    }

    /// Plants the Rust app's capture of `name`, identical to tC4's screen.
    fn identity(&mut self, name: &str) -> &mut Value {
        let screen = self.baseline(name);
        std::fs::copy(
            self.committed.baseline.join(format!("screens/{name}.png")),
            self.app().join(format!("screens/{name}.png")),
        )
        .unwrap();
        let tc4_dom =
            std::fs::read_to_string(self.committed.baseline.join(format!("dom/{name}.json")))
                .unwrap();
        std::fs::write(
            self.app().join(format!("dom/{name}.json")),
            as_rust_writes(&tc4_dom),
        )
        .unwrap();
        let masks: Value = read_json(&self.app().join("masks.json")).unwrap();
        let rects: serde_json::Map<String, Value> = masks["masks"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| Some((m["id"].as_str()?.to_owned(), m["rects"].get(name)?.clone())))
            .collect();
        let mut toast = screen["syncToast"].clone();
        if let Some(t) = toast.as_object_mut() {
            t.remove("missingVersions");
        }
        let entry = json!({
            "name": name, "kind": "screen", "scenario": screen["scenario"], "locale": screen.get("locale").cloned().unwrap_or(json!("en")),
            "by": "A6b", "backend": "memory", "layers": ["pixels", "dom", "meaning"],
            "error": null, "pageErrors": [], "refused": [],
            "homeCards": rename_keys(&screen["homeCards"]), "syncToast": toast, "maskRects": rects,
            "png": format!("screens/{name}.png"), "sha256": "", "dom": format!("dom/{name}.json"), "domSha256": "",
        });
        self.captures["screens"].as_array_mut().unwrap().push(entry);
        self.rehash(name);
        self.entry(name)
    }

    fn entry(&mut self, name: &str) -> &mut Value {
        self.captures["screens"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["name"] == name)
            .unwrap()
    }

    fn rehash(&mut self, name: &str) {
        let png = file_sha256(&self.app().join(format!("screens/{name}.png"))).unwrap();
        let dom = file_sha256(&self.app().join(format!("dom/{name}.json"))).unwrap();
        let entry = self.entry(name);
        entry["sha256"] = json!(png);
        entry["domSha256"] = json!(dom);
    }

    fn edit_dom(&mut self, name: &str, edit: impl FnOnce(String) -> String) {
        let file = self.app().join(format!("dom/{name}.json"));
        let text = std::fs::read_to_string(&file).unwrap();
        let edited = edit(text.clone());
        assert_ne!(edited, text, "the edit changed nothing");
        std::fs::write(&file, edited).unwrap();
        self.rehash(name);
    }

    fn edit_png(&mut self, name: &str, edit: impl FnOnce(&mut pixels::Image)) {
        let file = self.app().join(format!("screens/{name}.png"));
        let mut image = pixels::read(&file).unwrap();
        edit(&mut image);
        std::fs::write(&file, pixels::encode(&image).unwrap()).unwrap();
        self.rehash(name);
    }

    fn edit_json(&self, file: &str, edit: impl FnOnce(&mut Value)) {
        let path = self.app().join(file);
        let mut value: Value = read_json(&path).unwrap();
        edit(&mut value);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        )
        .unwrap();
    }

    fn run_with(&self, diffs: bool) -> Outcome {
        std::fs::write(
            self.app().join("captures.json"),
            serde_json::to_string_pretty(&self.captures).unwrap(),
        )
        .unwrap();
        let record = Record::load(self.paths()).unwrap_or_else(|e| panic!("{e}"));
        let diffs_dir = self.app().join("diffs");
        let now = report::build(&record, &DESIGN, diffs.then_some(diffs_dir.as_path()))
            .unwrap_or_else(|e| panic!("{e}"));
        let committed = self
            .report_override
            .clone()
            .unwrap_or_else(|| report::to_json(&now));
        let failures = gate::check(&record, &DESIGN, &committed, &now);
        Outcome {
            report: now,
            failures,
        }
    }

    /// The gate over the planted record, its report written as UPDATE_REPORT writes it.
    fn run(&self) -> Outcome {
        self.run_with(true)
    }
}

fn flip_red(image: &mut pixels::Image, x: u32, y: u32) {
    let at = ((y * image.width + x) * 4) as usize;
    image.rgba[at] ^= 0xff;
}

fn about_build_centre(p: &Planted) -> (u32, u32) {
    let masks: Value = read_json(&p.app().join("masks.json")).unwrap();
    let rect = &masks["masks"][0]["rects"]["about"];
    assert_eq!(masks["masks"][0]["id"], "about-build");
    let at = |k: &str| u32::try_from(rect[k].as_u64().unwrap()).unwrap();
    (at("x") + at("width") / 2, at("y") + at("height") / 2)
}

// ---- the gate ------------------------------------------------------------------------------

#[test]
fn an_identical_capture_is_same_on_every_layer() {
    let mut p = Planted::new();
    p.identity("about");
    p.identity("shared-home");
    let out = p.run();
    for name in ["about", "shared-home"] {
        let row = out.row(name);
        assert_eq!(row.status, Status::Same, "{row:#?}");
        assert!(row.missing.is_empty());
        assert_eq!(row.pixels.as_ref().unwrap().mismatched, 0);
        assert!(row.dom.as_ref().unwrap().same);
        assert!(row.meaning.as_ref().unwrap().same);
    }
    out.assert_clean(&["about", "shared-home"]);
    assert_eq!(out.report.totals.same, 2);
    assert_eq!(out.report.totals.unported, 60);
}

#[test]
fn a_planted_pixel_fails_the_gate() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_png("about", |image| flip_red(image, 5, 5));
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Differs);
    let pixels = row.pixels.as_ref().unwrap();
    assert_eq!((pixels.mismatched, pixels.outside_masks), (1, 1));
    assert_eq!(pixels.diff.as_deref(), Some("diffs/about.png"));
    let diff = pixels::read(&p.app().join("diffs/about.png")).unwrap();
    assert_eq!(
        &diff.rgba[(5 * diff.width as usize + 5) * 4..][..4],
        &[255, 0, 0, 255],
        "the differing pixel is red in the diff"
    );
    assert_eq!(out.rules("about"), [Rule::DiffersWithoutResidual]);
    assert!(out.global().is_empty(), "{:#?}", out.global());
    // Without its diff image the row is not ready for review.
    std::fs::remove_file(p.app().join("diffs/about.png")).unwrap();
    assert!(p.run_with(false).rules("about").contains(&Rule::StaleDiff));
}

#[test]
fn a_planted_pixel_inside_its_mask_is_declared() {
    let mut p = Planted::new();
    p.identity("about");
    let (x, y) = about_build_centre(&p);
    p.edit_png("about", |image| flip_red(image, x, y));
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Declared, "{row:#?}");
    let pixels = row.pixels.as_ref().unwrap();
    assert_eq!((pixels.mismatched, pixels.outside_masks), (1, 0));
    assert_eq!(pixels.masks, ["about-build"]);
    out.assert_clean(&["about"]);
}

#[test]
fn a_planted_dom_difference_fails_the_gate() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_dom("about", |t| {
        t.replacen(r#"data-tone=\"neutral\""#, r#"data-tone=\"accent\""#, 1)
    });
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Differs);
    let dom = row.dom.as_ref().unwrap();
    assert!(!dom.same);
    let first = &dom.first_diffs[0];
    assert!(
        first.contains(r#"tC4 "#)
            && first.contains(r#"data-tone="neutral""#)
            && first.contains(r#"| rust "#)
            && first.contains(r#"data-tone="accent""#),
        "{first}"
    );
    assert_eq!(dom.first_diffs.len(), 1);
    assert_eq!(out.rules("about"), [Rule::DiffersWithoutResidual]);
}

#[test]
fn a_changed_test_id_is_not_hidden_by_the_renaming() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_dom("about", |t| {
        t.replace(
            r#"data-testid=\"about-copyright\""#,
            r#"data-testid=\"about-copy\""#,
        )
    });
    let out = p.run();
    assert_eq!(out.row("about").status, Status::Differs);
    assert!(out.row("about").dom.as_ref().unwrap().first_diffs[0].contains("about-copy\""));
}

#[test]
fn a_planted_meaning_difference_fails_the_gate() {
    let mut p = Planted::new();
    p.identity("shared-home")["homeCards"]["sample_burrito"] = json!("share");
    let out = p.run();
    let row = out.row("shared-home");
    assert_eq!(row.status, Status::Differs);
    let meaning = row.meaning.as_ref().unwrap();
    assert!(!meaning.same);
    assert_eq!(
        meaning.diffs,
        ["homeCards[_local_/_local_/sample_burrito]: tC4 sync, rust share"]
    );
    assert_eq!(out.rules("shared-home"), [Rule::DiffersWithoutResidual]);
}

#[test]
fn the_q2_card_is_a_declared_difference() {
    let mut p = Planted::new();
    p.identity("shared-home")["homeCards"]["sample_burrito_obs"] = json!("sync");
    p.edit_dom("shared-home", |t| {
        let card = r#"<div data-action=\"upload\" data-shared=\"1\" data-testid=\"share-card-sample_burrito_obs\">"#;
        assert!(t.contains(card));
        t.replace(card, &card.replace("upload", "sync"))
            .replace(r#"data-testid=\"share-sample_burrito_obs\" data-tone=\"ocean\" data-trim=\"cap\" title=\"Upload changes\""#, r#"data-testid=\"share-sample_burrito_obs\" data-tone=\"ocean\" data-trim=\"cap\" title=\"Sync with Door43\""#)
    });
    let out = p.run();
    let row = out.row("shared-home");
    assert_eq!(row.status, Status::Declared, "{row:#?}");
    assert_eq!(row.meaning.as_ref().unwrap().declared, ["q2-obs-card"]);
    assert_eq!(row.dom.as_ref().unwrap().declared, ["q2-obs-card"]);
    out.assert_clean(&["shared-home"]);
}

const DEADLINE: &str = "the app did not go quiet for 800 ms within 30 s; in flight: nothing now; the server answered busy (test/quiet)";

#[test]
fn a_screen_that_reached_the_settle_deadline_fails_even_with_a_residual() {
    let mut p = Planted::new();
    p.identity("about")["error"] = json!(DEADLINE);
    p.edit_json(
        "residuals.json",
        |r| {
            r["rows"]["about"] =
                json!({ "layers": ["pixels"], "why": "a known difference", "until": null })
        },
    );
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Failed);
    assert_eq!(row.error.as_deref(), Some(DEADLINE));
    assert!(
        row.pixels.is_none() && row.dom.is_none(),
        "a failed capture is not compared"
    );
    let rules = out.rules("about");
    assert!(rules.contains(&Rule::CaptureFailed), "{rules:?}");
    assert!(rules.contains(&Rule::StaleResidual), "{rules:?}");
}

#[test]
fn a_page_error_or_a_refused_request_fails_the_screen() {
    let mut p = Planted::new();
    p.identity("about")["pageErrors"] = json!(["RuntimeError: unreachable"]);
    p.identity("help")["refused"] = json!(["https://example.org/"]);
    let out = p.run();
    assert_eq!(out.row("about").status, Status::Failed);
    assert_eq!(
        out.row("about").error.as_deref(),
        Some("page error: RuntimeError: unreachable")
    );
    assert_eq!(out.row("help").status, Status::Failed);
    assert!(
        out.row("help")
            .error
            .as_deref()
            .unwrap()
            .contains("https://example.org/")
    );
    assert_eq!(out.rules("about"), [Rule::CaptureFailed]);
    assert_eq!(out.rules("help"), [Rule::CaptureFailed]);
}

#[test]
fn a_residual_excuses_only_a_row_that_differs() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_png("about", |image| flip_red(image, 5, 5));
    p.identity("shared-home");
    p.edit_json("residuals.json", |r| r["rows"]["about"] = json!({ "layers": ["pixels"], "why": "one pixel of the title bar, planted", "until": null }));
    p.run().assert_clean(&["about", "shared-home"]);
    p.edit_json(
        "residuals.json",
        |r| {
            r["rows"]["shared-home"] =
                json!({ "layers": ["dom"], "why": "nothing differs", "until": null })
        },
    );
    let out = p.run();
    assert_eq!(out.rules("shared-home"), [Rule::StaleResidual]);
    assert!(out.rules("about").is_empty());
}

#[test]
fn an_edited_report_fails() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_png("about", |image| flip_red(image, 5, 5));
    p.edit_json("residuals.json", |r| {
        r["rows"]["about"] = json!({ "layers": ["pixels"], "why": "planted", "until": null })
    });
    let honest = report::to_json(&p.run().report);
    assert!(honest.contains("\"mismatched\": 1,"));
    p.report_override = Some(honest.replace("\"mismatched\": 1,", "\"mismatched\": 0,"));
    let out = p.run();
    assert!(
        out.global().iter().any(|f| f.rule == Rule::StaleReport),
        "{:#?}",
        out.failures
    );
}

#[test]
fn pixels_from_another_machine_fail() {
    let mut p = Planted::new();
    p.identity("about");
    p.captures["machine"]["fonts"]["sha256"] = json!("0".repeat(64));
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Failed);
    assert!(
        row.error
            .as_deref()
            .unwrap()
            .starts_with("pixels from another machine: tC4 linux-x64 Chromium"),
        "{row:#?}"
    );
    assert_eq!(out.rules("about"), [Rule::OtherMachine]);
    // A machine that cannot list its fonts is never comparable, even with itself.
    p.captures["machine"]["fonts"] = Value::Null;
    assert_eq!(p.run().rules("about"), [Rule::OtherMachine]);
    // Its DOM and what the DOM shows still compare.
    p.entry("about")["layers"] = json!(["dom", "meaning"]);
    let out = p.run();
    assert_eq!(out.row("about").status, Status::Same);
}

#[test]
fn a_dom_only_capture_compares_the_dialog_alone() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    p.edit_dom("about", |t| {
        t.replacen(r#"title=\"Settings\""#, r#"title=\"Settingz\""#, 1)
    });
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Same, "{row:#?}");
    assert_eq!(
        row.dom.as_ref().unwrap().scope,
        Some(Scope {
            attr: "role".into(),
            value: "dialog".into(),
            which: Which::First
        })
    );
    let missing: Vec<_> = row.missing.iter().map(|(l, u)| (*l, u.as_str())).collect();
    assert_eq!(missing, [(Layer::Pixels, "B2"), (Layer::Meaning, "B2")]);
    out.assert_clean(&["about"]);
    p.entry("about")["layers"] = json!(["pixels", "dom", "meaning"]);
    assert_eq!(p.run().row("about").status, Status::Differs);
}

#[test]
fn a_layer_whose_unit_has_landed_must_be_captured() {
    let p = Planted::new();
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("B2"))
    });
    let out = p.run();
    assert_eq!(
        out.rules("home"),
        [Rule::LayerOverdue, Rule::LayerOverdue, Rule::LayerOverdue]
    );
    assert_eq!(
        out.rules("account-menu"),
        [Rule::LayerOverdue, Rule::LayerOverdue],
        "pixels and meaning are B2's; the DOM is A5's"
    );
    assert!(
        out.rules("understand-tit1").is_empty(),
        "B3's screen is not due"
    );
    let due = out
        .failures
        .iter()
        .filter(|f| f.rule == Rule::LayerOverdue)
        .filter_map(|f| f.row.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        due.len(),
        11,
        "the six B2 screens and the five whose pixels are B2's: {due:?}"
    );
}

#[test]
fn a_scripted_row_fails_once_its_recapture_unit_has_landed() {
    let mut p = Planted::new();
    p.identity("sync-ok")["backend"] = json!("memory+scripted");
    let out = p.run();
    assert_eq!(
        out.row("sync-ok")
            .pending_until
            .as_ref()
            .map(|u| u.as_str()),
        Some("E1a")
    );
    out.assert_clean(&["sync-ok"]);
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("E1a"))
    });
    let out = p.run();
    assert!(
        out.rules("sync-ok").contains(&Rule::ScriptedOverdue),
        "{:?}",
        out.rules("sync-ok")
    );
    // A scripted screen captured as if its outcome were real is refused too.
    p.entry("sync-ok")["backend"] = json!("memory");
    assert!(p.run().rules("sync-ok").contains(&Rule::WrongBackend));
}

#[test]
fn a_capture_by_a_unit_that_has_not_landed_fails() {
    let mut p = Planted::new();
    p.identity("about")["by"] = json!("B2");
    assert_eq!(p.run().rules("about"), [Rule::CapturedByUnlandedUnit]);
}

#[test]
fn a_masked_element_outside_its_rectangle_fails() {
    let mut p = Planted::new();
    let entry = p.identity("about");
    let x = entry["maskRects"]["about-build"]["x"].as_u64().unwrap();
    entry["maskRects"]["about-build"]["x"] = json!(x + 20);
    assert_eq!(p.run().rules("about"), [Rule::MaskMoved]);
    p.entry("about")["maskRects"]["about-build"] = Value::Null;
    assert_eq!(p.run().rules("about"), [Rule::MaskTargetMissing]);
}

#[test]
fn every_screen_the_rebuild_differs_on_has_a_mask_citing_its_ruling() {
    let p = Planted::new();
    p.run().assert_clean(&[]);
    assert!(p.run().failures.iter().all(|f| f.rule != Rule::MaskRuling));
    p.edit_json("masks.json", |m| {
        m["masks"]
            .as_array_mut()
            .unwrap()
            .retain(|m| m["id"] != "q3-where")
    });
    let out = p.run();
    for screen in ["open-door43", "open-door43-internet-consent"] {
        assert_eq!(out.rules(screen), [Rule::MaskRuling], "{screen}");
    }
    let p = Planted::new();
    p.edit_json("masks.json", |m| {
        m["masks"][2]["rects"]["about"] = json!({ "x": 1, "y": 1, "width": 2, "height": 2 })
    });
    assert_eq!(p.run().rules("about"), [Rule::MaskRuling]);
    let p = Planted::new();
    p.edit_json("masks.json", |m| m["masks"][0]["ruling"] = json!("Q99"));
    assert!(p.run().global().iter().any(|f| f.rule == Rule::MaskRuling));
}

/// tC4's DOM file of `name` as a behaviour step, each element numbered.
fn numbered(file: &Path) -> Snapshot {
    let mut snapshot: Snapshot = read_json(file).unwrap();
    snapshot.screen = None;
    for (n, line) in snapshot.root.iter_mut().enumerate() {
        if line.trim_start().starts_with('<') {
            line.truncate(line.len() - 1);
            line.push_str(&format!(" #{n}>"));
        }
    }
    snapshot
}

fn recording(snapshot: &Snapshot) -> Recording {
    let step = |label: &str| Step {
        label: label.to_owned(),
        dom: snapshot.clone(),
        focus: "button \"✕\"".to_owned(),
        overflow: "hidden".to_owned(),
    };
    Recording {
        sequence: "about-keys".to_owned(),
        screen: "about".to_owned(),
        steps: vec![
            step("start"),
            step(r#"{"key":"Tab"}"#),
            step(r#"{"key":"Tab"}"#),
        ],
    }
}

fn write_recording(dir: &Path, recording: &Recording) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.json", recording.sequence)),
        serde_json::to_string_pretty(recording).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_planted_behaviour_difference_fails_the_gate() {
    let mut p = Planted::new();
    p.identity("about");
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    assert!(
        out.row("about").behaviour[0].same,
        "{:#?}",
        out.row("about").behaviour
    );
    out.assert_clean(&["about"]);

    // Step 2 has the focus on another element on the Rust side.
    let root = &mut rust.steps[2].dom.root;
    let focused = root.iter().position(|l| l.contains(" .focused")).unwrap();
    root[focused] = root[focused].replace(" .focused", "");
    let other = root
        .iter()
        .position(|l| l.contains("about-read-license"))
        .unwrap();
    root[other] = root[other].replace(" #", " .focused #");
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let result = &out.row("about").behaviour[0];
    assert!(!result.same);
    assert!(
        result
            .first_diff
            .as_deref()
            .unwrap()
            .starts_with("step 2 ({\"key\":\"Tab\"}): line "),
        "{result:?}"
    );
    assert_eq!(out.row("about").status, Status::Differs);
    assert_eq!(out.rules("about"), [Rule::DiffersWithoutResidual]);

    // A node the Rust side draws again where React keeps it.
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    let at = rust.steps[1]
        .dom
        .root
        .iter()
        .position(|l| l.ends_with(" #12>"))
        .unwrap();
    rust.steps[1].dom.root[at] = rust.steps[1].dom.root[at].replace(" #12>", " #new>");
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let diff = out.row("about").behaviour[0].first_diff.clone().unwrap();
    assert!(
        diff.starts_with("step 1") && diff.contains("#new>"),
        "{diff}"
    );

    // A Rust recording with no tC4 recording.
    std::fs::remove_file(p.paths().tc4_behaviour.join("about-keys.json")).unwrap();
    assert!(p.run().rules("about").contains(&Rule::BehaviourUnpaired));
}

#[test]
fn stray_and_mismatched_files_fail() {
    let mut p = Planted::new();
    p.identity("about");
    std::fs::write(p.app().join("screens/x.png"), b"not a capture").unwrap();
    let out = p.run();
    assert!(
        out.global()
            .iter()
            .any(|f| f.rule == Rule::StrayFile && f.message.contains("screens/x.png")),
        "{:#?}",
        out.failures
    );
    std::fs::remove_file(p.app().join("screens/x.png")).unwrap();
    p.entry("about")["domSha256"] = json!("0".repeat(64));
    assert_eq!(p.run().rules("about"), [Rule::HashMismatch]);
}

#[test]
fn unknown_units_are_refused() {
    let p = Planted::new();
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("B12"))
    });
    let out = p.run();
    assert!(
        out.global()
            .iter()
            .any(|f| f.rule == Rule::UnknownUnit && f.message.contains("B12"))
    );
    let p = Planted::new();
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("A6b"))
    });
    assert!(
        p.run()
            .global()
            .iter()
            .any(|f| f.rule == Rule::DuplicateUnit)
    );
}

#[test]
fn a_scope_that_matches_nothing_fails_the_row() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    p.edit_dom("about", |t| {
        t.replace(r#"role=\"dialog\""#, r#"role=\"region\""#)
    });
    let out = p.run();
    assert_eq!(out.row("about").status, Status::Failed);
    assert_eq!(out.rules("about"), [Rule::ScopeMissing]);
    let p = Planted::new();
    p.edit_json("scopes.json", |s| {
        s["scopes"]["home"] = json!({ "attr": "role", "value": "main", "which": "first" })
    });
    assert_eq!(p.run().rules("home"), [Rule::ScopeNotDomFirst]);
}

// ---- the normalizations, each on its own ------------------------------------------------------

fn snapshot(root: &[&str], styles: &[(&str, &str)]) -> Snapshot {
    Snapshot {
        screen: None,
        html: r#"<html dir="ltr" lang="en">"#.to_owned(),
        outside: Vec::new(),
        root: root.iter().map(|l| (*l).to_owned()).collect(),
        styles: styles
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

fn lines(
    snapshot: &Snapshot,
    rename: &[(String, String)],
    scope: Option<&Scope>,
    masks: &[(String, TargetSpec)],
) -> Result<Vec<String>, DomError> {
    dom::prepare(
        Side {
            name: "rust",
            snapshot,
            rename,
        },
        scope,
        masks,
    )
    .map(|p| p.lines)
}

#[test]
fn normalization_unit_cases() {
    // 1. Styles are spelled out; a key with no text is an error.
    let s = snapshot(
        &[
            r#"<div id="root" style=@aaaaaaaaaaaa>"#,
            r#"  <span  style=@aaaaaaaaaaaa .focused>"#,
            r#"    "hi""#,
        ],
        &[("aaaaaaaaaaaa", "color: red")],
    );
    assert_eq!(
        lines(&s, &[], None, &[]).unwrap(),
        [
            r#"html: <html dir="ltr" lang="en">"#,
            r#"<div id="root" style="color: red">"#,
            r#"  <span  style="color: red" .focused>"#,
            r#"    "hi""#
        ]
    );
    let broken = snapshot(&[r#"<div id="root" style=@bbbbbbbbbbbb>"#], &[]);
    assert!(matches!(
        lines(&broken, &[], None, &[]),
        Err(DomError::Format(_))
    ));

    // 2. A scope keeps its element's subtree, first or last; one that matches nothing is an error.
    let s = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <div role="dialog">"#,
            r#"    "one""#,
            r#"  <p >"#,
            r#"  <div role="dialog">"#,
            r#"    <b >"#,
            r#"      "two""#,
        ],
        &[],
    );
    let scope = |which| Scope {
        attr: "role".into(),
        value: "dialog".into(),
        which,
    };
    assert_eq!(
        lines(&s, &[], Some(&scope(Which::First)), &[]).unwrap(),
        [r#"<div role="dialog">"#, r#"  "one""#]
    );
    assert_eq!(
        lines(&s, &[], Some(&scope(Which::Last)), &[]).unwrap(),
        [r#"<div role="dialog">"#, r#"  <b >"#, r#"    "two""#]
    );
    let none = Scope {
        attr: "role".into(),
        value: "alert".into(),
        which: Which::First,
    };
    assert!(matches!(
        lines(&s, &[], Some(&none), &[]),
        Err(DomError::Scope(_))
    ));

    // 3. Folders become repo paths at the end of a value after `-` or `/`, the longest first;
    // nothing else is renamed.
    let rename = vec![
        (
            "sample_burrito_obs".to_owned(),
            "_local_/_local_/sample_burrito_obs".to_owned(),
        ),
        (
            "sample_burrito".to_owned(),
            "_local_/_local_/sample_burrito".to_owned(),
        ),
    ];
    let s = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <div data-testid="share-card-sample_burrito_obs">"#,
            r#"  <div data-testid="project-sample_burrito">"#,
            r#"  <div data-testid="sample_burrito-tile">"#,
            r#"  <div title="sample_burrito">"#,
            r#"    "sample_burrito""#,
        ],
        &[],
    );
    assert_eq!(
        lines(&s, &rename, None, &[]).unwrap()[2..],
        [
            r#"  <div data-testid="share-card-_local_/_local_/sample_burrito_obs">"#,
            r#"  <div data-testid="project-_local_/_local_/sample_burrito">"#,
            r#"  <div data-testid="sample_burrito-tile">"#,
            r#"  <div title="_local_/_local_/sample_burrito">"#,
            r#"    "sample_burrito""#,
        ]
    );

    // 4. Generated ids are numbered per prefix by first appearance; a derived id keeps its suffix.
    let s = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <div aria-labelledby="tc-dlg9" role="dialog">"#,
            r#"    <input aria-describedby="tc-f12-hint" id="tc-f12">"#,
            r#"    <p id="tc-f12-hint">"#,
            r#"    <input id="tc-f1" list="tc-f1-list">"#,
            r#"    <span id="tc-dlg9">"#,
            r#"    <span id="xtc-f3 btc-f4">"#,
        ],
        &[],
    );
    assert_eq!(
        lines(&s, &[], None, &[]).unwrap()[2..],
        [
            r#"  <div aria-labelledby="tc-dlg#1" role="dialog">"#,
            r#"    <input aria-describedby="tc-f#1-hint" id="tc-f#1">"#,
            r#"    <p id="tc-f#1-hint">"#,
            r#"    <input id="tc-f#2" list="tc-f#2-list">"#,
            r#"    <span id="tc-dlg#1">"#,
            r#"    <span id="xtc-f3 btc-f4">"#,
        ]
    );

    // 5. Same-origin asset paths become file names without a Vite hash; other URLs stay.
    let s = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <img src="/ui/0123456789abcdef/images/translationcore-logo.png">"#,
            r#"  <img src="/clients/uw-tc4/assets/logo-mark-AbC_12-x.png">"#,
            r#"  <a href="https://git.door43.org/uw/obs/src/branch/master/x.png">"#,
            r#"  <iframe src="/help/index.html#intro">"#,
            r#"  <div  style=@cccccccccccc>"#,
        ],
        &[(
            "cccccccccccc",
            r#"background-image: url("/media/obs/01-01.jpg")"#,
        )],
    );
    assert_eq!(
        lines(&s, &[], None, &[]).unwrap()[2..],
        [
            r#"  <img src="translationcore-logo.png">"#,
            r#"  <img src="logo-mark.png">"#,
            r#"  <a href="https://git.door43.org/uw/obs/src/branch/master/x.png">"#,
            r#"  <iframe src="index.html#intro">"#,
            r#"  <div  style="background-image: url(\"01-01.jpg\")">"#,
        ]
    );

    // 6. A mask takes its element and subtree out, found by test id or by its first text run;
    // a mask found on one side only is an error.
    let tc4 = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <strong data-testid="about-version">"#,
            r#"    "4.0.0-rc.1 (d310bf3)""#,
            r#"  <div >"#,
            r#"    "The project opens from git.door43.org. Only public.""#,
            r#"  <p >"#,
        ],
        &[],
    );
    let rust = snapshot(
        &[
            r#"<div id="root">"#,
            r#"  <strong data-testid="about-version">"#,
            r#"    "0.1.0 (01befe2)""#,
            r#"  <div data-testid="open-door43-where">"#,
            r#"    "The project opens from git.door43.org.""#,
            r#"  <p >"#,
        ],
        &[],
    );
    let masks = [
        DomMask {
            id: "build".into(),
            tc4: TargetSpec::Testid("about-version".into()),
            rust: TargetSpec::Testid("about-version".into()),
        },
        DomMask {
            id: "where".into(),
            tc4: TargetSpec::Text("The project opens from".into()),
            rust: TargetSpec::Testid("open-door43-where".into()),
        },
    ];
    let found = dom::compare(
        Side {
            name: "tC4",
            snapshot: &tc4,
            rename: &[],
        },
        Side {
            name: "rust",
            snapshot: &rust,
            rename: &[],
        },
        None,
        &masks,
        "x",
    )
    .unwrap();
    assert_eq!(
        (
            found.same,
            found.declared.as_slice(),
            found.first_diffs.len()
        ),
        (
            false,
            ["build".to_owned(), "where".to_owned()].as_slice(),
            0
        )
    );
    let missing = [DomMask {
        id: "where".into(),
        tc4: TargetSpec::Text("The project opens from".into()),
        rust: TargetSpec::Text("The project opens from git.door43.org. Only".into()),
    }];
    let error = dom::compare(
        Side {
            name: "tC4",
            snapshot: &tc4,
            rename: &[],
        },
        Side {
            name: "rust",
            snapshot: &rust,
            rename: &[],
        },
        None,
        &missing,
        "x",
    )
    .unwrap_err();
    assert!(
        matches!(&error, DomError::Mask(m) if m.contains("not in the rust DOM of x")),
        "{error:?}"
    );
}

#[test]
fn the_pixel_count_is_every_pixel_whose_bytes_differ() {
    let image = |pixels: &[[u8; 4]]| pixels::Image {
        width: 2,
        height: 2,
        rgba: pixels.concat(),
    };
    let a = image(&[
        [0, 0, 0, 255],
        [10, 10, 10, 255],
        [20, 20, 20, 255],
        [30, 30, 30, 255],
    ]);
    let b = image(&[
        [0, 0, 1, 255],
        [10, 10, 10, 255],
        [20, 20, 20, 254],
        [30, 30, 30, 255],
    ]);
    let mask = super::inputs::Rect {
        x: 0,
        y: 1,
        width: 1,
        height: 1,
    };
    let found = pixels::count(&a, &b, &[mask]).unwrap();
    assert_eq!(
        (
            found.count.mismatched,
            found.count.outside_masks,
            found.masks_used.as_slice()
        ),
        (2, 1, [0usize].as_slice())
    );
    let small = pixels::Image {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 255],
    };
    assert!(pixels::count(&a, &small, &[]).is_err());
    // An RGB file reads as opaque RGBA, and encoding then decoding keeps every byte.
    assert_eq!(pixels::decode(&pixels::encode(&a).unwrap()).unwrap(), a);
}
