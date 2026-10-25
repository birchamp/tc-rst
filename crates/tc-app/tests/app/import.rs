//! Import a translationCore 3 project and Update a book from USFM (unit F1), through
//! `App::request` on tc-store over its in-memory fault-injecting file system, with the real
//! import port (`TcImport`) and tC4's own import fixtures.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use tc_api::ApiError;
use tc_api::Refusal;
use tc_api::create::{Bible, Details};
use tc_api::ids::{JobId, ProjectRef, Scope, UploadId};
use tc_api::import::{ImportDetails, ImportFile, Review, Run, UpdateBook, UpdateReport};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::refusal::{CreateRefusal, ImportRefusal};
use tc_api::shown::Direction;
use tc_app::StoreChoice;
use tc_core::BookId;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};
use tc_store::hooks::Point;

use super::common::{Parts, TestApp, open_progress, post, rid};
use super::port_contract::{ended, listed};
use crate::store_world::{StoreWorld, assert_valid_burrito, files, store_world};

const TC3_TITUS: &str = "cfm_fbt_tit_book.zip";

pub(super) fn tc3_titus() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/conformance-fixtures/import/tc3")
            .join(TC3_TITUS),
    )
    .unwrap()
}

pub(super) fn aligned_titus() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm"),
    )
    .unwrap()
}

/// An app over a store world's tc-store, importing with `TcImport`.
pub(super) async fn app() -> (tempfile::TempDir, TestApp, StoreWorld) {
    let world = store_world();
    let store = world.store();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store);
        deps.import = Arc::new(tc_app::import::TcImport);
    })
    .await;
    (dir, t, world)
}

/// Puts a file where the server's upload route would have stored it.
pub(super) fn upload(dir: &Path, n: u8, name: &str, bytes: &[u8]) -> ImportFile {
    let id = UploadId::parse(&format!("{n:032x}")).unwrap();
    std::fs::write(dir.join("app-data/uploads").join(id.as_str()), bytes).unwrap();
    ImportFile {
        upload: id,
        name: name.to_owned(),
    }
}

fn details(name: &str, code: &str) -> ImportDetails {
    ImportDetails {
        name: name.to_owned(),
        code: code.to_owned(),
    }
}

pub(super) fn run(n: u8, file: &ImportFile, details: ImportDetails) -> Run {
    Run {
        request: rid(n),
        files: vec![file.clone()],
        details,
    }
}

fn refusal(error: ApiError) -> Refusal {
    match error {
        ApiError::Refused(refusal) => refusal,
        other => panic!("{other:?}"),
    }
}

fn folder(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

/// Every file at `name` in the projects folder, and every staging folder.
fn at_name_or_staging(world: &StoreWorld, name: &str) -> Vec<String> {
    world
        .fs
        .files_under(Path::new(crate::store_world::PROJECTS))
        .into_keys()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| p.contains(&format!("/{name}/")) || p.contains(".staging-"))
        .collect()
}

/// INV-AN-IMPORT-NEVER-LEAVES-A-HALF-PROJECT: a full disk leaves no project and no folder at
/// its name; while the create is held before its rename, the projects folder holds nothing at
/// the name and Home lists nothing, and a cancel then ends with the whole project or none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_or_stopped_import_leaves_no_project() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &tc3_titus());
    let before = listed(&t.app).await;

    world.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    let job: JobId = post(&t.app, &run(1, &file, details("cfm_fbt", "cfm")))
        .await
        .unwrap();
    assert_eq!(
        ended(&t.app, job).await,
        JobState::Failed(ApiError::Refused(Refusal::Import(
            ImportRefusal::WriteFailed
        )))
    );
    world.fs.fail_writes(None);
    assert_eq!(listed(&t.app).await, before, "a full disk adds no project");
    assert!(
        world
            .fs
            .peek(&world.folder(&folder("cfm_fbt")).join("metadata.json"))
            .is_none(),
        "INV-AN-IMPORT-NEVER-LEAVES-A-HALF-PROJECT: nothing at the name"
    );

    world.hooks.hold(Point::BeforeStagingRename);
    let job: JobId = post(&t.app, &run(2, &file, details("cfm_fbt", "cfm")))
        .await
        .unwrap();
    let hooks = world.hooks.clone();
    tokio::task::spawn_blocking(move || hooks.wait_reached(Point::BeforeStagingRename, 1))
        .await
        .unwrap();
    assert!(
        !at_name_or_staging(&world, "cfm_fbt")
            .iter()
            .any(|p| p.contains("/cfm_fbt/")),
        "INV-AN-IMPORT-NEVER-LEAVES-A-HALF-PROJECT: the projects folder holds nothing at the \
         name before the run ends"
    );
    assert_eq!(listed(&t.app).await, before, "Home lists nothing yet");
    t.app.jobs().cancel(job);
    world.hooks.release(Point::BeforeStagingRename);
    match ended(&t.app, job).await {
        JobState::Done(JobOutcome::Created { project }) => {
            assert_eq!(project, folder("cfm_fbt"));
            assert!(listed(&t.app).await.contains(&project));
            assert_valid_burrito(&world.fs, &world.folder(&project));
        }
        JobState::Cancelled => assert_eq!(listed(&t.app).await, before),
        other => panic!("{other:?}"),
    }
    assert!(
        !at_name_or_staging(&world, "cfm_fbt")
            .iter()
            .any(|p| p.contains(".staging-")),
        "no staging folder is left"
    );
}

/// INV-ONE-LANGUAGE-CODE-RULE (#835): the review and the run accept and refuse the codes
/// create does, before anything is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn import_accepts_x_codes_and_refuses_unusable_codes_before_import() {
    let (dir, t, world) = app().await;
    for (n, code) in ["x-ug", "acm-x-lebarabic", "e"].into_iter().enumerate() {
        let n = u8::try_from(n).unwrap();
        // A run that made its project drops its uploads, as the dialog then closes.
        let file = upload(dir.path(), 1 + n, TC3_TITUS, &tc3_titus());
        let created = post(
            &t.app,
            &Bible {
                request: rid(40 + n),
                details: Details {
                    name: format!("Created {n}"),
                    language_name: "Uyghur".to_owned(),
                    code: code.to_owned(),
                    door43_code: None,
                    direction: Direction::Ltr,
                    font: "Noto Sans (default)".to_owned(),
                },
                versification: "eng".to_owned(),
            },
        )
        .await;
        let name = format!("Imported {n}");
        let review = post(
            &t.app,
            &Review {
                files: vec![file.clone()],
                details: Some(details(&name, code)),
            },
        )
        .await
        .unwrap();
        let before = listed(&t.app).await;
        let ran = post(&t.app, &run(50 + n, &file, details(&name, code))).await;
        match created {
            Ok(_) => {
                assert_eq!(
                    review.refusal, None,
                    "INV-ONE-LANGUAGE-CODE-RULE: create keeps {code}, so import does"
                );
                let job = ran.unwrap();
                let JobState::Done(JobOutcome::Created { project }) = ended(&t.app, job).await
                else {
                    panic!("{code} was not imported")
                };
                let metadata: serde_json::Value = serde_json::from_slice(
                    &files(&world.fs, &world.folder(&project))["metadata.json"],
                )
                .unwrap();
                assert_eq!(metadata["languages"][0]["tag"], code);
            }
            Err(error) => {
                assert!(
                    matches!(
                        refusal(error),
                        Refusal::Create(CreateRefusal::CodeNotHeld { .. })
                    ),
                    "{code}"
                );
                assert!(
                    matches!(
                        review.refusal,
                        Some(Refusal::Create(CreateRefusal::CodeNotHeld { .. }))
                    ),
                    "INV-ONE-LANGUAGE-CODE-RULE: create refuses {code}, so the review does: {:?}",
                    review.refusal
                );
                assert!(
                    matches!(
                        ran,
                        Err(ApiError::Refused(Refusal::Create(
                            CreateRefusal::CodeNotHeld { .. }
                        )))
                    ),
                    "{code}: {ran:?}"
                );
                assert_eq!(listed(&t.app).await, before, "nothing was written");
            }
        }
    }
}

/// #499: the Bible name typed on the review step is the project's name, and its folder is
/// the one that name makes; a name in use is refused on the review and by the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_import_name_is_the_project_name() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &tc3_titus());
    let review = post(
        &t.app,
        &Review {
            files: vec![file.clone()],
            details: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(review.found.details, details("cfm_fbt", "cfm"));
    assert_eq!(review.found.books.len(), 1);
    assert_eq!(review.found.books[0].book, BookId::Titus);
    assert_eq!(review.found.books[0].aligned, 46);
    assert_eq!(review.found.books[0].decisions, 5);

    let typed = details("Tita del tC3", "cfm");
    let job = post(&t.app, &run(1, &file, typed.clone())).await.unwrap();
    let JobState::Done(JobOutcome::Created { project }) = ended(&t.app, job).await else {
        panic!("not imported")
    };
    assert_eq!(project, folder("tita_del_tc3"));
    let metadata: serde_json::Value =
        serde_json::from_slice(&files(&world.fs, &world.folder(&project))["metadata.json"])
            .unwrap();
    assert_eq!(
        metadata["identification"]["name"]["en"], "Tita del tC3",
        "#499: the typed name is the project's name"
    );
    assert_valid_burrito(&world.fs, &world.folder(&project));

    let file = upload(dir.path(), 2, TC3_TITUS, &tc3_titus());
    let again = post(
        &t.app,
        &Review {
            files: vec![file.clone()],
            details: Some(typed.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        again.refusal,
        Some(Refusal::Import(ImportRefusal::NameExists)),
        "a name in use is refused on the review (tC4 j09 :367)"
    );
    assert_eq!(
        post(&t.app, &run(2, &file, typed)).await,
        Err(ApiError::Refused(Refusal::Import(
            ImportRefusal::NameExists
        )))
    );
}

/// The open sample's scope.
pub(super) async fn open_sample(t: &TestApp) -> Scope {
    t.app
        .sessions()
        .open_screen(&folder("sample_burrito"), open_progress())
        .await
        .unwrap()
        .at
}

pub(super) fn update(n: u8, at: &Scope, book: BookId, file: &ImportFile) -> UpdateBook {
    UpdateBook {
        request: rid(n),
        at: at.clone(),
        book,
        file: file.clone(),
    }
}

fn words(t: &TestApp, at: &Scope, key: &RecordKey) -> Option<String> {
    match t
        .app
        .read(at)
        .unwrap()
        .handle()
        .snapshot()
        .fold()
        .projected(key)?
    {
        Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        } => Some(words),
        _ => None,
    }
}

/// The records with more than one version standing: what waits in the conflict review.
fn waiting(t: &TestApp, at: &Scope) -> BTreeSet<RecordKey> {
    let session = t.app.read(at).unwrap();
    let snapshot = session.handle().snapshot();
    let fold = snapshot.fold();
    fold.record_keys()
        .into_iter()
        .filter(|k| fold.heads(k).len() > 1)
        .collect()
}

fn titus_verses(t: &TestApp, at: &Scope) -> Vec<RecordKey> {
    let session = t.app.read(at).unwrap();
    let snapshot = session.handle().snapshot();
    snapshot
        .fold()
        .record_keys()
        .into_iter()
        .filter(|k| matches!(k, RecordKey::Verse(BookId::Titus, ..)))
        .collect()
}

async fn updated(t: &TestApp, job: JobId) -> UpdateReport {
    match ended(&t.app, job).await {
        JobState::Done(JobOutcome::BookUpdated(report)) => report,
        other => panic!("{other:?}"),
    }
}

/// INV-UPDATE-A-BOOK-REPORTS-WHAT-IT-DID (§13.6 U3, U6): the report's counts are the batches'.
/// A verse the file filled is counted as imported; every verse that waits in the review is
/// counted as to review, never as imported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_report_counts_match_the_batch() {
    let (dir, t, _world) = app().await;
    let at = open_sample(&t).await;
    let verses = titus_verses(&t, &at);
    let before: Vec<Option<String>> = verses.iter().map(|k| words(&t, &at, k)).collect();
    let waiting_before = waiting(&t, &at);
    let file = upload(dir.path(), 2, "TIT.usfm", aligned_titus().as_bytes());
    let report = updated(
        &t,
        post(&t.app, &update(1, &at, BookId::Titus, &file))
            .await
            .unwrap(),
    )
    .await;

    let conflicts: BTreeSet<RecordKey> = waiting(&t, &at)
        .difference(&waiting_before)
        .cloned()
        .collect();

    let filled = verses
        .iter()
        .zip(&before)
        .filter(|(k, was)| words(&t, &at, k) != **was && !conflicts.contains(*k))
        .count();
    let in_review = verses.iter().filter(|k| conflicts.contains(*k)).count();
    assert!(filled > 0 && in_review > 0, "{filled} {in_review}");
    assert_eq!(
        report.imported as usize, filled,
        "INV-UPDATE-A-BOOK-REPORTS-WHAT-IT-DID: imported counts the verses filled, and no verse \
         in conflict"
    );
    assert_eq!(
        report.to_review as usize,
        conflicts.len(),
        "INV-UPDATE-A-BOOK-REPORTS-WHAT-IT-DID: to review counts every record that waits"
    );
    assert!(report.to_review as usize >= in_review);
    assert!(!report.book_added);
    assert_eq!(report.book, BookId::Titus);
}

/// §13.6 U1: a file that does not read is refused with its line, a file of another book is
/// refused, and neither writes anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_refusal_names_the_line_and_writes_nothing() {
    let (dir, t, world) = app().await;
    let at = open_sample(&t).await;
    let project_files = || files(&world.fs, &world.folder(&folder("sample_burrito")));
    let before = project_files();
    let keys = || {
        t.app
            .read(&at)
            .unwrap()
            .handle()
            .snapshot()
            .fold()
            .record_keys()
            .len()
    };
    let records = keys();

    let twice = "\\id TIT\n\\c 1\n\\p\n\\v 1 One.\n\\v 1 One again.\n";
    let file = upload(dir.path(), 3, "TIT.usfm", twice.as_bytes());
    match post(&t.app, &update(1, &at, BookId::Titus, &file)).await {
        Err(ApiError::Refused(Refusal::Import(ImportRefusal::UpdateUnreadable {
            file,
            line,
            ..
        }))) => {
            assert_eq!(file, "TIT.usfm");
            assert_eq!(line, 5, "the refusal names the line");
        }
        other => panic!("{other:?}"),
    }

    let other = aligned_titus().replacen("\\id TIT", "\\id PHM", 1);
    let file = upload(dir.path(), 4, "PHM.usfm", other.as_bytes());
    assert_eq!(
        post(&t.app, &update(2, &at, BookId::Titus, &file)).await,
        Err(ApiError::Refused(Refusal::Import(
            ImportRefusal::UpdateOtherBook {
                file: "PHM.usfm".to_owned(),
                named: "PHM".to_owned(),
                chosen: "TIT".to_owned(),
            }
        )))
    );
    assert_eq!(keys(), records, "no record was written");
    assert_eq!(project_files(), before, "no file was written");
}

/// #409: an aligned USFM verse whose source word carries `x-ref` keeps it in the stored
/// alignment record (Update a book; a tC3 alignment goes through the same decoder of tC4's
/// record).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_keeps_x_ref() {
    let (dir, t, _world) = app().await;
    let at = open_sample(&t).await;
    // Titus 1:6 is an empty stub in the sample, so the file fills it with its alignment.
    let usfm = aligned_titus();
    let at_six = usfm.find("\\v 6 ").unwrap();
    // After the six attributes tC4 names, where an aligned file writes any other.
    let zaln = at_six + usfm[at_six..].find("\\zaln-s |").unwrap();
    let end = zaln + usfm[zaln..].find("\"\\*").unwrap() + 1;
    let with_ref = format!("{} x-ref=\"1:6\"{}", &usfm[..end], &usfm[end..]);
    let file = upload(dir.path(), 5, "TIT.usfm", with_ref.as_bytes());
    updated(
        &t,
        post(&t.app, &update(1, &at, BookId::Titus, &file))
            .await
            .unwrap(),
    )
    .await;
    let key = RecordKey::Alignment(
        BookId::Titus,
        tc_journal::ChapterNo::new(1).unwrap(),
        tc_journal::VerseNo::new(6).unwrap(),
    );
    let session = t.app.read(&at).unwrap();
    let Some(Record::Alignment { value, .. }) = session.handle().snapshot().fold().projected(&key)
    else {
        panic!("Titus 1:6 has no alignment")
    };
    let first = &value.alignments[0].top_words[0];
    assert_eq!(
        first.extra.get("x-ref").map(|v| v.to_string()),
        Some("\"1:6\"".to_owned()),
        "#409: the source word keeps x-ref: {first:?}"
    );
}
