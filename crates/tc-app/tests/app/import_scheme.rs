//! VRS-GUESS (owner ruling H13, tC4 #822): a tC3 import offers the versification its books'
//! verse counts fit, writes the one the review page showed, and Update a book keeps it.
//! INV-IMPORT-SCHEME-SHOWN.

use std::io::{Read, Write};

use tc_api::ApiError;
use tc_api::ids::ProjectRef;
use tc_api::import::{ImportDetails, ImportFile, ImportReview, Review, SchemeFit};
use tc_api::jobs::{JobOutcome, JobState};
use tc_core::BookId;
use tc_versification::StandardScheme;

use super::common::{TestApp, open_progress, post};
use super::import::{app, run, tc3_titus, update, upload};
use super::port_contract::ended;
use crate::store_world::{StoreWorld, assert_valid_burrito, files};

/// The tC3 Titus fixture rewritten as Malachi in `chapters`' shape (#822's repro).
fn tc3_malachi(chapters: &[u16]) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(tc3_titus())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for name in ["LICENSE.md", "settings.json", "manifest.json"] {
        let mut text = String::new();
        src.by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
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
    for (chapter, &verses) in (1..).zip(chapters) {
        let body: serde_json::Map<String, serde_json::Value> = (1..=verses)
            .map(|v| (v.to_string(), format!("Malakai {chapter}:{v} thu.").into()))
            .collect();
        out.start_file(format!("mal/{chapter}.json"), opts).unwrap();
        out.write_all(serde_json::to_string(&body).unwrap().as_bytes())
            .unwrap();
    }
    out.finish().unwrap().into_inner()
}

const HEBREW_MALACHI: [u16; 3] = [14, 17, 24];

fn details() -> ImportDetails {
    ImportDetails {
        name: "Malakai".to_owned(),
        code: "cfm".to_owned(),
    }
}

async fn review(t: &TestApp, file: &ImportFile) -> ImportReview {
    post(
        &t.app,
        &Review {
            files: vec![file.clone()],
            details: Some(details()),
            use_installed: None,
            versification: None,
        },
    )
    .await
    .unwrap()
}

/// Imports `file` with `scheme`, as Import sends what the review page shows.
async fn import(t: &TestApp, file: &ImportFile, scheme: &str) -> ProjectRef {
    let mut asked = run(60, file, details());
    asked.versification = scheme.to_owned();
    let job = post(&t.app, &asked).await.unwrap();
    match ended(&t.app, job).await {
        JobState::Done(JobOutcome::Created { project }) => project,
        other => panic!("not imported: {other:?}"),
    }
}

fn vrs(world: &StoreWorld, project: &ProjectRef) -> serde_json::Value {
    serde_json::from_slice(&files(&world.fs, &world.folder(project))["ingredients/vrs.json"])
        .unwrap()
}

fn standard(scheme: StandardScheme) -> serde_json::Value {
    serde_json::from_str(scheme.json()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tc3_malachi_in_hebrew_numbering_imports_as_org() {
    let (dir, t, world) = app().await;
    let file = upload(
        dir.path(),
        1,
        "cfm_mal_book.zip",
        &tc3_malachi(&HEBREW_MALACHI),
    );
    let shown = review(&t, &file).await.found.versification;
    assert_eq!(shown.scheme, "org", "the default the review page shows");
    assert_eq!(
        shown.fit,
        SchemeFit::Tied {
            schemes: vec!["lxx".into(), "org".into()]
        }
    );
    assert_eq!(shown.names, ["eng", "lxx", "org", "rsc", "rso", "vul"]);

    let project = import(&t, &file, &shown.scheme).await;
    assert_eq!(
        vrs(&world, &project),
        standard(StandardScheme::Org),
        "INV-IMPORT-SCHEME-SHOWN: the project gets the scheme the review page showed"
    );
    let on_disk = files(&world.fs, &world.folder(&project));
    let usfm = String::from_utf8(on_disk["ingredients/MAL.usfm"].clone()).unwrap();
    let ch3 = usfm.split("\\c 3").nth(1).unwrap();
    assert!(
        (19..=24).all(|v| ch3.contains(&format!("\\v {v} Malakai 3:{v} thu."))),
        "no verse is lost: {ch3}"
    );
    assert_valid_burrito(&world.fs, &world.folder(&project));

    let at = t
        .app
        .sessions()
        .open_screen(&project, open_progress())
        .await
        .unwrap()
        .at;
    let projected = t
        .app
        .read(&at)
        .unwrap()
        .handle()
        .snapshot()
        .fold()
        .projected(&tc_journal::RecordKey::Versification);
    let Some(tc_journal::Record::Versification { value }) = projected else {
        panic!("no versification record: {projected:?}")
    };
    assert_eq!(
        value.name.as_deref(),
        Some("org"),
        "the project's versification record names org"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tc3_import_writes_the_versification_the_person_chose() {
    let (dir, t, world) = app().await;
    let file = upload(
        dir.path(),
        1,
        "cfm_mal_book.zip",
        &tc3_malachi(&HEBREW_MALACHI),
    );
    let mut asked = run(61, &file, details());
    asked.versification = "kjv".to_owned();
    let refused = post(&t.app, &asked).await.unwrap_err();
    assert!(
        matches!(&refused, ApiError::BadRequest { field, .. } if field == "versification"),
        "{refused:?}"
    );
    // Nothing was written: the same files import with a standard versification.
    let project = import(&t, &file, "vul").await;
    assert_eq!(vrs(&world, &project), standard(StandardScheme::Vul));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tc3_import_the_verse_counts_do_not_decide_offers_eng_and_says_so() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, "cfm_fbt_tit_book.zip", &tc3_titus());
    let shown = review(&t, &file).await.found.versification;
    assert_eq!(
        (shown.scheme.as_str(), shown.fit),
        ("eng", SchemeFit::Undecided)
    );
    let project = import(&t, &file, &shown.scheme).await;
    assert_eq!(vrs(&world, &project), standard(StandardScheme::Eng));

    let file = upload(
        dir.path(),
        2,
        "cfm_mal_book.zip",
        &tc3_malachi(&[14, 17, 24, 2]),
    );
    let shown = review(&t, &file).await.found.versification;
    assert_eq!(
        (shown.scheme.as_str(), shown.fit),
        ("org", SchemeFit::Nearest)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_keeps_the_projects_versification() {
    let (dir, t, world) = app().await;
    let file = upload(
        dir.path(),
        1,
        "cfm_mal_book.zip",
        &tc3_malachi(&HEBREW_MALACHI),
    );
    let project = import(&t, &file, "org").await;
    let at = t
        .app
        .sessions()
        .open_screen(&project, open_progress())
        .await
        .unwrap()
        .at;
    // Malachi in English numbering: four chapters.
    let mut usfm = String::from("\\id MAL\n");
    for (chapter, verses) in (1..).zip([14, 17, 18, 6]) {
        usfm.push_str(&format!("\\c {chapter}\n"));
        for v in 1..=verses {
            usfm.push_str(&format!("\\v {v} Malakai {chapter}:{v} thu.\n"));
        }
    }
    let file = upload(dir.path(), 2, "MAL.usfm", usfm.as_bytes());
    let job = post(&t.app, &update(3, &at, BookId::Malachi, &file))
        .await
        .unwrap();
    assert!(
        matches!(
            ended(&t.app, job).await,
            JobState::Done(JobOutcome::BookUpdated(_))
        ),
        "the update ran"
    );
    assert_eq!(vrs(&world, &project), standard(StandardScheme::Org));
}
