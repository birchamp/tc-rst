//! The planted tests: the gate run on the real tC4 baseline, gallery, docs and masks, with Rust
//! captures planted in a temporary `parity/app`. An identity capture is tC4's own screenshot,
//! and tC4's DOM rewritten as the Rust app writes it (project folders for repo paths, other
//! generated id numbers, its own asset paths); it must come out `same`, and one planted
//! difference must fail the gate. A gallery case is planted the same way from its gallery
//! files, and a test that edits the gallery copies one case into a temporary gallery.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use super::behaviour::{Recording, Step};
use super::design::{self, Design, DiffLayer, Layer};
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
    /// A temporary gallery holding the cases `gallery_case` copied, when a test made one.
    gallery: Option<std::path::PathBuf>,
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

/// The units landed in the planted world: those of phase 0 (APP-DESIGN §10).
const PLANTED_LANDED: [&str; 15] = [
    "A1", "A2", "A3", "A4a", "A4b", "A6a", "A6b", "A7", "A9", "A10", "A11", "B1b", "C0", "G0", "A5",
];

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
        let planted = Self {
            dir,
            committed,
            manifest,
            captures,
            report_override: None,
            gallery: None,
        };
        // The planted world is the record as phase 0 left it, whatever has landed since: its
        // units, and no row explained yet. Its captures start empty, so a later unit's landing
        // would make each of its layers overdue in every planted test; a test that needs a
        // later unit lands it.
        planted.edit_json("units.json", |u| u["landed"] = json!(PLANTED_LANDED));
        planted.edit_json("residuals.json", |r| r["rows"] = json!({}));
        // A planted capture is tC4's own, so it draws every element a mask names.
        planted.edit_json("masks.json", |m| {
            for mask in m["masks"].as_array_mut().into_iter().flatten() {
                mask.as_object_mut().map(|mask| mask.remove("rustAbsent"));
            }
        });
        planted
    }

    fn app(&self) -> &Path {
        self.dir.path()
    }

    fn paths(&self) -> Paths {
        Paths {
            app: self.app().to_owned(),
            tc4_behaviour: self.app().join("tc4-behaviour"),
            gallery: self
                .gallery
                .clone()
                .unwrap_or_else(|| self.committed.gallery.clone()),
            ..self.committed.clone()
        }
    }

    /// The committed gallery case `name`, as cases.json lists it.
    fn case(&self, name: &str) -> Value {
        let cases: Value = read_json(&self.committed.app.join("cases.json")).unwrap();
        cases["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("no gallery case {name}"))
            .clone()
    }

    /// Copies the committed case `name` into a temporary gallery that holds only it, with a
    /// cases.json that lists only it.
    fn gallery_case(&mut self, name: &str) {
        let case = self.case(name);
        let gallery = self.app().join("gallery");
        for sub in ["screens", "dom", "actions"] {
            std::fs::create_dir_all(gallery.join(sub)).unwrap();
        }
        for key in ["png", "dom", "actions"] {
            let rel = case[key].as_str().unwrap();
            std::fs::copy(self.committed.gallery.join(rel), gallery.join(rel)).unwrap();
        }
        let others: Vec<String> =
            read_json::<Value>(&self.app().join("cases.json")).unwrap()["cases"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| c["name"].as_str().filter(|n| *n != name).map(str::to_owned))
                .collect();
        self.edit_json("cases.json", |c| c["cases"] = json!([case]));
        // The masks' rectangles on the cases left out go with them.
        self.edit_json("masks.json", |m| {
            for mask in m["masks"].as_array_mut().unwrap() {
                let rects = mask["rects"].as_object_mut().unwrap();
                rects.retain(|k, _| !others.contains(k));
            }
        });
        self.gallery = Some(gallery);
    }

    /// Plants the Rust app's capture of gallery case `name` by `unit`, identical to the
    /// gallery's: its PNG, and its DOM rewritten as the Rust app writes it.
    fn identity_case(&mut self, name: &str, unit: &str, layers: &[&str]) -> &mut Value {
        let case = self.case(name);
        let gallery = self.paths().gallery;
        std::fs::copy(
            gallery.join(case["png"].as_str().unwrap()),
            self.app().join(format!("screens/{name}.png")),
        )
        .unwrap();
        let tc4_dom = std::fs::read_to_string(gallery.join(case["dom"].as_str().unwrap())).unwrap();
        std::fs::write(
            self.app().join(format!("dom/{name}.json")),
            as_rust_writes(&tc4_dom),
        )
        .unwrap();
        let entry = json!({
            "name": name, "kind": "case", "scenario": case["scenario"], "locale": case["locale"],
            "by": unit, "backend": "store", "layers": layers,
            "error": null, "pageErrors": [], "refused": [],
            "homeCards": {}, "syncToast": null, "maskRects": {},
            "png": format!("screens/{name}.png"), "sha256": "", "dom": format!("dom/{name}.json"), "domSha256": "",
        });
        self.captures["screens"].as_array_mut().unwrap().push(entry);
        self.rehash(name);
        self.land(unit);
        self.entry(name)
    }

    /// Adds `unit` to the landed units, once.
    fn land(&self, unit: &str) {
        self.edit_json("units.json", |u| {
            let landed = u["landed"].as_array_mut().unwrap();
            if !landed.iter().any(|l| l == unit) {
                landed.push(json!(unit));
            }
        });
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
            "by": "A6b", "backend": "store", "layers": ["pixels", "dom", "meaning"],
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

    /// Edits the lines under `#root` of the Rust app's DOM file of `name`.
    fn edit_root(&mut self, name: &str, edit: impl FnOnce(&mut Vec<String>)) {
        let file = self.app().join(format!("dom/{name}.json"));
        let mut snapshot: Snapshot = read_json(&file).unwrap();
        let before = snapshot.root.clone();
        edit(&mut snapshot.root);
        assert_ne!(snapshot.root, before, "the edit changed nothing");
        std::fs::write(&file, serde_json::to_string(&snapshot).unwrap()).unwrap();
        self.rehash(name);
    }

    fn edit_png(&mut self, name: &str, edit: impl FnOnce(&mut pixels::Image)) {
        let file = self.app().join(format!("screens/{name}.png"));
        let mut image = pixels::read(&file).unwrap();
        edit(&mut image);
        std::fs::write(&file, pixels::encode(&image).unwrap()).unwrap();
        self.rehash(name);
    }

    /// The planted record as before `unit` landed: a planted run starts with no capture, so a
    /// landed unit owes every layer it captures.
    fn unland(&self, unit: &str) {
        self.edit_json("units.json", |u| {
            u["landed"].as_array_mut().unwrap().retain(|l| l != unit);
        });
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
    let cases = Record::load(p.paths()).unwrap().cases.cases.len();
    assert!(cases > 0, "the committed gallery has cases");
    assert_eq!(out.report.totals.unported, 60 + cases);
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
fn a_residual_names_exactly_the_layers_that_differ() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_dom("about", |t| {
        t.replacen(r#"data-tone=\"neutral\""#, r#"data-tone=\"accent\""#, 1)
    });
    let explain = |p: &Planted, layers: Value| {
        p.edit_json("residuals.json", |r| {
            r["rows"]["about"] = json!({ "layers": layers, "why": "planted", "until": null })
        });
        p.run()
    };
    // A residual that explains the pixels does not excuse a DOM difference.
    let out = explain(&p, json!(["pixels"]));
    assert_eq!(out.row("about").differing(), [DiffLayer::Dom].into());
    assert_eq!(out.rules("about"), [Rule::ResidualLayers]);
    let message = &out
        .failures
        .iter()
        .find(|f| f.rule == Rule::ResidualLayers)
        .unwrap()
        .message;
    assert!(
        message.starts_with(
            r#"residuals.json explains the layers ["pixels"], but the row differs in ["dom"]"#
        ),
        "{message}"
    );
    explain(&p, json!(["dom"])).assert_clean(&["about"]);
    // Nor may it name a layer that does not differ.
    assert_eq!(
        explain(&p, json!(["dom", "pixels"])).rules("about"),
        [Rule::ResidualLayers]
    );
    p.edit_png("about", |image| flip_red(image, 5, 5));
    explain(&p, json!(["pixels", "dom"])).assert_clean(&["about"]);
    assert_eq!(
        explain(&p, json!(["dom"])).rules("about"),
        [Rule::ResidualLayers]
    );
}

#[test]
fn a_row_that_differs_only_in_behaviour_is_explained_as_behaviour() {
    let mut p = Planted::new();
    p.identity("about");
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    rust.steps[1].focus = "button \"Read the license\"".to_owned();
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    p.edit_json("residuals.json", |r| {
        r["rows"]["about"] = json!({ "layers": ["behaviour"], "why": "planted", "until": null })
    });
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Differs, "{row:#?}");
    assert_eq!(row.differing(), [DiffLayer::Behaviour].into());
    out.assert_clean(&["about"]);
    p.edit_json("residuals.json", |r| {
        r["rows"]["about"]["layers"] = json!(["dom"])
    });
    assert_eq!(p.run().rules("about"), [Rule::ResidualLayers]);
}

#[test]
fn a_screenshot_of_another_size_fails_even_with_a_residual() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_png("about", |image| {
        image.height -= 1;
        image
            .rgba
            .truncate((image.width * image.height * 4) as usize);
    });
    p.edit_json("residuals.json", |r| {
        r["rows"]["about"] = json!({ "layers": ["pixels"], "why": "planted", "until": null })
    });
    let out = p.run();
    let row = out.row("about");
    assert_eq!(row.status, Status::Failed, "{row:#?}");
    assert_eq!(
        row.error.as_deref(),
        Some(
            "the screenshot is another size than tC4's: rust 1280x799, tC4 1280x800; capture it at the screen's viewport"
        )
    );
    assert!(row.pixels.is_none() && row.dom.is_none());
    assert_eq!(
        out.rules("about"),
        [Rule::CaptureFailed, Rule::StaleResidual]
    );
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
    p.unland("A5");
    // A8's DOM layers (share-signin, net-failed, signin-error) are not this test's.
    p.unland("A8");
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
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("A5"))
    });
    assert_eq!(
        p.run().rules("account-menu"),
        [Rule::LayerOverdue, Rule::LayerOverdue, Rule::LayerOverdue],
        "with A5 landed, its DOM is due too"
    );
    assert!(
        out.rules("understand-tit1").is_empty(),
        "B3's screen is not due"
    );
    let record = Record::load(p.paths()).unwrap();
    let (cases, screens): (std::collections::BTreeSet<_>, std::collections::BTreeSet<_>) = out
        .failures
        .iter()
        .filter(|f| f.rule == Rule::LayerOverdue)
        .filter_map(|f| f.row.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .partition(|name| record.case(name).is_some());
    assert_eq!(
        screens.len(),
        11,
        "the six B2 screens and the five whose pixels are B2's: {screens:?}"
    );
    let b2_cases: std::collections::BTreeSet<_> = record
        .cases
        .cases
        .iter()
        .filter(|c| c.dom_unit.as_str() == "B2" || c.pixels_unit.as_str() == "B2")
        .map(|c| c.name.clone())
        .collect();
    assert!(b2_cases.contains("license") && b2_cases.contains("home-empty"));
    assert_eq!(
        cases, b2_cases,
        "every gallery case a B2 layer of is due too"
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

/// §7.5: once C1a has moved the parity environment to the store, a capture on the memory
/// backend fails; a scripted capture keeps its own rule.
#[test]
fn a_memory_capture_fails_once_c1a_has_landed() {
    let mut p = Planted::new();
    p.identity("about");
    p.edit_json("units.json", |u| {
        u["landed"]
            .as_array_mut()
            .unwrap()
            .retain(|unit| unit != "C1a")
    });
    p.entry("about")["backend"] = json!("memory");
    p.run().assert_clean(&["about"]);
    p.edit_json("units.json", |u| {
        u["landed"].as_array_mut().unwrap().push(json!("C1a"))
    });
    assert_eq!(p.run().rules("about"), [Rule::MemoryAfterStore]);
    p.entry("about")["backend"] = json!("store");
    p.run().assert_clean(&["about"]);
    p.identity("sync-ok")["backend"] = json!("memory+scripted");
    let out = p.run();
    assert!(
        !out.rules("sync-ok").contains(&Rule::MemoryAfterStore),
        "{:?}",
        out.rules("sync-ok")
    );
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
    p.unland("A5");
    p.edit_json("masks.json", |m| {
        m["masks"][2]["rects"]["about"] = json!({ "x": 1, "y": 1, "width": 2, "height": 2 })
    });
    assert_eq!(p.run().rules("about"), [Rule::MaskRuling]);
    let p = Planted::new();
    p.edit_json("masks.json", |m| m["masks"][0]["ruling"] = json!("Q99"));
    assert!(p.run().global().iter().any(|f| f.rule == Rule::MaskRuling));
}

/// B9a masks the Urdu row of `source-texts` under tC4 #701, which docs/DIFFERENCES.md lists among the
/// differences on purpose, as the gallery's residuals cite #552.
#[test]
fn a_mask_may_cite_a_tc4_issue_that_plan_lists() {
    let p = Planted::new();
    let mask = |ruling: &str| {
        json!({
            "id": "ur-deva-name",
            "ruling": ruling,
            "why": "tC4 names Urdu (Devanagari) اردو in Arabic script; the rebuild names it उर्दू and reads its helps left to right.",
            "target": { "text": "اردو" },
            "rust": { "text": "उर्दू" },
            "rects": { "source-texts": { "x": 400, "y": 300, "width": 120, "height": 20 } },
            "meaning": null,
        })
    };
    p.edit_json("masks.json", |m| {
        m["masks"].as_array_mut().unwrap().push(mask("#701"))
    });
    p.run().assert_clean(&["source-texts"]);
    // An issue DIFFERENCES does not list is refused as an owner's answer it does not list is.
    p.edit_json("masks.json", |m| {
        *m["masks"].as_array_mut().unwrap().last_mut().unwrap() = mask("#7010")
    });
    let out = p.run();
    let refused: Vec<_> = out
        .global()
        .into_iter()
        .filter(|f| f.rule == Rule::MaskRuling)
        .collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].message.contains("#7010"), "{}", refused[0]);
}

/// A DOM file as a behaviour step, each element numbered.
fn numbered(file: &Path) -> Snapshot {
    number(read_json(file).unwrap())
}

/// A snapshot as a behaviour step: each element numbered by its line under `#root`.
fn number(mut snapshot: Snapshot) -> Snapshot {
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
fn a_scoped_sequence_numbers_its_nodes_whatever_precedes_the_scope() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut page: Snapshot = read_json(&p.app().join("dom/about.json")).unwrap();
    let dialog = page
        .root
        .iter()
        .position(|l| l.contains(r#"role="dialog""#))
        .unwrap();
    let indent = page.root[dialog].len() - page.root[dialog].trim_start().len();
    // The Rust page holds one more element before the dialog, so every node number in the
    // dialog is one higher than tC4's.
    page.root.insert(
        dialog,
        format!("{}<div data-testid=\"toasts\">", " ".repeat(indent)),
    );
    let rust = recording(&number(page));
    assert_ne!(
        tc4.steps[0].dom.root[dialog],
        rust.steps[0].dom.root[dialog + 1]
    );
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let row = out.row("about");
    assert!(row.behaviour[0].same, "{:#?}", row.behaviour);
    assert_eq!(row.status, Status::Same);
    out.assert_clean(&["about"]);
}

/// The step's DOM with the dialog taken out, as after a key that closed it.
fn without_dialog(step: &mut Step) {
    let root = &mut step.dom.root;
    let at = root
        .iter()
        .position(|l| l.contains(r#"role="dialog""#))
        .unwrap();
    let depth = root[at].len() - root[at].trim_start().len();
    let end = (at + 1..root.len())
        .find(|&i| root[i].len() - root[i].trim_start().len() <= depth)
        .unwrap_or(root.len());
    root.drain(at..end);
    step.focus = "button \"\"".to_owned();
}

/// A5 D7: a step that closes the dialog of a DOM-first sequence compares equal when the dialog
/// is gone on both sides.
#[test]
fn a_sequence_step_whose_scope_is_gone_on_both_sides_compares_equal() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    let mut tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    without_dialog(&mut tc4.steps[2]);
    without_dialog(&mut rust.steps[2]);
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let row = out.row("about");
    assert!(row.behaviour[0].same, "{:#?}", row.behaviour);
    assert_eq!(row.status, Status::Same);
    out.assert_clean(&["about"]);
}

#[test]
fn a_sequence_step_whose_scope_is_gone_on_one_side_differs() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    without_dialog(&mut rust.steps[2]);
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let result = &out.row("about").behaviour[0];
    assert!(!result.same);
    let diff = result.first_diff.as_deref().unwrap();
    assert!(
        diff.starts_with("step 2") && diff.contains("(scope absent)"),
        "{diff}"
    );
    assert_eq!(out.rules("about"), [Rule::DiffersWithoutResidual]);
}

#[test]
fn a_scoped_sequence_compares_the_documents_lang_and_dir() {
    let mut p = Planted::new();
    p.identity("about")["layers"] = json!(["dom"]);
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    rust.steps[1].dom.html = r#"<html dir="rtl" lang="ar">"#.to_owned();
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let result = &out.row("about").behaviour[0];
    assert!(!result.same);
    let diff = result.first_diff.as_deref().unwrap();
    assert!(
        diff.starts_with("step 1") && diff.contains(r#"lang="ar""#),
        "{diff}"
    );
}

#[test]
fn a_masked_element_takes_no_part_in_a_sequence() {
    let mut p = Planted::new();
    p.identity("about");
    let tc4 = recording(&numbered(&p.committed.baseline.join("dom/about.json")));
    let mut rust = recording(&numbered(&p.app().join("dom/about.json")));
    for (n, step) in rust.steps.iter_mut().enumerate() {
        let version = step
            .dom
            .root
            .iter()
            .position(|l| l.trim_start().starts_with(r#""4.0.0-rc."#))
            .unwrap();
        step.dom.root[version] =
            step.dom.root[version].replace("4.0.0-rc.", &format!("0.{n}.0-rc."));
    }
    write_recording(&p.paths().tc4_behaviour, &tc4);
    write_recording(&p.app().join("behaviour"), &rust);
    let out = p.run();
    let row = out.row("about");
    assert!(row.behaviour[0].same, "{:#?}", row.behaviour);
    assert_eq!(row.status, Status::Same);
    out.assert_clean(&["about"]);
}

#[test]
fn ids_after_a_mask_are_numbered_without_the_ids_inside_it() {
    let mut p = Planted::new();
    p.identity("open-door43");
    // The rebuild's Q3 note holds a generated id that tC4's does not; the fields after it
    // still number from 1.
    p.edit_root("open-door43", |root| {
        let note = root
            .iter()
            .position(|l| l.trim_start().starts_with("\"The project opens from"))
            .unwrap();
        root[note - 1] = root[note - 1].replacen("<div ", r#"<div aria-describedby="tc-f3""#, 1);
    });
    let out = p.run();
    let row = out.row("open-door43");
    let dom = row.dom.as_ref().unwrap();
    assert!(dom.first_diffs.is_empty(), "{:#?}", dom.first_diffs);
    assert_eq!(dom.declared, ["q3-where"]);
    assert_eq!(row.status, Status::Declared);
    out.assert_clean(&["open-door43"]);
}

#[test]
fn the_sync_toast_is_compared_field_by_field() {
    let mut p = Planted::new();
    p.identity("sync-ok-teammates")["backend"] = json!("memory+scripted");
    let out = p.run();
    assert_eq!(out.row("sync-ok-teammates").status, Status::Same);
    out.assert_clean(&["sync-ok-teammates"]);

    p.entry("sync-ok-teammates")["syncToast"]["received"] = json!(3);
    let out = p.run();
    let row = out.row("sync-ok-teammates");
    assert_eq!(row.status, Status::Differs);
    assert_eq!(
        row.meaning.as_ref().unwrap().diffs,
        ["syncToast.received: tC4 2, rust 3"]
    );
    assert_eq!(
        out.rules("sync-ok-teammates"),
        [Rule::DiffersWithoutResidual]
    );

    p.entry("sync-ok-teammates")["syncToast"] = Value::Null;
    let out = p.run();
    assert_eq!(
        out.row("sync-ok-teammates").meaning.as_ref().unwrap().diffs,
        ["syncToast: tC4 shows the toast, rust does not"]
    );
    assert_eq!(
        out.rules("sync-ok-teammates"),
        [Rule::DiffersWithoutResidual]
    );
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

// ---- the gallery cases -------------------------------------------------------------------------

#[test]
fn a_gallery_case_is_a_row_naming_its_units() {
    let p = Planted::new();
    p.unland("A5");
    let out = p.run();
    let row = out.row("license");
    assert_eq!(row.kind, super::inputs::RowKind::Case);
    assert_eq!(row.status, Status::Unported);
    assert_eq!(
        (row.scenario.as_str(), row.locale.as_str()),
        ("sample", "en")
    );
    let missing: Vec<_> = row.missing.iter().map(|(l, u)| (*l, u.as_str())).collect();
    assert_eq!(
        missing,
        [(Layer::Pixels, "B2"), (Layer::Dom, "A5")],
        "a case has no meaning layer"
    );
    let row = out.row("conflict-review-mine");
    assert_eq!(
        (row.scenario.as_str(), row.status),
        ("shared", Status::Unported)
    );
    out.assert_clean(&["license", "conflict-review-mine"]);
}

#[test]
fn an_identical_gallery_capture_is_same() {
    let mut p = Planted::new();
    p.identity_case("open-progress-journal", "B3", &["pixels", "dom"]);
    let out = p.run();
    let row = out.row("open-progress-journal");
    assert_eq!(row.status, Status::Same, "{row:#?}");
    assert!(row.missing.is_empty());
    assert_eq!(row.pixels.as_ref().unwrap().mismatched, 0);
    assert!(row.dom.as_ref().unwrap().same);
    assert!(row.meaning.is_none());
    out.assert_clean(&["open-progress-journal"]);
    // A planted pixel and a planted DOM difference fail it.
    p.edit_png("open-progress-journal", |image| flip_red(image, 5, 5));
    p.edit_dom("open-progress-journal", |t| {
        t.replacen(
            r#"data-testid=\"open-progress-stage\""#,
            r#"data-testid=\"open-progress-step\""#,
            1,
        )
    });
    let out = p.run();
    assert_eq!(out.row("open-progress-journal").status, Status::Differs);
    assert_eq!(
        out.row("open-progress-journal").differing(),
        [DiffLayer::Pixels, DiffLayer::Dom].into()
    );
    assert_eq!(
        out.rules("open-progress-journal"),
        [Rule::DiffersWithoutResidual]
    );
}

#[test]
fn a_case_from_another_machine_fails() {
    let mut p = Planted::new();
    p.identity_case("open-progress-journal", "B3", &["pixels", "dom"]);
    p.edit_json("cases.json", |c| {
        c["machine"]["fonts"]["sha256"] = json!("0".repeat(64))
    });
    let out = p.run();
    assert_eq!(out.row("open-progress-journal").status, Status::Failed);
    assert_eq!(out.rules("open-progress-journal"), [Rule::OtherMachine]);
    // The baseline's machine does not stand in for the gallery's.
    let mut p = Planted::new();
    p.identity_case("open-progress-journal", "B3", &["pixels", "dom"]);
    assert_eq!(p.run().row("open-progress-journal").status, Status::Same);
    p.captures["machine"]["fonts"]["sha256"] = json!("1".repeat(64));
    assert_eq!(p.run().rules("open-progress-journal"), [Rule::OtherMachine]);
}

#[test]
fn a_dom_first_case_compares_its_scope_alone() {
    let mut p = Planted::new();
    p.identity_case("license", "A5", &["dom"]);
    // Outside the dialog: Home behind it, which is B2's to port.
    p.edit_dom("license", |t| {
        t.replacen(
            r#"data-testid=\"add-project\""#,
            r#"data-testid=\"add-a-project\""#,
            1,
        )
    });
    let out = p.run();
    let row = out.row("license");
    assert_eq!(row.status, Status::Same, "{row:#?}");
    assert_eq!(
        row.dom.as_ref().unwrap().scope,
        Some(Scope {
            attr: "role".into(),
            value: "dialog".into(),
            which: Which::Last
        })
    );
    let missing: Vec<_> = row.missing.iter().map(|(l, u)| (*l, u.as_str())).collect();
    assert_eq!(missing, [(Layer::Pixels, "B2")]);
    out.assert_clean(&["license"]);
    // Inside the dialog.
    p.edit_dom("license", |t| {
        t.replacen(
            r#"data-testid=\"license-close\""#,
            r#"data-testid=\"license-shut\""#,
            1,
        )
    });
    let out = p.run();
    assert_eq!(out.row("license").status, Status::Differs);
    assert!(out.row("license").dom.as_ref().unwrap().first_diffs[0].contains("license-shut"));
}

#[test]
fn a_changed_or_stray_gallery_file_fails() {
    let mut p = Planted::new();
    p.gallery_case("home-empty");
    p.run().assert_clean(&["home-empty"]);
    p.edit_json("cases.json", |c| {
        c["cases"][0]["domSha256"] = json!("0".repeat(64))
    });
    let out = p.run();
    assert_eq!(out.rules("home-empty"), [Rule::HashMismatch]);
    assert!(
        out.failures[0]
            .message
            .starts_with("parity/view-gallery/dom/home-empty.json has sha256 "),
        "{:#?}",
        out.failures
    );
    let mut p = Planted::new();
    p.gallery_case("home-empty");
    std::fs::write(p.app().join("gallery/screens/x.png"), b"not a case").unwrap();
    let out = p.run();
    assert!(
        out.global()
            .iter()
            .any(|f| f.rule == Rule::StrayFile && f.message.contains("view-gallery/screens/x.png")),
        "{:#?}",
        out.failures
    );
    // A trace is a file of the case too.
    let mut p = Planted::new();
    p.gallery_case("home-empty");
    std::fs::write(p.app().join("gallery/actions/home-empty.json"), b"{}").unwrap();
    assert_eq!(p.run().rules("home-empty"), [Rule::HashMismatch]);
}

#[test]
fn a_case_whose_unit_has_landed_must_be_captured() {
    let p = Planted::new();
    p.land("B3");
    let out = p.run();
    assert_eq!(
        out.rules("open-progress-state"),
        [Rule::LayerOverdue, Rule::LayerOverdue]
    );
    let message = &out
        .failures
        .iter()
        .find(|f| f.row.as_deref() == Some("open-progress-state"))
        .unwrap()
        .message;
    assert!(
        message.contains("capture-rust.mjs --unit B3 --case open-progress-state"),
        "{message}"
    );
    // A DOM-first case owes its DOM to the first unit only.
    let p = Planted::new();
    p.land("A5");
    assert_eq!(p.run().rules("license"), [Rule::LayerOverdue]);
}

#[test]
fn a_case_capture_of_another_scenario_or_with_meaning_fails() {
    let mut p = Planted::new();
    p.identity_case("open-progress-journal", "B3", &["pixels", "dom"])["scenario"] =
        json!("shared");
    assert_eq!(
        p.run().rules("open-progress-journal"),
        [Rule::PlanDisagreesWithBaseline]
    );
    let mut p = Planted::new();
    p.identity_case("open-progress-journal", "B3", &["pixels", "dom", "meaning"]);
    let out = p.run();
    let row = out.row("open-progress-journal");
    assert_eq!(row.status, Status::Failed);
    assert_eq!(
        row.error.as_deref(),
        Some("a gallery case has no meaning layer: capture it with --layers pixels,dom")
    );
    assert_eq!(out.rules("open-progress-journal"), [Rule::CaptureFailed]);
}

#[test]
fn a_case_outside_section_7_4_or_without_its_scope_fails() {
    let mut p = Planted::new();
    p.unland("A5");
    p.gallery_case("license");
    p.run().assert_clean(&["license"]);
    p.edit_json("cases.json", |c| c["cases"][0]["group"] = json!("about"));
    assert_eq!(p.run().rules("license"), [Rule::CaseGroup]);
    p.edit_json("cases.json", |c| {
        c["cases"][0]["group"] = json!("license");
        c["cases"][0]["pixelsUnit"] = json!("A5");
    });
    assert_eq!(
        p.run().rules("license"),
        [Rule::CaseGroup, Rule::CaseScope],
        "other units than §7.4's; and a scope on a case one unit captures"
    );
    p.edit_json("cases.json", |c| {
        c["cases"][0]["pixelsUnit"] = json!("B2");
        c["cases"][0]["scope"] = Value::Null;
    });
    assert_eq!(p.run().rules("license"), [Rule::CaseScope]);
    let mut p = Planted::new();
    p.gallery_case("home-empty");
    p.edit_json("cases.json", |c| c["cases"][0]["name"] = json!("home"));
    let out = p.run();
    assert!(
        out.rules("home").contains(&Rule::DuplicateRow),
        "{:#?}",
        out.failures
    );
    p.edit_json("cases.json", |c| {
        c["cases"][0]["name"] = json!("home-empty");
        let again = c["cases"][0].clone();
        c["cases"].as_array_mut().unwrap().push(again);
    });
    assert!(p.run().rules("home-empty").contains(&Rule::DuplicateRow));
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
            rust_absent: false,
        },
        DomMask {
            id: "where".into(),
            tc4: TargetSpec::Text("The project opens from".into()),
            rust: TargetSpec::Testid("open-door43-where".into()),
            rust_absent: false,
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
        rust_absent: false,
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
    // A mask whose element the rebuild does not draw yet is taken out of tC4's side only, and
    // fails the row once the rebuild draws it.
    let absent = |rust: &str| {
        [DomMask {
            id: "card".into(),
            tc4: TargetSpec::Testid("about-version".into()),
            rust: TargetSpec::Testid(rust.into()),
            rust_absent: true,
        }]
    };
    let compare = |masks: &[DomMask]| {
        dom::compare(
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
            masks,
            "x",
        )
    };
    let lacking = compare(&absent("share-card-obs")).unwrap();
    assert!(!lacking.same && !lacking.first_diffs.is_empty());
    let drawn = compare(&absent("about-version")).unwrap_err();
    assert!(
        matches!(&drawn, DomError::Mask(m) if m.contains("remove its `rustAbsent`")),
        "{drawn:?}"
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
