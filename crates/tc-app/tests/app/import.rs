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
use tc_api::import::{
    CarryOver, FoundVersion, ImportDetails, ImportFile, Review, Run, UpdateBook, UpdateReport,
    VersionSlot, Versions, VersionsView,
};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::refusal::{CreateRefusal, ImportRefusal};
use tc_api::shown::Direction;
use tc_app::StoreChoice;
use tc_core::BookId;
use tc_door43_fake::{CommitSeed, FakeDoor43, ReleaseKind, RepoSeed};
use tc_journal::keys::{PinGroup, ToolId};
use tc_journal::record::{CheckValue, DecisionRecord, VerseValue};
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

/// The notes a new project pins, installed at their commit as the app's seed leaves them.
/// Installed before the app starts.
pub(super) fn install_notes(dir: &Path) {
    use std::io::Write;
    let pin = tc_resources::suite::english_helps()
        .pin(tc_resources::SetSlot::TranslationNotes)
        .unwrap()
        .clone();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for file in super::upgrades::export("en_tn@v91", "unfoldingWord/en_tn") {
        let bytes = if file.path == "metadata.json" {
            let mut metadata: serde_json::Value = serde_json::from_slice(&file.bytes).unwrap();
            for value in metadata["identification"]["primary"]["dcs"]
                .as_object_mut()
                .unwrap()
                .values_mut()
            {
                value["revision"] = serde_json::json!(pin.sha.to_hex());
            }
            serde_json::to_vec(&metadata).unwrap()
        } else {
            file.bytes
        };
        writer
            .start_file(&file.path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let zip = writer.finish().unwrap().into_inner();
    let store =
        tc_resources::ResourceStore::open(dir.join("app-data").join(tc_app::RESOURCES_FOLDER))
            .unwrap();
    tc_resources::install_dcs_zip(&store, &pin, &zip, 1 << 30).unwrap();
}

/// An app over a store world's tc-store, importing with `TcImport`.
pub(super) async fn app() -> (tempfile::TempDir, TestApp, StoreWorld) {
    app_with(None).await
}

/// An app that looks up on `door43`, with the internet on.
async fn app_with(door43: Option<&FakeDoor43>) -> (tempfile::TempDir, TestApp, StoreWorld) {
    let world = store_world();
    let store = world.store();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
    install_notes(dir.path());
    let url = door43.map(|fake| fake.url().to_owned());
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        if let Some(url) = &url {
            config.door43 = url.parse().unwrap();
        }
        deps.store = StoreChoice::Given(store);
        deps.import = Arc::new(tc_app::import::TcImport);
    })
    .await;
    if door43.is_some() {
        post(
            &t.app,
            &tc_api::net::Set {
                request: rid(200),
                on: true,
            },
        )
        .await
        .unwrap();
    }
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
        found: Vec::new(),
        license: "CC BY-SA 4.0".to_owned(),
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
                use_installed: None,
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
            use_installed: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(review.found.details, details("cfm_fbt", "cfm"));
    assert_eq!(review.found.books.len(), 1);
    assert_eq!(review.found.books[0].book, BookId::Titus);
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
            use_installed: None,
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
        report.to_review as usize, in_review,
        "INV-UPDATE-A-BOOK-REPORTS-WHAT-IT-DID: to review counts the verses that wait"
    );
    assert!(!report.book_added);
    assert_eq!(report.book, BookId::Titus);

    // The job deleted the first upload; the person chooses the file again.
    let file = upload(dir.path(), 8, "TIT.usfm", aligned_titus().as_bytes());
    let again = updated(
        &t,
        post(&t.app, &update(2, &at, BookId::Titus, &file))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (again.imported, again.to_review),
        (0, 0),
        "a rerun with nothing new imports nothing and puts nothing in review"
    );
}

/// The review Update a book opens says which version came from the file: an import, not the
/// person on this computer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_review_names_the_file_side_imported() {
    use tc_api::conflicts::{List, MadeBy, ReviewEntries, ReviewTab, ReviewView};
    let (dir, t, _world) = app().await;
    let at = open_sample(&t).await;
    let file = upload(dir.path(), 9, "TIT.usfm", aligned_titus().as_bytes());
    let report = updated(
        &t,
        post(&t.app, &update(1, &at, BookId::Titus, &file))
            .await
            .unwrap(),
    )
    .await;
    assert!(report.to_review > 0);
    let view: ReviewView = post(
        &t.app,
        &List {
            at: at.clone(),
            tab: ReviewTab::Text,
        },
    )
    .await
    .unwrap();
    let ReviewEntries::Conflicts(entries) = view.entries else {
        panic!("{:?}", view.entries)
    };
    assert!(!entries.is_empty());
    for entry in &entries {
        let by: Vec<&MadeBy> = entry.versions.iter().map(|v| &v.by).collect();
        assert!(
            by.contains(&&MadeBy::Import),
            "the file's version reads as imported: {by:?}"
        );
    }
}

/// Update a book stores NFC, as SaveVerse does: the same words in NFD and then in NFC are one
/// text, not a conflict.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_stores_nfc() {
    let (dir, t, _world) = app().await;
    let at = open_sample(&t).await;
    let usfm = |words: &str| format!("\\id TIT\n\\c 1\n\\p\n\\v 6 {words}\n");
    // Titus 1:6 is an empty stub in the sample, so the first file fills it.
    let nfd = upload(
        dir.path(),
        6,
        "TIT.usfm",
        usfm("Cafe\u{301} one.").as_bytes(),
    );
    let first = updated(
        &t,
        post(&t.app, &update(1, &at, BookId::Titus, &nfd))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(first.imported, 1);
    let key = RecordKey::Verse(
        BookId::Titus,
        tc_journal::ChapterNo::new(1).unwrap(),
        tc_journal::VerseNo::new(6).unwrap(),
    );
    assert_eq!(words(&t, &at, &key).as_deref(), Some("Caf\u{e9} one."));

    let waiting_before = waiting(&t, &at);
    let nfc = upload(dir.path(), 7, "TIT.usfm", usfm("Caf\u{e9} one.").as_bytes());
    let second = updated(
        &t,
        post(&t.app, &update(2, &at, BookId::Titus, &nfc))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!((second.imported, second.to_review), (0, 0));
    assert_eq!(
        waiting(&t, &at),
        waiting_before,
        "nothing new waits for review"
    );
}

/// §13.6 U1: a file that does not read is refused with its line, a file of another book is
/// refused, and neither writes anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_a_book_refusal_names_the_line_and_writes_nothing() {
    let (dir, t, world) = app().await;
    let at = open_sample(&t).await;
    let project_files = || files(&world.fs, &world.folder(&folder("sample_burrito")));
    // Opening writes its own files (metadata, resources) in the background; compare after them.
    t.app.read(&at).unwrap().handle().wait_idle();
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
    {
        let after = project_files();
        let changed: Vec<_> = after
            .iter()
            .filter(|(k, v)| before.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .chain(before.keys().filter(|k| !after.contains_key(*k)).cloned())
            .collect();
        assert!(changed.is_empty(), "no file was written: {changed:?}");
    }
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

/// A Door43 repository of unfoldingWord with one published release per tag, each holding
/// `fixture`'s files.
async fn add_releases(fake: &FakeDoor43, name: &str, fixture: &str, tags: &[&str]) {
    fake.add_repo(RepoSeed {
        owner: "unfoldingWord".into(),
        name: name.into(),
        subject: name.into(),
        language: "en".into(),
        topics: vec!["tc-ready".into()],
        private: false,
        commits: tags
            .iter()
            .map(|tag| CommitSeed {
                message: format!("Release {tag}"),
                files: super::upgrades::export(fixture, &format!("unfoldingWord/{name}")),
                tag: Some((*tag).to_owned()),
                release: Some(ReleaseKind::Published),
            })
            .collect(),
    })
    .await
    .unwrap();
}

/// The Titus project with a translationWords decision too (a bookmark on `admonish`), and the
/// translationWords version `en_tw` v87 in its manifest.
fn titus_with_words() -> Vec<u8> {
    titus_edited(&|name, json| {
        if name == "manifest.json" {
            json["externalResources"]["tWordsGateway"] =
                "https://git.door43.org/unfoldingWord/en_tw/archive/v87.zip".into();
        } else if name == "apps/translationCore/index/translationWords/tit/admonish.json" {
            json[0]["reminders"] = true.into();
        }
    })
}

/// The Titus project with its `gu55` notes decision on a check no release of the notes has
/// (`zz99`, on words the verse's notes never quote): a check tC3's version had and the installed
/// one dropped.
fn titus_with_a_dropped_check() -> Vec<u8> {
    titus_edited(&|name, json| {
        if !name.starts_with("apps/translationCore/index/translationNotes/tit/") {
            return;
        }
        for item in json.as_array_mut().into_iter().flatten() {
            let Some(context) = item.get_mut("contextId") else {
                continue;
            };
            if context["checkId"] == "gu55" {
                context["checkId"] = "zz99".into();
                context["quote"] = "οὐδέν".into();
                context["quoteString"] = "οὐδέν".into();
            }
        }
    })
}

/// The Titus project with each JSON file passed through `edit`.
fn titus_edited(edit: &dyn Fn(&str, &mut serde_json::Value)) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut source = zip::ZipArchive::new(std::io::Cursor::new(tc3_titus())).unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for n in 0..source.len() {
        let mut entry = source.by_index(n).unwrap();
        let name = entry.name().to_owned();
        if entry.is_dir() {
            writer
                .add_directory(&name, zip::write::SimpleFileOptions::default())
                .unwrap();
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name.ends_with(".json")
            && let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let before = json.clone();
            edit(&name, &mut json);
            if json != before {
                bytes = serde_json::to_vec(&json).unwrap();
            }
        }
        writer
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

async fn looked_up(t: &TestApp, file: &ImportFile) -> VersionsView {
    let job = post(
        &t.app,
        &Versions {
            request: rid(100),
            files: vec![file.clone()],
        },
    )
    .await
    .unwrap();
    match ended(&t.app, job).await {
        JobState::Done(JobOutcome::VersionsFound(view)) => view,
        other => panic!("the lookup ended {other:?}"),
    }
}

async fn imported(t: &TestApp, file: &ImportFile, found: Vec<FoundVersion>) -> Scope {
    let job = post(
        &t.app,
        &Run {
            found,
            ..run(1, file, details("cfm_fbt", "cfm"))
        },
    )
    .await
    .unwrap();
    let JobState::Done(JobOutcome::Created { project }) = ended(&t.app, job).await else {
        panic!("not imported")
    };
    t.app
        .sessions()
        .open_screen(&project, open_progress())
        .await
        .unwrap()
        .at
}

/// Each decision of `tool` the project holds, by its check id.
fn decisions(t: &TestApp, at: &Scope, tool: ToolId) -> Vec<(String, DecisionRecord)> {
    let session = t.app.read(at).unwrap();
    let snapshot = session.handle().snapshot();
    let fold = snapshot.fold();
    fold.record_keys()
        .into_iter()
        .filter_map(|key| match fold.projected(&key)? {
            Record::Check {
                key,
                value: CheckValue::Decision(decision),
            } if key.tool == tool => Some((key.check_id.as_str().to_owned(), decision)),
            _ => None,
        })
        .collect()
}

/// A slot of one of the project's pin groups, as tC4 writes it in `resources.json`.
fn pinned(t: &TestApp, at: &Scope, group: PinGroup, slot: &str) -> serde_json::Value {
    let session = t.app.read(at).unwrap();
    let Some(Record::Pins { value, .. }) = session
        .handle()
        .snapshot()
        .fold()
        .projected(&RecordKey::Pins(group))
    else {
        panic!("no {group:?} pins")
    };
    serde_json::from_slice(&value.slot(slot).unwrap().to_canonical()).unwrap()
}

/// The lookup resolves each slot's tag to its commit on Door43, derives the links of a found
/// translationWords version from the `_twl` releases (tC4 `twlFor`), and leaves unresolved a
/// slot whose version Door43 does not have.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_version_lookup_answers_from_a_fake_door43() {
    let fake = FakeDoor43::start().await.unwrap();
    add_releases(&fake, "en_tn", "en_tn@v91", &["v87"]).await;
    add_releases(&fake, "el-x-koine_ugnt", "en_ult@v89", &["v0.35"]).await;
    add_releases(&fake, "en_twl", "en_twl@v86", &["v86"]).await;
    add_releases(&fake, "en_tw", "en_tw@v87", &["v87"]).await;
    let sha = |name: &'static str, tag: &'static str| {
        let fake = &fake;
        async move { fake.resolve("unfoldingWord", name, tag).await.unwrap() }
    };
    let (dir, t, _world) = app_with(Some(&fake)).await;
    let file = upload(dir.path(), 1, TC3_TITUS, &titus_with_words());
    let view = looked_up(&t, &file).await;
    let found = |name: &str, slot, tag: &str, sha: String| FoundVersion {
        slot,
        repo_path: format!("git.door43.org/unfoldingWord/{name}"),
        tag: tag.to_owned(),
        sha,
    };
    assert_eq!(
        view.found,
        vec![
            found(
                "en_tn",
                VersionSlot::TranslationNotes,
                "v87",
                sha("en_tn", "v87").await
            ),
            found(
                "en_tw",
                VersionSlot::TranslationWords,
                "v87",
                sha("en_tw", "v87").await
            ),
            found(
                "en_twl",
                VersionSlot::TranslationWordsLinks,
                "v86",
                sha("en_twl", "v86").await
            ),
        ],
        "the tags resolve to their commits, and the words bring their links"
    );
    assert_eq!(
        view.unresolved,
        vec![VersionSlot::OriginalNewTestament],
        "Door43 has no UGNT v0.34"
    );
}

/// tC4 `applyVersions`: a version found on Door43 is the project's pin, it records the books
/// checked against it, and the decisions stay on it, valid, with no carry-over.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_pins_the_versions_found_on_door43() {
    let fake = FakeDoor43::start().await.unwrap();
    add_releases(&fake, "en_tn", "en_tn@v91", &["v87"]).await;
    add_releases(&fake, "el-x-koine_ugnt", "en_ult@v89", &["v0.34"]).await;
    let tn = fake.resolve("unfoldingWord", "en_tn", "v87").await.unwrap();
    let ugnt = fake
        .resolve("unfoldingWord", "el-x-koine_ugnt", "v0.34")
        .await
        .unwrap();
    let (dir, t, _world) = app_with(Some(&fake)).await;
    let file = upload(dir.path(), 1, TC3_TITUS, &tc3_titus());
    let view = looked_up(&t, &file).await;
    let at = imported(&t, &file, view.found).await;

    let notes = pinned(&t, &at, PinGroup::Primary, "translationNotes");
    assert_eq!(
        (
            notes["repoPath"].as_str(),
            notes["version"].as_str(),
            notes["sha"].as_str()
        ),
        (
            Some("git.door43.org/unfoldingWord/en_tn"),
            Some("v87"),
            Some(tn.as_str())
        ),
        "the project pins the notes tC3 checked against"
    );
    assert_eq!(notes["books"], serde_json::json!(["TIT"]));
    let greek = pinned(&t, &at, PinGroup::OriginalLanguage, "nt");
    assert_eq!(greek["sha"].as_str(), Some(ugnt.as_str()));

    let decided = decisions(&t, &at, ToolId::TranslationNotes);
    assert_eq!(decided.len(), 5);
    for (id, decision) in &decided {
        assert_eq!(
            (decision.resource.sha.to_hex(), decision.imported_invalid),
            (tn.clone(), false),
            "{id} stays on the version it was made against"
        );
    }
}

/// tC4 `carryOverDecisions`: with no version found, the decisions move to the installed notes,
/// and one whose check those notes lack keeps its record, marked invalid.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_marks_invalid_a_decision_whose_check_the_installed_pin_lacks() {
    let (dir, t, _world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &titus_with_a_dropped_check());
    let at = imported(&t, &file, Vec::new()).await;
    let installed = tc_resources::suite::english_helps()
        .pin(tc_resources::SetSlot::TranslationNotes)
        .unwrap()
        .resource();
    let decided = decisions(&t, &at, ToolId::TranslationNotes);
    let mut ids: Vec<&str> = decided.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["fyf8", "rtc9", "swi9", "u0gk", "zz99"],
        "no decision is dropped"
    );
    for (id, decision) in &decided {
        assert_eq!(
            decision.resource, installed,
            "{id} is on the installed notes"
        );
        assert_eq!(
            decision.imported_invalid,
            id == "zz99",
            "{id}: only the decision whose check the installed notes lack is invalid"
        );
    }
}

/// tC4 j09 :219 and `alignedVerses`: the Titus project keeps an alignment record for each of
/// its 46 verses, none with an aligned word, so the review counts 0 aligned verses; and it
/// names the slots its versions are for before any lookup (tC4 `NOT_LOOKED_UP`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_review_counts_as_tc4_counts() {
    let (dir, t, _world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &tc3_titus());
    let review = post(
        &t.app,
        &Review {
            files: vec![file],
            details: None,
            use_installed: None,
        },
    )
    .await
    .unwrap();
    let book = &review.found.books[0];
    assert_eq!(
        (book.verses, book.aligned, book.decisions),
        (46, 0, 5),
        "Verses 46, Alignments 0 verses, Checking decisions 5 (tC4 j09 :219)"
    );
    assert_eq!(
        review.found.slots,
        [
            VersionSlot::TranslationNotes,
            VersionSlot::OriginalNewTestament
        ],
        "the versions the project names, before the lookup"
    );
    assert_eq!(
        review.carry_over, None,
        "asked without Use installed versions"
    );
}

/// tC4 `importUseInstalled` and `applyVersions`: Use installed versions says, before anything
/// is written, how many decisions carry over to the installed versions and how many must be
/// checked again; a version found keeps its slot's decisions where they were.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn use_installed_versions_counts_what_carries_over_and_writes_nothing() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &titus_with_a_dropped_check());
    let before = listed(&t.app).await;
    let staged = at_name_or_staging(&world, "cfm_fbt");
    let ask = |found: Vec<FoundVersion>| Review {
        files: vec![file.clone()],
        details: None,
        use_installed: Some(found),
    };
    let review = post(&t.app, &ask(Vec::new())).await.unwrap();
    assert_eq!(
        review.carry_over,
        Some(CarryOver {
            carried: 4,
            invalidated: 1
        }),
        "the installed notes hold four of the five checks"
    );
    let notes = FoundVersion {
        slot: VersionSlot::TranslationNotes,
        repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
        tag: "v87".into(),
        sha: "a".repeat(40),
    };
    let review = post(&t.app, &ask(vec![notes])).await.unwrap();
    assert_eq!(
        review.carry_over,
        Some(CarryOver {
            carried: 0,
            invalidated: 0
        }),
        "the notes found keep their decisions; no other tool has any"
    );
    assert_eq!(listed(&t.app).await, before, "no project was made");
    assert_eq!(at_name_or_staging(&world, "cfm_fbt"), staged);
}

/// A version found for other files is refused: a slot the files do not name, or a version
/// of a named slot they were not checked against.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_refuses_versions_the_files_do_not_name() {
    let (dir, t, _world) = app().await;
    let before = listed(&t.app).await;
    let version = |slot, repo: &str, tag: &str| FoundVersion {
        slot,
        repo_path: format!("git.door43.org/unfoldingWord/{repo}"),
        tag: tag.into(),
        sha: "a".repeat(40),
    };
    let wrong = [
        version(VersionSlot::OriginalOldTestament, "hbo_uhb", "v2.1.30"),
        version(VersionSlot::TranslationNotes, "en_tn", "v1"),
    ];
    for (n, found) in (1..).zip(wrong) {
        let file = upload(dir.path(), n, TC3_TITUS, &tc3_titus());
        let job = post(
            &t.app,
            &Run {
                found: vec![found.clone()],
                ..run(n, &file, details("cfm_fbt", "cfm"))
            },
        )
        .await
        .unwrap();
        let ended = ended(&t.app, job).await;
        assert!(
            matches!(
                &ended,
                JobState::Failed(ApiError::BadRequest { field, .. }) if field == "found"
            ),
            "{found:?}: {ended:?}"
        );
    }
    assert_eq!(listed(&t.app).await, before, "no project was made");
}

/// tC4 j09 :240 and :345: the project's metadata states the license the run names, the files'
/// one or the one picked where they differ, as `copyright.shortStatements`; a license the files
/// do not name is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_import_states_the_license_in_its_metadata() {
    let (dir, t, world) = app().await;
    let file = upload(dir.path(), 1, TC3_TITUS, &tc3_titus());
    let wrong = Run {
        license: "CC0 1.0 Public Domain".into(),
        ..run(1, &file, details("Tita licencia", "cfm"))
    };
    let job = post(&t.app, &wrong).await.unwrap();
    assert!(
        matches!(
            ended(&t.app, job).await,
            JobState::Failed(ApiError::BadRequest { ref field, .. }) if field == "license"
        ),
        "a license the files do not name"
    );
    let file = upload(dir.path(), 2, TC3_TITUS, &tc3_titus());
    let job = post(&t.app, &run(2, &file, details("Tita licencia", "cfm")))
        .await
        .unwrap();
    let JobState::Done(JobOutcome::Created { project }) = ended(&t.app, job).await else {
        panic!("not imported")
    };
    let metadata: serde_json::Value =
        serde_json::from_slice(&files(&world.fs, &world.folder(&project))["metadata.json"])
            .unwrap();
    assert_eq!(
        metadata["copyright"],
        serde_json::json!({ "shortStatements": [{ "statement": "CC BY-SA 4.0" }] })
    );
    assert_valid_burrito(&world.fs, &world.folder(&project));
}

/// tC4 #832, INV-TEXT-DIRECTION-HAS-ONE-SOURCE, INV-VALID-BURRITO: an import keeps the
/// language name and direction the tC3 manifest's
/// `target_language` holds, in metadata.json and in what Settings reads; a manifest with
/// neither, or a code changed on the review step, imports as before: named by its code, left
/// to right.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tc3_import_keeps_the_manifest_language_name_and_direction() {
    let (dir, t, world) = app().await;
    let rtl = titus_edited(&|name, json| {
        if name == "manifest.json" {
            json["target_language"]["direction"] = serde_json::json!(" RTL");
        }
    });
    let bare = titus_edited(&|name, json| {
        if name == "manifest.json" {
            let language = json["target_language"].as_object_mut().unwrap();
            language.remove("name");
            language.remove("direction");
        }
    });
    let cases = [
        (
            tc3_titus(),
            "Tita ltr",
            "Chin, Falam",
            "ltr",
            Direction::Ltr,
        ),
        (
            rtl.clone(),
            "Tita rtl",
            "Chin, Falam",
            "rtl",
            Direction::Rtl,
        ),
        (rtl, "Tita recoded", "cnh", "ltr", Direction::Ltr),
        (bare, "Tita bare", "cfm", "ltr", Direction::Ltr),
    ];
    for (n, (bytes, typed, language, script, direction)) in (1u8..).zip(cases) {
        let file = upload(dir.path(), n, TC3_TITUS, &bytes);
        let code = if typed == "Tita recoded" {
            "cnh"
        } else {
            "cfm"
        };
        let job = post(&t.app, &run(n, &file, details(typed, code)))
            .await
            .unwrap();
        let JobState::Done(JobOutcome::Created { project }) = ended(&t.app, job).await else {
            panic!("{typed}: not imported")
        };
        let metadata: serde_json::Value =
            serde_json::from_slice(&files(&world.fs, &world.folder(&project))["metadata.json"])
                .unwrap();
        assert_eq!(
            metadata["languages"][0],
            serde_json::json!({ "name": { "en": language }, "scriptDirection": script, "tag": code }),
            "{typed}: metadata.json's language"
        );
        assert_valid_burrito(&world.fs, &world.folder(&project));
        let view = post(&t.app, &tc_api::settings::ViewFromHome { project })
            .await
            .unwrap();
        assert_eq!(
            (view.language_name.as_str(), view.direction),
            (language, direction),
            "{typed}: what Settings reads"
        );
    }
}
