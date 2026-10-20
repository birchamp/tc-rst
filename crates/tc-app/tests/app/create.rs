//! Create a project, add books and save settings (unit C6) through `App::request`, on tc-store
//! over its in-memory fault-injecting file system: J1 and J20's server side, `home/languages`'
//! codes through create, the settings contract and Save from a Home card.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::Refusal;
use tc_api::create::{
    AddBlankBooks, AddUsfmBooks, Bible, BooksAdded, Details, Obs, ReviewUsfm, UsfmFile,
    UsfmOutcome, UsfmProblem, UsfmVerdict,
};
use tc_api::home::{List, ProjectKind};
use tc_api::ids::{ProjectRef, UploadId};
use tc_api::refusal::{CreateRefusal, SettingsRefusal};
use tc_api::settings::{Save, SettingsView, ShownValues, ViewFromHome};
use tc_api::shown::{Direction, Known};
use tc_app::StoreChoice;
use tc_app::backend::{ProjectHandle, StoreCommand};
use tc_core::BookId;
use tc_journal::{Json, Record, RecordKey};

use super::common::{Parts, TestApp, post, rid};
use crate::counting_store::{CountingStore, SharedStore};
use crate::store_world::{StoreWorld, assert_valid_burrito, files, store_world};

const FONT: &str = "Scheherazade New — Arabic script";

fn kanuri() -> Details {
    Details {
        name: "Kanuri Bible".to_owned(),
        language_name: "Kanuri".to_owned(),
        code: "kr".to_owned(),
        door43_code: None,
        direction: Direction::Rtl,
        font: FONT.to_owned(),
    }
}

fn bible(request: u8, details: Details) -> Bible {
    Bible {
        request: rid(request),
        details,
        versification: "eng".to_owned(),
    }
}

/// An app over a store world's tc-store, with the store counting its opens.
async fn app() -> (
    tempfile::TempDir,
    TestApp,
    StoreWorld,
    Arc<CountingStore<SharedStore>>,
) {
    let world = store_world();
    let store = Arc::new(CountingStore::new(SharedStore(world.store())));
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store.clone());
    })
    .await;
    (dir, t, world, store)
}

fn project_files(
    world: &StoreWorld,
    project: &ProjectRef,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    files(&world.fs, &world.folder(project))
}

fn metadata(world: &StoreWorld, project: &ProjectRef) -> serde_json::Value {
    let files = project_files(world, project);
    serde_json::from_slice(files.get("metadata.json").expect("metadata.json")).unwrap()
}

fn refusal(error: ApiError) -> Refusal {
    match error {
        ApiError::Refused(refusal) => refusal,
        other => panic!("{other:?}"),
    }
}

/// Puts a file where the server's upload route would have stored it.
fn upload(dir: &Path, n: u8, name: &str, bytes: &[u8]) -> UsfmFile {
    let id = UploadId::parse(&format!("{n:032x}")).unwrap();
    std::fs::write(dir.join("app-data/uploads").join(id.as_str()), bytes).unwrap();
    UsfmFile {
        upload: id,
        name: name.to_owned(),
    }
}

fn aligned_titus() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm"),
    )
    .unwrap()
}

/// J1: New Bible's Create makes a project that is listed, keeps its language, direction, font
/// and Door43 code, and becomes a valid Scripture Burrito with its first book.
#[tokio::test]
async fn new_bible_is_created_whole_and_a_blank_book_makes_it_a_valid_burrito() {
    let (_dir, t, world, _store) = app().await;
    let details = Details {
        code: "acm-x-lebarabi-c".to_owned(),
        door43_code: Some("acm-x-lebarabic".to_owned()),
        ..kanuri()
    };
    let created = post(&t.app, &bible(1, details)).await.unwrap();
    assert_eq!(created.project.as_str(), "kanuri_bible");

    let home = post(&t.app, &List {}).await.unwrap();
    assert!(
        home.cards
            .iter()
            .any(|card| card.project == created.project)
    );
    // The settings dialog reads what the person chose, from the project's history, though no
    // derived file exists before the first book.
    let view = post(
        &t.app,
        &ViewFromHome {
            project: created.project.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(view.language_name, "Kanuri");
    assert_eq!(view.direction, Direction::Rtl);
    assert_eq!(view.font.as_deref(), Some(FONT));
    assert_eq!(
        view.gateway.current.as_ref().unwrap().gateway.language,
        "en"
    );

    let added = post(
        &t.app,
        &AddBlankBooks {
            request: rid(2),
            project: created.project.clone(),
            books: vec![BookId::Titus, BookId::Titus],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        added,
        BooksAdded {
            added: vec![BookId::Titus]
        }
    );

    assert_valid_burrito(&world.fs, &world.folder(&created.project));
    let meta = metadata(&world, &created.project);
    assert_eq!(meta["languages"][0]["tag"], "acm-x-lebarabi-c");
    assert_eq!(meta["languages"][0]["scriptDirection"], "rtl");
    assert_eq!(meta["languages"][0]["name"]["en"], "Kanuri");
    assert_eq!(meta["identification"]["name"]["en"], "Kanuri Bible");
    let titus =
        String::from_utf8(project_files(&world, &created.project)["ingredients/TIT.usfm"].clone())
            .unwrap();
    // Titus has three chapters of 16, 15 and 15 verses; every verse is a stub.
    assert_eq!(titus.matches("\\c ").count(), 3, "{titus}");
    assert_eq!(titus.matches("___").count(), 46, "{titus}");
    // The settings file keeps the Door43 code beside the project's own (Q35).
    let settings: serde_json::Value = serde_json::from_slice(
        &project_files(&world, &created.project)["ingredients/checking/settings.json"],
    )
    .unwrap();
    assert_eq!(settings["door43Language"]["code"], "acm-x-lebarabic");
    assert_eq!(settings["textDirection"], "rtl");
    // The card lists the book after the add.
    let home = post(&t.app, &List {}).await.unwrap();
    let card = home
        .cards
        .iter()
        .find(|c| c.project == created.project)
        .unwrap();
    match &card.books {
        Known::Known(books) => assert_eq!(books.len(), 1),
        other => panic!("{other:?}"),
    }
}

/// A-27: the Create button sends one request for a press; a retry after a lost reply gets the
/// first reply and makes nothing twice.
#[tokio::test]
async fn a_create_sent_twice_with_one_request_id_makes_one_project() {
    let (_dir, t, world, _store) = app().await;
    let first = post(&t.app, &bible(7, kanuri())).await.unwrap();
    let again = post(&t.app, &bible(7, kanuri())).await.unwrap();
    assert_eq!(first, again);
    let names: Vec<_> = t
        .app
        .sessions()
        .store()
        .list()
        .unwrap()
        .projects
        .into_iter()
        .filter(|p| p.folder.as_str().starts_with("kanuri"))
        .collect();
    assert_eq!(names.len(), 1);
    assert!(
        world
            .fs
            .peek(
                &world
                    .folder(&first.project)
                    .join("ingredients/tc/project.json")
            )
            .is_some()
    );
}

/// tC4 `wizard.nameInUse`, `nameRequired`, `codeRequired`, `abbrRequired`; and #582: a language
/// no project can hold is refused before any folder exists.
#[tokio::test]
async fn create_refuses_what_the_form_and_the_burrito_refuse_and_writes_nothing() {
    let (_dir, t, world, _store) = app().await;
    let before = files(&world.fs, Path::new("/projects")).len();
    let ask = |details| bible(9, details);
    let refused = |details| {
        let t = &t;
        async move {
            // A new request id each time: this is a new press, not a retry.
            static NEXT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(100);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            refusal(
                post(
                    &t.app,
                    &Bible {
                        request: rid(id),
                        ..ask(details)
                    },
                )
                .await
                .unwrap_err(),
            )
        }
    };
    assert_eq!(
        refused(Details {
            name: " ".into(),
            ..kanuri()
        })
        .await,
        Refusal::Create(CreateRefusal::NameRequired)
    );
    assert_eq!(
        refused(Details {
            code: String::new(),
            ..kanuri()
        })
        .await,
        Refusal::Create(CreateRefusal::CodeRequired)
    );
    assert_eq!(
        refused(Details {
            name: "Библия".into(),
            code: "кр".into(),
            ..kanuri()
        })
        .await,
        Refusal::Create(CreateRefusal::AbbrRequired)
    );
    match refused(Details {
        code: "acm-x-lebarabic".into(),
        ..kanuri()
    })
    .await
    {
        Refusal::Create(CreateRefusal::CodeNotHeld { code, .. }) => {
            assert_eq!(code, "acm-x-lebarabic");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(files(&world.fs, Path::new("/projects")).len(), before);
    // A name that makes the folder of an existing project.
    assert_eq!(
        refused(Details {
            name: "Sample Burrito".into(),
            ..kanuri()
        })
        .await,
        Refusal::Create(CreateRefusal::NameInUse {
            abbr: "sample_burrito".into()
        })
    );
    assert_eq!(files(&world.fs, Path::new("/projects")).len(), before);
}

/// tC4 #386: regional and private-use tags are kept as the person chose them.
#[tokio::test]
async fn create_keeps_regional_language_tags() {
    let (_dir, t, world, _store) = app().await;
    for (n, tag) in ["es-419", "pt-BR", "x-ug"].into_iter().enumerate() {
        let details = Details {
            name: format!("Tagged {n}"),
            code: tag.to_owned(),
            ..kanuri()
        };
        let created = post(&t.app, &bible(20 + n as u8, details)).await.unwrap();
        post(
            &t.app,
            &AddBlankBooks {
                request: rid(30 + n as u8),
                project: created.project.clone(),
                books: vec![BookId::Philemon],
            },
        )
        .await
        .unwrap();
        assert_eq!(
            metadata(&world, &created.project)["languages"][0]["tag"],
            tag
        );
    }
}

/// J20: New Open Bible Stories makes a project of fifty stories that is a valid Burrito.
#[tokio::test]
async fn new_obs_makes_fifty_stories() {
    let (_dir, t, world, _store) = app().await;
    let created = post(
        &t.app,
        &Obs {
            request: rid(3),
            details: Details {
                name: "Kanuri Stories".into(),
                ..kanuri()
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(created.project.as_str(), "kanuri_stories");
    let files = project_files(&world, &created.project);
    let stories = files
        .keys()
        .filter(|p| p.starts_with("ingredients/content/") && p.ends_with(".md"))
        .count();
    assert_eq!(stories, 50, "{:?}", files.keys().collect::<Vec<_>>());
    assert_valid_burrito(&world.fs, &world.folder(&created.project));
    let home = post(&t.app, &List {}).await.unwrap();
    let card = home
        .cards
        .iter()
        .find(|c| c.project == created.project)
        .unwrap();
    assert_eq!(card.kind, ProjectKind::Stories);
}

/// Add books from USFM files (tC4 #484, #767, #553): each file a book in one batch, the
/// alignments split out, the others refused in their own rows, the uploads deleted.
#[tokio::test]
async fn usfm_files_add_whole_books_and_refuse_the_rest_in_their_rows() {
    let (dir, t, world, _store) = app().await;
    let project = post(&t.app, &bible(4, kanuri())).await.unwrap().project;
    let good = upload(dir.path(), 1, "57-TIT.usfm", &aligned_titus());
    let twin = upload(dir.path(), 2, "TIT-copy.usfm", &aligned_titus());
    let no_id = upload(dir.path(), 3, "notes.txt", b"just some notes\n");
    let not_text = upload(dir.path(), 4, "binary.usfm", &[0xff, 0xfe, 0x00]);
    let damaged = upload(
        dir.path(),
        5,
        "bad.usfm",
        b"\\id GEN\n\\c 1\n\\v 1 In\n\\c 1\n\\v 2 again\n",
    );
    assert_eq!(
        post(&t.app, &ReviewUsfm { file: good.clone() })
            .await
            .unwrap(),
        UsfmVerdict::Book {
            book: BookId::Titus
        }
    );
    assert!(matches!(
        post(
            &t.app,
            &ReviewUsfm {
                file: damaged.clone()
            }
        )
        .await
        .unwrap(),
        UsfmVerdict::Refused(UsfmProblem::Damaged { .. })
    ));
    let added = post(
        &t.app,
        &AddUsfmBooks {
            request: rid(5),
            project: project.clone(),
            files: vec![
                good.clone(),
                twin.clone(),
                no_id.clone(),
                not_text.clone(),
                damaged.clone(),
            ],
        },
    )
    .await
    .unwrap();
    let outcomes: Vec<_> = added.rows.iter().map(|r| r.outcome.clone()).collect();
    assert_eq!(
        outcomes[0],
        UsfmOutcome::Added {
            book: BookId::Titus
        }
    );
    assert_eq!(
        outcomes[1],
        UsfmOutcome::Refused(UsfmProblem::TwoFiles {
            book: BookId::Titus,
            first: "57-TIT.usfm".into()
        })
    );
    assert_eq!(outcomes[2], UsfmOutcome::Refused(UsfmProblem::NoId));
    assert_eq!(outcomes[3], UsfmOutcome::Refused(UsfmProblem::NotText));
    assert!(matches!(
        outcomes[4],
        UsfmOutcome::Refused(UsfmProblem::Damaged { .. })
    ));
    // The book is in, aligned, valid, and no upload is left.
    assert_valid_burrito(&world.fs, &world.folder(&project));
    let usfm =
        String::from_utf8(project_files(&world, &project)["ingredients/TIT.usfm"].clone()).unwrap();
    assert!(usfm.contains("\\c 3"), "the book has its chapters");
    let alignments = project_files(&world, &project)
        .keys()
        .filter(|p| p.contains("alignment"))
        .count();
    assert!(alignments > 0, "an alignment sidecar is written");
    for file in [&good, &twin, &no_id, &not_text, &damaged] {
        assert!(
            !dir.path()
                .join("app-data/uploads")
                .join(file.upload.as_str())
                .exists()
        );
    }
    // The same book again is refused as present, and nothing changes.
    let again = upload(dir.path(), 6, "again.usfm", &aligned_titus());
    let added = post(
        &t.app,
        &AddUsfmBooks {
            request: rid(6),
            project: project.clone(),
            files: vec![again],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        added.rows[0].outcome,
        UsfmOutcome::Refused(UsfmProblem::Exists {
            book: BookId::Titus
        })
    );
}

fn setting(project_handle: &dyn ProjectHandle, key: &str, text: &str) -> StoreCommand {
    let key = RecordKey::Setting(tc_journal::keys::SettingKey::parse(key).unwrap());
    StoreCommand::set_setting(
        project_handle.snapshot().mint(&key, None).unwrap(),
        Some(Json::Str(text.to_owned())),
    )
}

async fn settings_view(t: &TestApp, project: &ProjectRef) -> SettingsView {
    post(
        &t.app,
        &ViewFromHome {
            project: project.clone(),
        },
    )
    .await
    .unwrap()
}

fn save(
    request: u8,
    project: &ProjectRef,
    view: &SettingsView,
    direction: Direction,
    font: &str,
) -> Save {
    Save {
        request: rid(request),
        project: project.clone(),
        shown: ShownValues {
            direction: view.direction,
            font: view.font.clone(),
        },
        direction,
        font: font.to_owned(),
    }
}

/// The settings contract: what the dialog shows is what Save compares and what it writes. Only
/// the settings that changed are written, and Home reads the new values from the derived file.
#[tokio::test]
async fn settings_save_writes_the_changes_and_the_dialog_reads_them_back() {
    let (_dir, t, world, _store) = app().await;
    let project = world.bible.clone();
    let view = settings_view(&t, &project).await;
    assert_eq!((view.direction, view.font.clone()), (Direction::Ltr, None));
    post(&t.app, &save(1, &project, &view, Direction::Rtl, FONT))
        .await
        .unwrap();
    let view = settings_view(&t, &project).await;
    assert_eq!(
        (view.direction, view.font.as_deref()),
        (Direction::Rtl, Some(FONT))
    );
    // Only the font changes: the direction is not written again.
    let before = project_files(&world, &project)["ingredients/checking/settings.json"].clone();
    post(
        &t.app,
        &save(2, &project, &view, Direction::Rtl, "Charis SIL"),
    )
    .await
    .unwrap();
    let view = settings_view(&t, &project).await;
    assert_eq!(view.font.as_deref(), Some("Charis SIL"));
    assert_ne!(
        project_files(&world, &project)["ingredients/checking/settings.json"],
        before
    );
    // Saving what is shown changes nothing.
    let same = project_files(&world, &project)["ingredients/checking/settings.json"].clone();
    post(
        &t.app,
        &save(3, &project, &view, Direction::Rtl, "Charis SIL"),
    )
    .await
    .unwrap();
    assert_eq!(
        project_files(&world, &project)["ingredients/checking/settings.json"],
        same
    );
    // A font the project never set and the dialog's first font write nothing either.
    let other = world.other.clone();
    let view = settings_view(&t, &other).await;
    post(
        &t.app,
        &save(
            4,
            &other,
            &view,
            Direction::Ltr,
            tc_api::settings::DEFAULT_FONT,
        ),
    )
    .await
    .unwrap();
    assert!(
        !project_files(&world, &other).contains_key("ingredients/checking/settings.json")
            || !String::from_utf8_lossy(
                &project_files(&world, &other)["ingredients/checking/settings.json"]
            )
            .contains("textFont")
    );
}

/// AD14: a save mints tokens only for the values the dialog showed. When the project holds
/// others (a teammate's work arrived since), nothing is saved and the dialog is told to look
/// again.
#[tokio::test]
async fn settings_from_home_mints_only_for_shown_values() {
    let (_dir, t, world, _store) = app().await;
    let project = world.bible.clone();
    let view = settings_view(&t, &project).await;
    // A change lands after the dialog read: the direction becomes rtl.
    {
        let handle = world.open(&project);
        handle
            .submit(setting(handle.as_ref(), "textDirection", "rtl"))
            .await
            .unwrap();
        handle.clone().close().await.unwrap();
    }
    let error = post(&t.app, &save(1, &project, &view, Direction::Ltr, FONT))
        .await
        .unwrap_err();
    assert_eq!(refusal(error), Refusal::Settings(SettingsRefusal::Changed));
    // Nothing of the save was written: the font is still not set, the direction still rtl.
    let view = settings_view(&t, &project).await;
    assert_eq!((view.direction, view.font.clone()), (Direction::Rtl, None));
    // With the values the project holds, the same save goes through.
    post(&t.app, &save(2, &project, &view, Direction::Ltr, FONT))
        .await
        .unwrap();
    let view = settings_view(&t, &project).await;
    assert_eq!(
        (view.direction, view.font.as_deref()),
        (Direction::Ltr, Some(FONT))
    );
}

/// AD38: a card clicked while a settings save opens the project shares the save's session: the
/// store opens the folder once, and the save lands in the session the screen has.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn card_click_during_settings_save_shares_the_session() {
    let (_dir, t, world, store) = app().await;
    let project = world.bible.clone();
    let view = settings_view(&t, &project).await;
    store.hold_open(&project);
    let saving = {
        let app = t.app.clone();
        let (project, request) = (
            project.clone(),
            save(1, &project, &view, Direction::Rtl, FONT),
        );
        tokio::spawn(async move { post(&app, &request).await.map(|()| project) })
    };
    super::common::until("the save's open reaches the store", || {
        store.opens(&project) == 1
    })
    .await;
    let click = {
        let sessions = t.app.sessions().clone();
        let project = project.clone();
        tokio::spawn(async move {
            sessions
                .open_screen(&project, super::common::open_progress())
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    store.release_open(&project);
    saving.await.unwrap().unwrap();
    let open = click.await.unwrap().unwrap();
    assert_eq!(store.opens(&project), 1, "the store opened the folder once");
    let session = t.app.sessions().reader(&open.at).unwrap();
    let held = session.read().projected(&RecordKey::Setting(
        tc_journal::keys::SettingKey::parse("textDirection").unwrap(),
    ));
    assert!(
        matches!(held, Some(Record::Setting { value: Some(Json::Str(ref d)), .. }) if d == "rtl"),
        "{held:?}"
    );
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.sessions().settled(&project).await;
}

/// tC4 #574: a blank book takes its chapters, verses and paragraphs from the project's pinned
/// literal Bible when this computer has it, and from the versification when it has not.
#[tokio::test]
async fn a_blank_book_is_seeded_from_the_pinned_literal_pane() {
    let resources = super::scenario::scenario("sample").resources().unwrap();
    // The sample's resources are installed before the app starts.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
    tc_app::backend::scenario::install_resources_into(&dir.path().join("app-data"), &resources)
        .unwrap();
    let world = store_world();
    let store = Arc::new(CountingStore::new(SharedStore(world.store())));
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store.clone());
    })
    .await;
    let project = post(&t.app, &bible(1, kanuri())).await.unwrap().project;
    post(
        &t.app,
        &AddBlankBooks {
            request: rid(2),
            project: project.clone(),
            books: vec![BookId::Titus],
        },
    )
    .await
    .unwrap();
    let titus =
        String::from_utf8(project_files(&world, &project)["ingredients/TIT.usfm"].clone()).unwrap();
    assert_eq!(titus.matches("___").count(), 46, "{titus}");
    assert!(
        titus.matches("\\p").count() + titus.matches("\\q").count() > 3,
        "the literal Bible's paragraphs, not one for each chapter: {titus}"
    );
}
