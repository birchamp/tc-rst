//! Open Bible Stories (unit B7) through `App::request`: the story rail, one story with its
//! gateway story and pictures, a story's helps and its check lists, on the parity scenarios
//! with their resources installed.

use std::sync::Arc;
use std::time::Duration;

use tc_api::check::{
    CardCount, CheckEmpty, CheckListRef, CheckPlace, CheckSources, CheckUnit, Item,
    Open as CheckOpen, Picker, RailRows, StoryPane, Tool, ToolState,
};
use tc_api::helps::{ArticleLink, ArticleView, ReadySlot, Slot, Story as HelpsStory, StoryArticle};
use tc_api::ids::{Scope, StoryNo};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::session::{Open, OpenUnit, OpenedProject};
use tc_api::story::{self, PackVia, SourceNotice};
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::install_resources_into;
use tc_app::{App, StoreChoice};

use super::common::{Parts, TestApp, folder, rid};
use super::scenario::scenario;
use super::understand::{ok, start};

fn story_no(n: u8) -> StoryNo {
    StoryNo::new(n).unwrap()
}

async fn open_obs(app: &App) -> OpenedProject {
    let job = ok(
        app,
        &Open {
            request: rid(41),
            project: folder("sample_burrito_obs"),
            unit: Some(OpenUnit::Story(story_no(1))),
        },
    )
    .await;
    let mut watch = app.jobs().watch(job).unwrap();
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("the open ends");
    match state {
        JobState::Done(JobOutcome::Opened(opened)) => *opened,
        other => panic!("{other:?}"),
    }
}

async fn story_at(app: &App, at: &Scope, n: u8) -> story::StoryView {
    ok(
        app,
        &story::Open {
            at: at.clone(),
            story: story_no(n),
        },
    )
    .await
}

fn ready<T>(slot: &Slot<T>) -> &ReadySlot<T> {
    match slot {
        Slot::Ready(ready) => ready,
        other => panic!("not ready: {}", describe(other)),
    }
}

fn describe<T>(slot: &Slot<T>) -> &'static str {
    match slot {
        Slot::Ready(_) => "ready",
        Slot::None => "none",
        Slot::Fetch => "fetch",
        Slot::Missing => "missing",
        Slot::OldTwl => "old words links",
        Slot::Error { .. } => "error",
    }
}

fn list(tool: Tool, n: u8) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Story(story_no(n)),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_of_a_story_project_names_its_kind_and_no_books() {
    let (_dir, t) = start("sample").await;
    let opened = open_obs(&t.app).await;
    assert_eq!(opened.kind, tc_api::home::ProjectKind::Stories);
    assert_eq!(opened.name, "Equipo Ejemplo — Historias Bíblicas Abiertas");
    assert!(opened.books.is_empty());
    assert_eq!(opened.place, None, "nothing is remembered yet");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_story_rail_lists_the_fifty_stories_in_order() {
    let (_dir, t) = start("sample").await;
    let at = open_obs(&t.app).await.session.at;
    let rows = ok(&t.app, &story::List { at }).await.stories;
    let numbers: Vec<u8> = rows.iter().map(|r| r.story.get()).collect();
    assert_eq!(numbers, (1..=50).collect::<Vec<u8>>());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bible_project_has_no_story_rail() {
    let (_dir, t) = start("sample").await;
    let at = super::understand::opened(&t.app).await.session.at;
    let reply = t.app.request(&story::List { at }).await.unwrap();
    assert!(matches!(reply, tc_api::Reply::Err(_)), "{reply:?}");
}

/// tC4's `obs-translate` state: story 1 of the sample, whose gateway story and pictures are
/// not on this computer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_opened_story_is_its_file_with_the_title_after_its_number() {
    let (_dir, t) = start("sample").await;
    let at = open_obs(&t.app).await.session.at;
    let view = story_at(&t.app, &at, 1).await;
    assert_eq!(
        view.title, "La Creación",
        "the number is the file's, not the title's"
    );
    assert_eq!(view.reference, "Una historia bíblica de: Génesis 1-2");
    assert_eq!(view.frames.len(), 16);
    let drafted: Vec<usize> = view
        .frames
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.text.is_empty())
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(drafted, [1, 2], "two frames of sixteen are drafted: 13%");
    assert!(
        view.frames[0]
            .text
            .starts_with("Así fue como Dios hizo todo")
    );
    assert!(
        view.frames[0].text.contains('\n'),
        "a frame's lines stay as written"
    );
    assert_eq!(view.comments, []);
    // No gateway story is on this computer: the pin is named, with its version.
    assert_eq!(view.gateway, None);
    assert_eq!(
        view.source,
        Some(SourceNotice::NotInstalled {
            pin: "git.door43.org/es-419_gl/es-419_obs@4a239590d543".to_owned()
        })
    );
    assert_eq!(view.source_version.as_deref(), Some("v2"));
    // No picture pack either: a missing picture is a note, never a frame error.
    assert!(view.frames.iter().all(|f| f.image.is_none()));
    let note = view.image_note.expect("the note says why");
    assert_eq!(note.wanted, 16);
    assert!(
        note.packs
            .iter()
            .all(|p| p.pin == "git.door43.org/uW/obs_images_360@7146d5b504f6"
                && p.installed_at.is_none()
                && p.error.as_deref() == Some("not installed")),
        "{note:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pinned_gateway_story_sits_beside_the_project_s() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let view = story_at(&t.app, &at, 1).await;
    let gateway = view.gateway.expect("the English story is installed");
    assert_eq!(gateway.title, "The Creation");
    assert_eq!(gateway.reference, "A Bible story from: Genesis 1-2");
    assert_eq!(gateway.frames.len(), 16);
    assert!(gateway.frames[0].starts_with("This is how God made everything"));
    assert_eq!(view.source, None, "it was read and matches");
    assert_eq!(view.source_version.as_deref(), Some("v9"));
    // The scenario installs the story set but no pictures.
    assert!(view.image_note.is_some());
}

/// The gateway has story 1 only (tC4's mock serves one): another story says so, and keeps
/// every frame's own text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_the_gateway_lacks_says_so_and_keeps_the_projects_text() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let view = story_at(&t.app, &at, 2).await;
    assert_eq!(view.gateway, None);
    assert!(
        matches!(&view.source, Some(SourceNotice::Error { message }) if message.contains("no story 2")),
        "{:?}",
        view.source
    );
    assert_eq!(view.title, "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_outside_the_project_is_refused() {
    let (_dir, t) = start("sample").await;
    let at = open_obs(&t.app).await.session.at;
    // Story numbers are 1 to 50 on the wire; the project holds all fifty, so a project that
    // lacks one would be refused: the request names a number no project could hold.
    let json = format!(
        r#"{{"at":{},"story":51}}"#,
        serde_json::to_string(&at).unwrap()
    );
    assert!(serde_json::from_str::<story::Open>(&json).is_err());
}

// ---------------------------------------------------------------------------------------
// Pictures
// ---------------------------------------------------------------------------------------

const PICTURE: [u8; 4] = [0xFF, 0xD8, 0xFF, 0xD9];

/// A burrito zip of the default picture pack (`uW/obs_images_360`) holding frames 1 and 2 of
/// story 1.
fn pictures_zip() -> Vec<u8> {
    use std::io::Write;
    let pin = tc_resources::suite::obs_images();
    let names = ["360px/obs-en-01-01.jpg", "360px/obs-en-01-02.jpg"];
    let mut ingredients = serde_json::Map::new();
    for name in names {
        ingredients.insert(
            format!("ingredients/{name}"),
            serde_json::json!({
                "checksum": {"md5": tc_burrito::Md5::of(&PICTURE).to_hex()},
                "mimeType": "image/jpeg",
                "size": PICTURE.len(),
            }),
        );
    }
    // The metadata of a real resource export, made the pack's: its commit, its flavor and the
    // two pictures.
    let mut metadata: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            super::common::repo_root()
                .join("fixtures/tc4/test-fixtures/resources/en_obs-tn@v13/metadata.json"),
        )
        .unwrap(),
    )
    .unwrap();
    metadata["identification"]["primary"]["dcs"] = serde_json::json!({
        "uW/obs_images_360": {"revision": pin.sha.to_hex(), "timestamp": "2026-09-17T18:20:53.766Z"}
    });
    metadata["type"]["flavorType"]["flavor"]["name"] = "x-obsimages".into();
    metadata["ingredients"] = ingredients.into();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("metadata.json", options).unwrap();
    writer.write_all(metadata.to_string().as_bytes()).unwrap();
    for name in names {
        writer
            .start_file(format!("ingredients/{name}"), options)
            .unwrap();
        writer.write_all(&PICTURE).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

async fn start_with_pictures(name: &str) -> (tempfile::TempDir, TestApp) {
    let state = scenario(name);
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path().join("app-data");
    install_resources_into(&app_data, &state.resources().unwrap()).unwrap();
    let store = tc_resources::ResourceStore::open(app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    tc_resources::install_dcs_zip(
        &store,
        tc_resources::suite::obs_images(),
        &pictures_zip(),
        1 << 30,
    )
    .unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await;
    (dir, t)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_installed_pack_gives_its_frames_their_pictures_and_the_route_serves_them() {
    let (_dir, t) = start_with_pictures("sample").await;
    let at = open_obs(&t.app).await.session.at;
    let view = story_at(&t.app, &at, 1).await;
    let pack = "git.door43.org~uW~obs_images_360~7146d5b504f6b63b9e11f7dc0b18c594d0ae179d";
    assert_eq!(
        view.frames[0].image.as_deref(),
        Some(format!("/media/obs/{pack}/360px/obs-en-01-01.jpg").as_str())
    );
    assert_eq!(
        view.frames[1].image.as_deref(),
        Some(format!("/media/obs/{pack}/360px/obs-en-01-02.jpg").as_str())
    );
    assert!(
        view.frames[2..].iter().all(|f| f.image.is_none()),
        "the pack holds no picture for the other frames"
    );
    assert_eq!(view.image_note, None, "a picture was found");
    // The route gives back the file, and only a picture the pack lists.
    let file = t
        .app
        .obs_picture(pack, "360px/obs-en-01-01.jpg")
        .await
        .expect("served");
    assert_eq!(
        (file.bytes.as_slice(), file.content_type),
        (&PICTURE[..], "image/jpeg")
    );
    for refused in [
        "360px/obs-en-01-03.jpg",
        "../metadata.json",
        "metadata.json",
        "360px/../../metadata.json",
    ] {
        assert!(
            t.app.obs_picture(pack, refused).await.is_none(),
            "{refused}"
        );
    }
    assert!(
        t.app
            .obs_picture(
                "git.door43.org~uW~obs_images_360~0000",
                "360px/obs-en-01-01.jpg"
            )
            .await
            .is_none()
    );
    assert!(
        t.app
            .obs_picture(
                "git.door43.org~nobody~nothing~7146d5b504f6b63b9e11f7dc0b18c594d0ae179d",
                "a.jpg"
            )
            .await
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pack_report_names_where_the_pack_is_and_how_it_was_listed() {
    let (_dir, t) = start_with_pictures("sample").await;
    let at = open_obs(&t.app).await.session.at;
    // Story 3's pictures are not in the pack: its frames show none, and the note names the
    // pack with its file count.
    let view = story_at(&t.app, &at, 3).await;
    let note = view.image_note.expect("no picture of story 3 was found");
    let installed: Vec<_> = note
        .packs
        .iter()
        .filter(|p| p.installed_at.is_some())
        .collect();
    assert!(!installed.is_empty(), "{note:?}");
    assert!(
        installed
            .iter()
            .all(|p| p.via == Some(PackVia::Paths) && p.files == 2)
    );
}

// ---------------------------------------------------------------------------------------
// The helps
// ---------------------------------------------------------------------------------------

async fn helps_at(app: &App, at: &Scope, n: u8) -> tc_api::helps::PassageView {
    ok(
        app,
        &HelpsStory {
            at: at.clone(),
            story: story_no(n),
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stories_helps_are_cards_of_its_frames_with_the_story_as_the_chapter() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let helps = helps_at(&t.app, &at, 1).await;
    let notes = ready(&helps.notes);
    assert_eq!(notes.items.len(), 91, "story 1's notes: tC4's `0 of 91`");
    assert!(notes.items.iter().all(|n| n.chapter.as_str() == "1"));
    let first = &notes.items[0];
    assert_eq!(
        (first.verse.as_str(), first.quote.as_str()),
        ("0", "The Creation")
    );
    assert!(!first.note.is_empty());
    assert!(
        notes.items.iter().all(|n| n.panes.is_empty()),
        "no pane to light words in"
    );
    let words = ready(&helps.words);
    assert_eq!(words.items.len(), 35);
    assert_eq!(
        (
            words.items[0].verse.as_str(),
            words.items[0].slug.as_str(),
            words.items[0].category.as_deref()
        ),
        ("1", "god", Some("kt"))
    );
    assert_eq!(ready(&helps.questions).items.len(), 22);
    assert!(
        matches!(helps.simplified, Slot::None),
        "a story has no simplified text"
    );
    assert_eq!(helps.other_book, None);
    // Another story's cards are that story's.
    let other = helps_at(&t.app, &at, 2).await;
    assert!(
        ready(&other.notes)
            .items
            .iter()
            .all(|n| n.chapter.as_str() == "2")
    );
}

/// tC4's `obs-understand`: the pinned notes and word links are not on this computer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helps_the_computer_lacks_are_stated_not_empty() {
    let (_dir, t) = start("sample").await;
    let at = open_obs(&t.app).await.session.at;
    let helps = helps_at(&t.app, &at, 1).await;
    assert_eq!(describe(&helps.notes), "fetch");
    assert_eq!(describe(&helps.words), "fetch");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_articles_of_a_story_s_helps_are_read_from_the_story_project_s_resources() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    // The scenario's Translation Academy holds a few dozen articles; this is one of them.
    let academy = "figs-idiom".to_owned();
    let article = ok(
        &t.app,
        &StoryArticle {
            at: at.clone(),
            link: ArticleLink::Academy { slug: academy },
        },
    )
    .await;
    assert!(
        matches!(&article, ArticleView::Found { blocks, .. } if !blocks.is_empty()),
        "{article:?}"
    );
    let word = ok(
        &t.app,
        &StoryArticle {
            at: at.clone(),
            link: ArticleLink::Words {
                category: Some("kt".into()),
                slug: "god".into(),
            },
        },
    )
    .await;
    assert!(
        matches!(&word, ArticleView::Found { title, .. } if title == "God"),
        "{word:?}"
    );
    let missing = ok(
        &t.app,
        &StoryArticle {
            at,
            link: ArticleLink::Words {
                category: Some("kt".into()),
                slug: "no-such-word".into(),
            },
        },
    )
    .await;
    assert_eq!(missing, ArticleView::Missing);
}

// ---------------------------------------------------------------------------------------
// The checks
// ---------------------------------------------------------------------------------------

fn story_picker(at: &Scope) -> Picker {
    Picker {
        at: at.clone(),
        unit: CheckUnit::Story(story_no(1)),
    }
}

/// tC4's `obs-check-picker`: both tools need their es-419 pins downloaded, and a story has no
/// Align card.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_story_picker_needs_the_pinned_resources_and_has_no_align_card() {
    let (_dir, t) = start("sample").await;
    let at = open_obs(&t.app).await.session.at;
    let view = ok(&t.app, &story_picker(&at)).await;
    let tools: Vec<Tool> = view.tools.iter().map(|c| c.tool).collect();
    assert_eq!(tools, [Tool::TranslationWords, Tool::TranslationNotes]);
    assert!(view.tools.iter().all(|c| c.state == ToolState::Fetch));
    assert_eq!(
        view.tools[0].pin.as_ref().unwrap().repo_path,
        "git.door43.org/es-419_gl/es-419_obs-twl"
    );
    assert_eq!(
        view.tools[1].pin.as_ref().unwrap().repo_path,
        "git.door43.org/es-419_gl/es-419_obs-tn"
    );
    assert_eq!(view.align, None, "the entry is absent, not disabled");
    // A tool that is not ready does not open.
    let reply = t
        .app
        .request(&CheckOpen {
            at,
            list: list(Tool::TranslationNotes, 1),
            since: None,
        })
        .await
        .unwrap();
    assert!(
        matches!(reply, tc_api::Reply::Err(tc_api::ApiError::ToolNotReady)),
        "{reply:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_story_picker_counts_the_open_story_s_checks() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let view = ok(&t.app, &story_picker(&at)).await;
    let counted = |i: usize| match &view.tools[i].progress {
        Some(CardCount::Counted(progress)) => progress.clone(),
        other => panic!("{other:?}"),
    };
    let (words, notes) = (counted(0), counted(1));
    assert_eq!((words.decided, words.total), (0, 35));
    assert_eq!((notes.decided, notes.total), (0, 91));
    let next = notes.next.expect("the first check is next");
    assert_eq!(next.title, "The Creation");
    assert_eq!(next.place.to_string(), "OBS 1:0");
    // Another story counts its own.
    let two = ok(
        &t.app,
        &Picker {
            at,
            unit: CheckUnit::Story(story_no(2)),
        },
    )
    .await;
    let Some(CardCount::Counted(second)) = &two.tools[1].progress else {
        panic!("{:?}", two.tools[1].progress)
    };
    assert_ne!(second.total, 91, "story 2 has its own notes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_session_lists_that_story_s_checks_in_frame_order() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let session = ok(
        &t.app,
        &CheckOpen {
            at: at.clone(),
            list: list(Tool::TranslationNotes, 1),
            since: None,
        },
    )
    .await;
    assert_eq!(session.empty, None);
    assert_eq!((session.progress.decided, session.progress.total), (0, 91));
    assert_eq!(session.start, 0, "the first check not decided");
    let citation = session.citation.expect("the list names its resource");
    assert_eq!(
        citation.pin.repo_path,
        "git.door43.org/unfoldingWord/en_obs-tn"
    );
    assert_eq!(citation.pin.version.as_deref(), Some("v13"));
    let RailRows::Full(rows) = &session.rows else {
        panic!("a first open sends the whole rail")
    };
    assert_eq!(rows.len(), 91);
    let places: Vec<(u8, u16)> = rows
        .iter()
        .map(|r| match &r.place {
            CheckPlace::Frame { story, frame } => (story.get(), *frame),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(places.iter().all(|(story, _)| *story == 1));
    let mut sorted = places.clone();
    sorted.sort_by_key(|(_, frame)| *frame);
    assert_eq!(
        places, sorted,
        "the title (frame 0) first, then every frame in order"
    );
    assert_eq!(rows[0].title, "The Creation");
    assert_eq!(rows[1].title, "the beginning");
    // Words: the same session for the other tool.
    let words = ok(
        &t.app,
        &CheckOpen {
            at,
            list: list(Tool::TranslationWords, 1),
            since: None,
        },
    )
    .await;
    assert_eq!(words.progress.total, 35);
    assert_eq!(words.empty, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_with_no_check_says_so() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    // The English set has notes for story 1 only in this scenario's tN: find a story with
    // none, if any, else the list of a story is simply that story's.
    for n in 1..=50u8 {
        let session = ok(
            &t.app,
            &CheckOpen {
                at: at.clone(),
                list: list(Tool::TranslationWords, n),
                since: None,
            },
        )
        .await;
        if session.progress.total == 0 {
            assert_eq!(session.empty, Some(CheckEmpty::None));
            return;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_check_shows_the_gateway_frame_with_its_quote_lit_and_the_projects_text() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let item = |index| Item {
        at: at.clone(),
        list: list(Tool::TranslationNotes, 1),
        index,
    };
    let pane = |view: &tc_api::check::CheckItemView| match &view.sources {
        CheckSources::Story { pane } => pane.clone(),
        other => panic!("{other:?}"),
    };
    // The title's check (frame 0): the gateway title lit whole, the project's title as words.
    let title = ok(&t.app, &item(0)).await;
    assert_eq!(title.place.to_string(), "OBS 1:0");
    assert_eq!(title.title, "The Creation");
    assert_eq!(title.target.words, ["La", "Creación"]);
    assert!(
        title.target.verse.is_some(),
        "the frame's token, for a mark"
    );
    assert!(!title.note.is_empty(), "the note's blocks");
    assert_eq!(
        title.helps_dir,
        Some(tc_api::shown::Direction::Ltr),
        "the English set's notes read left to right"
    );
    let StoryPane::Text { tokens, lit } = pane(&title) else {
        panic!("{:?}", title.sources)
    };
    assert_eq!(tokens.concat(), "The Creation");
    let lit_text: Vec<&str> = lit.iter().map(|i| tokens[*i as usize].as_str()).collect();
    assert_eq!(lit_text, ["The", "Creation"]);
    // A frame's check: only the quote's own words are lit.
    let frame = ok(&t.app, &item(1)).await;
    assert_eq!(frame.title, "the beginning");
    let StoryPane::Text { tokens, lit } = pane(&frame) else {
        panic!("{:?}", frame.sources)
    };
    assert!(
        tokens
            .concat()
            .starts_with("This is how God made everything in the beginning.")
    );
    let lit_text: Vec<&str> = lit.iter().map(|i| tokens[*i as usize].as_str()).collect();
    assert_eq!(lit_text, ["the", "beginning"]);
    assert_eq!(frame.target.words[..4], ["Así", "fue", "como", "Dios"]);
    // A check of an undrafted frame has no text of the project's to select from.
    let undrafted = ok(&t.app, &item(90)).await;
    assert!(undrafted.target.words.is_empty());
    // A word list's item links to its tW article.
    let word = ok(
        &t.app,
        &Item {
            at: at.clone(),
            list: list(Tool::TranslationWords, 1),
            index: 0,
        },
    )
    .await;
    assert_eq!(
        word.article.link,
        ArticleLink::Words {
            category: Some("kt".into()),
            slug: "god".into()
        }
    );
    // An index the list lacks is refused.
    assert!(matches!(
        t.app.request(&item(91)).await.unwrap(),
        tc_api::Reply::Err(tc_api::ApiError::BadRequest { .. })
    ));
}

/// Reading a story, its helps and its checks changes nothing in the project (tC4 #501): the
/// snapshot's sequence number is what a publication moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_a_story_writes_nothing() {
    let (_dir, t) = start("en-pins").await;
    let at = open_obs(&t.app).await.session.at;
    let seq = || t.app.read(&at).unwrap().read().seq();
    let before = seq();
    story_at(&t.app, &at, 1).await;
    ok(&t.app, &story::List { at: at.clone() }).await;
    helps_at(&t.app, &at, 1).await;
    ok(&t.app, &story_picker(&at)).await;
    ok(
        &t.app,
        &CheckOpen {
            at: at.clone(),
            list: list(Tool::TranslationNotes, 1),
            since: None,
        },
    )
    .await;
    ok(
        &t.app,
        &Item {
            at: at.clone(),
            list: list(Tool::TranslationNotes, 1),
            index: 0,
        },
    )
    .await;
    assert_eq!(seq(), before);
}
