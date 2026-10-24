//! What planning a change of pins costs (APP-DESIGN §8.2, "Count and plan an upgrade or gateway
//! change"; PERFORMANCE rule 2): the books the change affects, never the project. A new checking
//! language whose package covers one book moves the decisions of that book on Psalms and on the
//! corpus ten times larger alike, and a package that covers every book costs each book it covers.

use std::io::Write;
use std::path::Path;

use tc_api::jobs::{JobOutcome, JobState};
use tc_api::resources::{GatewayRef, PlanGateway, PlanView};
use tc_app::backend::{CheckpointReason, StoreCommand};
use tc_core::BookId;
use tc_journal::command::{PlannedChange, UpgradePlan};
use tc_journal::keys::PinGroup;
use tc_journal::record::Record;
use tc_journal::{Cause, RecordKey, ToolId};
use tc_resources::{
    BookCoverage, ProjectPins, ResourcePin, ResourceStore, SetSlot, install_dcs_zip,
};

use super::corpus::{PSALMS, corpora, store_corpora};
use super::harness::{Backing, Scale, cost, costs_on_both, growth, scale};
use crate::common::rid;

const FIXTURES: &str = "fixtures/tc4/test-fixtures/resources";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A burrito export of `repo` at `sha` that holds `files` (ingredient path, bytes), made from a
/// fixture's metadata: the identity, the commit and the ingredients are the export's own.
fn export(repo: &str, sha: &str, files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut metadata: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root().join(FIXTURES).join("en_twl@v86/metadata.json")).unwrap(),
    )
    .unwrap();
    let dcs = metadata["identification"]["primary"]["dcs"]
        .as_object_mut()
        .unwrap();
    dcs.clear();
    dcs.insert(
        repo.to_owned(),
        serde_json::json!({"revision": sha, "timestamp": "2026-10-01T00:00:00.000Z"}),
    );
    let ingredients: serde_json::Map<String, serde_json::Value> = files
        .iter()
        .map(|(path, bytes)| {
            let (md5, mime) = (tc_core::Md5::of(bytes).to_hex(), "text/plain");
            (
                path.clone(),
                serde_json::json!({"checksum": {"md5": md5}, "mimeType": mime, "size": bytes.len()}),
            )
        })
        .collect();
    metadata["ingredients"] = serde_json::Value::Object(ingredients);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("metadata.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&metadata).unwrap())
        .unwrap();
    for (path, bytes) in files {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The Spanish package `es-419_gl` installed on this computer, its notes and links holding the
/// Psalms table under each of `covers`' names: the books it covers.
fn install_package(app_data: &Path, covers: &[BookId]) {
    let folder = app_data.join(tc_app::RESOURCES_FOLDER);
    // The harness shares one read-only store between its scales. This test installs, and a
    // package installed for one scale would otherwise stay installed for the next.
    if folder.is_symlink() {
        #[cfg(unix)]
        std::fs::remove_file(&folder).unwrap();
        // Windows removes a directory link as a directory.
        #[cfg(windows)]
        std::fs::remove_dir(&folder).unwrap();
    } else if folder.is_dir() {
        // The harness copied the corpus (Windows without symlink rights).
        std::fs::remove_dir_all(&folder).unwrap();
    }
    let store = ResourceStore::open(folder).unwrap();
    let scale_dir = root().join("fixtures/scale/psa");
    let tn = std::fs::read(scale_dir.join("en_tn/tn_PSA.tsv")).unwrap();
    let twl = std::fs::read(scale_dir.join("en_twl/twl_PSA.tsv")).unwrap();
    let tables = |table: &[u8]| -> Vec<(String, Vec<u8>)> {
        covers
            .iter()
            .map(|book| (format!("ingredients/{}.tsv", book.code()), table.to_vec()))
            .collect()
    };
    for (n, (name, files)) in [
        ("es-419_tn", tables(&tn)),
        ("es-419_twl", tables(&twl)),
        (
            "es-419_tw",
            vec![(
                "ingredients/payload/kt/god.md".to_owned(),
                b"# God\n".to_vec(),
            )],
        ),
        (
            "es-419_ta",
            vec![(
                "ingredients/translate/translate-toc.yaml".to_owned(),
                b"sections: []\n".to_vec(),
            )],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let sha = format!("{n:x}").repeat(40);
        let pin = ResourcePin::new(
            format!("git.door43.org/es-419_gl/{name}").parse().unwrap(),
            sha.parse().unwrap(),
        )
        .with_version("v1".parse().unwrap());
        install_dcs_zip(
            &store,
            &pin,
            &export(&format!("es-419_gl/{name}"), &sha, &files),
            1 << 30,
        )
        .unwrap();
    }
}

async fn plan(s: &Scale) -> PlanView {
    let at = s.scope();
    let reply = s
        .app
        .request(&PlanGateway {
            request: rid(90),
            at,
            gateway: GatewayRef {
                language: "es-419".into(),
                owner: "es-419_gl".into(),
            },
        })
        .await
        .unwrap();
    let job = match reply {
        tc_api::Reply::Ok(job) => job,
        other => panic!("{other:?}"),
    };
    let mut watch = s.app.jobs().watch(job).unwrap();
    // The bound only catches a hang: GitHub's Windows runner took over 120 s for the plan.
    let state = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        watch.wait_for(tc_app::jobs::JobRegistry::is_ended),
    )
    .await
    .expect("hung: the plan did not end in 600 s")
    .unwrap()
    .clone();
    match state {
        JobState::Done(JobOutcome::PlanReady(view)) => *view,
        other => panic!("{other:?}"),
    }
}

/// The (tool, book) pairs of the project that hold a decision.
fn units_with_decisions(s: &Scale) -> usize {
    let read = s.app.read(&s.scope()).unwrap().read();
    read.projected_books()
        .into_iter()
        .map(|book| {
            [ToolId::TranslationNotes, ToolId::TranslationWords]
                .into_iter()
                .filter(|tool| !read.decisions_of(*tool, book).is_empty())
                .count()
        })
        .sum()
}

/// Opens the project and gives it the two sets a project pins: its primary set with the
/// translation words the English fallback has (the corpus pins notes and links only), and the
/// English fallback with the books it covers recorded, as a computer that has the bundled suite
/// installed reads them. Then installs a package that covers `covers`.
async fn with_package(s: &Scale, covers: &[BookId]) {
    let at = s.open().await;
    let session = s.app.read(&at).unwrap();
    let read = session.read();
    let primary_key = RecordKey::Pins(PinGroup::Primary);
    let Some(Record::Pins { value, .. }) = read.projected(&primary_key) else {
        panic!("the corpus pins its primary set")
    };
    let mut pins = ProjectPins::default();
    pins.set_group_value(PinGroup::Primary, &value).unwrap();
    let corpus_books = read.projected_books();
    let covered = |pin: &ResourcePin| {
        pin.clone()
            .with_books(BookCoverage::Books(corpus_books.clone()))
    };
    let english = tc_resources::suite::english_helps();
    let mut fallback = english.clone();
    for slot in [SetSlot::TranslationNotes, SetSlot::TranslationWordsLinks] {
        let pin = covered(english.pin(slot).unwrap());
        fallback.pins.insert(slot, pin);
    }
    if let Some(primary) = pins.primary.as_mut() {
        primary.pins.insert(
            SetSlot::TranslationWords,
            english.pin(SetSlot::TranslationWords).unwrap().clone(),
        );
    }
    pins.fallback = Some(fallback);
    let write = |group: PinGroup| PlannedChange {
        parents: read.heads(&RecordKey::Pins(group)),
        record: Record::Pins {
            group,
            value: pins.group_value(group).unwrap(),
        },
        cause: Cause::Edit,
    };
    session
        .handle()
        .submit(StoreCommand::apply_upgrade(UpgradePlan {
            changes: vec![write(PinGroup::Primary), write(PinGroup::Fallback)],
        }))
        .await
        .unwrap();
    // The pins are on disk before the plan is counted: the store writes them 3 s after the
    // batch, which a slow machine would put inside the count.
    session
        .handle()
        .checkpoint(CheckpointReason::Other("the plan's test".to_owned()))
        .await;
    install_package(&s.app_data(), covers);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_cost_follows_affected_books() {
    // One book covered: the change affects Psalms, whichever project holds it. (On the store:
    // the memory backend runs no command, and the corpus's pins are first given the words.)
    let (small, large) = costs_on_both(
        Backing::Store,
        async |s: &Scale| with_package(s, &[PSALMS]).await,
        async |s: &Scale| {
            let view = plan(s).await;
            assert!(
                !view.rows.is_empty()
                    && view
                        .rows
                        .iter()
                        .all(|r| r.unit == tc_api::check::CheckCollection::Book(PSALMS)),
                "{view:?}"
            );
        },
    )
    .await;
    assert!(
        small.app.book_usfm > 0,
        "the plan reads what it affects: {small:?}"
    );
    let grew = growth(&small, &large);
    assert!(
        grew.is_empty(),
        "planning a change that affects one book grows with the project:\n{}",
        grew.join("\n")
    );

    // Every book of the larger corpus covered: each book it affects costs a read of its own,
    // and no more than two projections of its text for each tool.
    let books: Vec<BookId> = corpora().larger.fold.projected_books();
    assert!(books.len() > 1, "the larger corpus holds more books");
    let (memory, disk) = (corpora(), store_corpora());
    let probe = scale(Backing::Store, &memory.larger, &disk.larger).await;
    with_package(&probe, &books).await;
    let units = units_with_decisions(&probe);
    let all = cost(&probe, async |s: &Scale| {
        let view = plan(s).await;
        assert!(view.rows.len() > 1, "{view:?}");
    })
    .await;
    assert!(
        all.app.book_usfm > large.app.book_usfm,
        "covering every book costs more than covering one: {} against {}",
        all.app.book_usfm,
        large.app.book_usfm
    );
    assert!(
        all.app.book_usfm <= 2 * units as u64,
        "at most two projections of a book's text for each tool that holds decisions: {} for {units}",
        all.app.book_usfm
    );
    assert!(all.app.keys_probed > large.app.keys_probed);
}
