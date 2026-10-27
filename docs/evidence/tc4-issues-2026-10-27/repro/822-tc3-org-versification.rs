// Crate test target: tc-app, `--test app`. Copy to crates/tc-app/tests/app/repro_822.rs, add
// `mod repro_822;` inside `mod app { .. }` in crates/tc-app/tests/app.rs, run
//   cargo test -p tc-app --test app -- repro_822 --nocapture
// then remove both.
//
// tC4 #822: a tC3 import of a book whose verse counts fit only `org` (Malachi in 3 chapters,
// 3:19-24 as in Hebrew Bibles) should make an `org` project. This imports such a tC3 project
// through the real import port and prints the scheme and the verses the project holds.

use std::io::{Read, Write};

use tc_api::import::{ImportDetails, Review};
use tc_api::jobs::{JobOutcome, JobState};

use super::common::post;
use super::import::{app, run, tc3_titus, upload};
use super::port_contract::ended;
use crate::store_world::files;

/// The tC3 fixture rewritten as Malachi in the `org` shape: 14, 17 and 24 verses.
fn tc3_malachi_org() -> Vec<u8> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(tc3_titus())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for name in ["LICENSE.md", "settings.json", "manifest.json"] {
        let mut text = String::new();
        src.by_name(name).unwrap().read_to_string(&mut text).unwrap();
        if name == "manifest.json" {
            text = text
                .replace("\"id\": \"tit\"", "\"id\": \"mal\"")
                .replace("\"Titus\"", "\"Malachi\"");
        }
        out.start_file(name, opts).unwrap();
        out.write_all(text.as_bytes()).unwrap();
    }
    let mut manifest = String::new();
    src.by_name("tit/manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    out.start_file("mal/manifest.json", opts).unwrap();
    out.write_all(manifest.as_bytes()).unwrap();
    for (chapter, verses) in [(1, 14), (2, 17), (3, 24)] {
        let body: serde_json::Map<String, serde_json::Value> = (1..=verses)
            .map(|v| (v.to_string(), format!("Malakai {chapter}:{v} thu.").into()))
            .collect();
        out.start_file(format!("mal/{chapter}.json"), opts).unwrap();
        out.write_all(serde_json::to_string(&body).unwrap().as_bytes())
            .unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_822_a_tc3_malachi_in_org_shape_gets_which_scheme() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, "cfm_mal_book.zip", &tc3_malachi_org());
    let details = || ImportDetails {
        name: "Malakai org".to_owned(),
        code: "cfm".to_owned(),
    };
    let review = post(
        &t.app,
        &Review {
            files: vec![file.clone()],
            details: Some(details()),
            use_installed: None,
        },
    )
    .await;
    println!("REVIEW: {review:?}");
    let job = post(&t.app, &run(60, &file, details())).await;
    println!("RUN: {job:?}");
    let job = job.unwrap();
    let state = ended(&t.app, job).await;
    println!("ENDED: {state:?}");
    let JobState::Done(JobOutcome::Created { project }) = state else {
        panic!("not imported: {state:?}")
    };
    let on_disk = files(&world.fs, &world.folder(&project));
    let vrs: serde_json::Value =
        serde_json::from_slice(&on_disk["ingredients/vrs.json"]).unwrap();
    let scheme = vrs
        .get("name")
        .or_else(|| vrs.get("scheme"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    println!("VRS NAME: {scheme}");
    println!(
        "VRS MAL: {}",
        vrs.get("maxVerses")
            .and_then(|m| m.get("MAL"))
            .map(ToString::to_string)
            .unwrap_or_default()
    );
    let usfm = String::from_utf8(on_disk["ingredients/MAL.usfm"].clone()).unwrap();
    let ch3 = usfm.split("\\c 3").nth(1).unwrap_or_default();
    let kept: Vec<u32> = (19..=24)
        .filter(|v| ch3.contains(&format!("\\v {v} ")))
        .collect();
    println!("MAL 3:19-24 KEPT IN MAL.usfm: {kept:?}");
    println!(
        "CHAPTERS IN MAL.usfm: {}",
        usfm.matches("\\c ").count()
    );
    assert_eq!(kept, (19..=24).collect::<Vec<_>>(), "no verse is lost");
    assert_eq!(scheme, "org", "#822 acceptance 1: a book that fits only org gets org");
}
