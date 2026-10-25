//! What the Community Checking preview prints (unit B8) through `App::request`: the sample's
//! Titus and Jonah and its story project, each as tC4's own print code states it in
//! `fixtures/export/print-vectors.json`. Then D2's exports: the conflict check before one,
//! the Story Markdown zip, and the print page a print export leaves for `GET /print/{job}`.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;
use tc_api::ApiError;
use tc_api::export::{self, PrintFrame, PrintItem, PrintStoryItem};
use tc_api::ids::{DownloadId, JobId, Scope};
use tc_api::jobs::{JobKind, JobOutcome, JobState, Phase};
use tc_app::jobs::JobSpec;
use tc_core::BookId;

use super::common::{ManualJob, post, repo_root, rid};
use super::understand::{ok, opened, start};

fn vectors() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let text = std::fs::read_to_string(repo_root().join("fixtures/export/print-vectors.json"))
            .unwrap();
        serde_json::from_str(&text).unwrap()
    })
}

fn case(kind: &str, name: &str) -> &'static Value {
    vectors()[kind]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()
}

/// A book's print written as the vectors write `items`, each verse with what it prints.
fn written(print: &export::BookPrint) -> Value {
    Value::Array(
        print
            .items
            .iter()
            .map(|item| match item {
                PrintItem::Gap(run) => serde_json::json!({ "gap": [run.from, run.to] }),
                PrintItem::Chapter(c) => serde_json::json!({
                    "c": c.chapter,
                    "verses": c.verses.iter().map(|v| {
                        serde_json::json!([v.label.as_str(), v.text])
                    }).collect::<Vec<_>>(),
                }),
            })
            .collect(),
    )
}

/// tC4's items for a book case, each verse with its text when it prints any.
fn their_written(case: &Value) -> Value {
    let text_of = |c: u64, n: &str| -> Value {
        let verse = case["byChapter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["key"].as_str().unwrap().parse::<u64>().ok() == Some(c))
            .flat_map(|e| e["verses"].as_array().unwrap())
            .find(|v| v["n"] == n)
            .unwrap();
        let text = verse["text"].as_str().unwrap();
        if verse["drafted"].as_bool().unwrap() && !text.is_empty() {
            text.into()
        } else {
            Value::Null
        }
    };
    Value::Array(
        case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                if item.get("gap").is_some() {
                    return item.clone();
                }
                let c = item["c"].as_u64().unwrap();
                serde_json::json!({
                    "c": c,
                    "verses": item["verses"].as_array().unwrap().iter().map(|n| {
                        let n = n.as_str().unwrap();
                        serde_json::json!([n, text_of(c, n)])
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

async fn book(app: &tc_app::App, at: &Scope, book: BookId) -> export::BookPrint {
    ok(
        app,
        &export::Book {
            at: at.clone(),
            book,
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_samples_books_print_as_tc4_prints_them() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    for (code, name) in [
        (BookId::Titus, "titus-sample"),
        (BookId::Jonah, "jonah-sample"),
    ] {
        let theirs = case("books", name);
        let print = book(&t.app, &at, code).await;
        assert_eq!(written(&print), their_written(theirs), "{name}");
        assert_eq!(Value::Bool(print.undrafted), theirs["undrafted"], "{name}");
    }
    // Titus prints chapter 1 only: verses 1 to 5 are drafted, 6 to 16 print as not yet
    // drafted, and the undrafted chapters 2 and 3 come after the last drafted one.
    let titus = book(&t.app, &at, BookId::Titus).await;
    let [PrintItem::Chapter(chapter)] = titus.items.as_slice() else {
        panic!("{titus:?}")
    };
    assert_eq!(chapter.chapter, 1);
    let drafted: Vec<&str> = chapter
        .verses
        .iter()
        .filter(|v| v.text.is_some())
        .map(|v| v.label.as_str())
        .collect();
    assert_eq!(drafted, ["1", "2", "3", "4", "5"]);
    assert_eq!(chapter.verses.len(), 16);
}

/// The preview states a verse as Translate holds it: the same verses, drafted the same way,
/// with the same words. Translate's grid runs a poem's lines into one; the print keeps the
/// line breaks, which its flow sets as spaces.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_preview_prints_the_verses_translate_shows() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let rows = ok(
        &t.app,
        &tc_api::editor::Chapters {
            at: at.clone(),
            book: BookId::Jonah,
        },
    )
    .await;
    let print = book(&t.app, &at, BookId::Jonah).await;
    for item in &print.items {
        let PrintItem::Chapter(chapter) = item else {
            continue;
        };
        let row = rows
            .chapters
            .iter()
            .find(|c| c.chapter == chapter.chapter)
            .unwrap();
        let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let theirs: Vec<(&str, bool, String)> = row
            .verses
            .iter()
            .map(|v| (v.key.as_str(), v.drafted, words(&v.text)))
            .collect();
        let ours: Vec<(&str, bool, String)> = chapter
            .verses
            .iter()
            .map(|v| {
                (
                    v.label.as_str(),
                    v.text.is_some(),
                    words(v.text.as_deref().unwrap_or_default()),
                )
            })
            .collect();
        assert_eq!(ours, theirs, "chapter {}", chapter.chapter);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_book_outside_the_project_is_refused() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let reply = t
        .app
        .request(&export::Book {
            at,
            book: BookId::Genesis,
        })
        .await
        .unwrap();
    assert!(matches!(reply, tc_api::Reply::Err(_)), "{reply:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_story_sample_prints_its_drafted_story_as_tc4_prints_it() {
    let (_dir, t) = start("sample").await;
    let at = super::story::open_obs(&t.app).await.session.at;
    let print = ok(&t.app, &export::Stories { at: at.clone() }).await;
    let theirs = case("stories", "obs-sample");
    assert_eq!(Value::Bool(print.undrafted), theirs["undrafted"]);
    let ours: Vec<Value> = print
        .items
        .iter()
        .map(|item| match item {
            PrintStoryItem::Gap(run) => {
                serde_json::json!({ "gap": [run.from.get(), run.to.get()] })
            }
            PrintStoryItem::Story(story) => serde_json::json!({
                "story": story.story.get(),
                "frames": story.frames.iter().map(|f| match f {
                    PrintFrame::Gap(run) => serde_json::json!({ "gap": [run.from, run.to] }),
                    PrintFrame::Drafted(frame) => {
                        serde_json::json!({ "n": frame.frame, "text": frame.text })
                    }
                }).collect::<Vec<_>>(),
            }),
        })
        .collect();
    assert_eq!(Value::Array(ours), theirs["items"]);
    let [PrintStoryItem::Story(story)] = print.items.as_slice() else {
        panic!("{print:?}")
    };
    assert_eq!(story.title, "La Creación");
    assert_eq!(
        story.reference.as_deref(),
        Some("Una historia bíblica de: Génesis 1-2")
    );
    // No picture pack is on this computer: no frame has a picture.
    assert!(
        story
            .frames
            .iter()
            .all(|f| !matches!(f, PrintFrame::Drafted(d) if d.picture.is_some()))
    );
    // A Bible book of a story project is refused.
    let reply = t
        .app
        .request(&export::Book {
            at,
            book: BookId::Titus,
        })
        .await
        .unwrap();
    assert!(matches!(reply, tc_api::Reply::Err(_)), "{reply:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drafted_frame_prints_the_picture_its_pack_has() {
    let (_dir, t) = super::story::start_with_pictures("sample").await;
    let at = super::story::open_obs(&t.app).await.session.at;
    let print = ok(&t.app, &export::Stories { at }).await;
    let [PrintStoryItem::Story(story)] = print.items.as_slice() else {
        panic!("{print:?}")
    };
    let pack = "git.door43.org~uW~obs_images_360~7146d5b504f6b63b9e11f7dc0b18c594d0ae179d";
    let pictures: Vec<Option<String>> = story
        .frames
        .iter()
        .filter_map(|f| match f {
            PrintFrame::Drafted(d) => Some(d.picture.clone()),
            PrintFrame::Gap(_) => None,
        })
        .collect();
    assert_eq!(
        pictures,
        [1, 2].map(|n| Some(format!("/media/obs/{pack}/360px/obs-en-01-0{n}.jpg")))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bible_project_has_no_stories_to_print() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let reply = t.app.request(&export::Stories { at }).await.unwrap();
    assert!(matches!(reply, tc_api::Reply::Err(_)), "{reply:?}");
}

/// tC4 #782: Genesis 1:1 of the en_ult aligns "the heavens" inside another alignment, two levels
/// down. A book added from that file prints the whole verse, as Translate shows it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_preview_prints_the_words_inside_nested_alignments() {
    use tc_api::create::{AddUsfmBooks, Bible, Details, UsfmFile, UsfmOutcome};
    use tc_api::ids::UploadId;
    use tc_api::session::{Open, OpenUnit};
    use tc_api::shown::Direction;
    use tc_app::StoreChoice;

    use super::common::{Parts, TestApp, post, rid};

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
    let world = crate::store_world::store_world();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(world.store());
    })
    .await;
    let project = post(
        &t.app,
        &Bible {
            request: rid(1),
            details: Details {
                name: "English Bible".to_owned(),
                language_name: "English".to_owned(),
                code: "en".to_owned(),
                door43_code: None,
                direction: Direction::Ltr,
                font: String::new(),
            },
            versification: "eng".to_owned(),
        },
    )
    .await
    .unwrap()
    .project;
    let upload = UploadId::parse(&format!("{:032x}", 1)).unwrap();
    std::fs::copy(
        repo_root().join("fixtures/tc4/test-fixtures/en_ult/GEN-1.usfm"),
        dir.path().join("app-data/uploads").join(upload.as_str()),
    )
    .unwrap();
    let added = post(
        &t.app,
        &AddUsfmBooks {
            request: rid(2),
            project: project.clone(),
            files: vec![UsfmFile {
                upload,
                name: "01-GEN.usfm".to_owned(),
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        added.rows[0].outcome,
        UsfmOutcome::Added {
            book: BookId::Genesis
        }
    );
    let job = ok(
        &t.app,
        &Open {
            request: rid(3),
            project,
            unit: Some(OpenUnit::Book(BookId::Genesis)),
        },
    )
    .await;
    let mut watch = t.app.jobs().watch(job).unwrap();
    let state = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("hung: the open ends");
    let at = match state {
        tc_api::jobs::JobState::Done(tc_api::jobs::JobOutcome::Opened(opened)) => opened.session.at,
        other => panic!("{other:?}"),
    };
    let print = book(&t.app, &at, BookId::Genesis).await;
    let Some(PrintItem::Chapter(one)) = print.items.first() else {
        panic!("{print:?}")
    };
    let first = one.verses.first().unwrap();
    assert_eq!(first.label.as_str(), "1");
    let words = first
        .text
        .as_deref()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        words,
        "In the beginning God created the heavens and the earth."
    );
}

/// The export menu asks first while conflicts are open (STORAGE §8.5): two teammates' versions
/// of Titus 1:1 are one conflict of one verse in Titus, and none in another book or in the
/// stories.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preflight_counts_open_conflicts() {
    let (w, at) = super::conflicts::two_versions().await;
    let preflight = |subject| export::Preflight {
        at: at.clone(),
        subject,
    };
    let titus = post(
        &w.t.app,
        &preflight(export::Subject::Book {
            book: BookId::Titus,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        titus,
        export::PreflightView {
            conflicts: 1,
            verses: 1
        }
    );
    let none = export::PreflightView {
        conflicts: 0,
        verses: 0,
    };
    let jonah = post(
        &w.t.app,
        &preflight(export::Subject::Book {
            book: BookId::Jonah,
        }),
    )
    .await
    .unwrap();
    assert_eq!(jonah, none);
    let stories = post(&w.t.app, &preflight(export::Subject::Stories))
        .await
        .unwrap();
    assert_eq!(stories, none);
}

/// The server's downloads, kept in memory: each file registered, by its name, and where the
/// app wrote it.
struct Downloads {
    dir: tempfile::TempDir,
    files: Mutex<Vec<(String, Vec<u8>)>>,
    written: Mutex<Vec<PathBuf>>,
}

impl tc_app::DownloadFiles for Downloads {
    fn dir(&self) -> PathBuf {
        self.dir.path().to_owned()
    }

    fn add(
        &self,
        written: &Path,
        file_name: &str,
        _content_type: &'static str,
    ) -> Result<DownloadId, String> {
        let bytes = std::fs::read(written).map_err(|e| e.to_string())?;
        self.written.lock().unwrap().push(written.to_owned());
        let mut files = self.files.lock().unwrap();
        files.push((file_name.to_owned(), bytes));
        Ok(DownloadId::parse(&format!("{:032x}", files.len())).unwrap())
    }
}

fn give_downloads(app: &tc_app::App) -> Arc<Downloads> {
    let downloads = Arc::new(Downloads {
        dir: tempfile::tempdir().unwrap(),
        files: Mutex::default(),
        written: Mutex::default(),
    });
    app.provide_downloads(downloads.clone());
    downloads
}

/// Runs an export and waits for its end; a minute is a hang.
async fn run(app: &tc_app::App, req: &export::RunExport) -> (JobId, JobState) {
    let job = post(app, req).await.unwrap();
    let mut watch = app.jobs().watch(job).unwrap();
    let state = tokio::time::timeout(Duration::from_secs(60), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("hung: the export ends");
    (job, state)
}

fn exported(state: JobState) -> export::Exported {
    match state {
        JobState::Done(JobOutcome::ExportReady(done)) => done,
        other => panic!("{other:?}"),
    }
}

/// tC4's English catalog messages for a print's lines.
fn english_words() -> export::PrintWords {
    export::PrintWords {
        chapter_heading: "Chapter {n}".into(),
        chapter_not_drafted: "[ chapter {n} not yet drafted ]".into(),
        chapters_not_drafted: "[ chapters {from}–{to} not yet drafted ]".into(),
        not_yet_drafted: "[ verse not yet drafted ]".into(),
        frame_not_drafted: "[ frame {n} not yet drafted ]".into(),
        frames_not_drafted: "[ frames {from}–{to} not yet drafted ]".into(),
        story_not_drafted: "[ story {n} not yet drafted ]".into(),
        stories_not_drafted: "[ stories {from}–{to} not yet drafted ]".into(),
        story_number: "Story {n}".into(),
        image_alt: "Story frame {n}".into(),
        nothing_drafted_obs: "Nothing is drafted in this project yet.".into(),
    }
}

/// tC4 zips every file of `content/` (`obsMarkdown.ts`): the story files and the front and
/// back matter, byte for byte. Leaving the matter out would drop the translator's text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_export_zips_the_stories_and_the_front_and_back_matter() {
    let (_dir, t) = start("sample").await;
    let downloads = give_downloads(&t.app);
    let at = super::story::open_obs(&t.app).await.session.at;
    let (_, state) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(70),
            at,
            producer: export::Producer::StoryMarkdown,
            subject: export::Subject::Stories,
            print: None,
        },
    )
    .await;
    let export::ExportedFile::Download { file_name, .. } = exported(state).file else {
        panic!("a download")
    };
    assert!(
        file_name.starts_with("Equipo Ejemplo — Historias Bíblicas Abiertas-stories-")
            && file_name.ends_with(".zip"),
        "{file_name}"
    );
    let files = downloads.files.lock().unwrap();
    let [(name, zip)] = files.as_slice() else {
        panic!("one file")
    };
    assert_eq!(name, &file_name);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip.clone())).unwrap();
    let stored = super::scenario::scenario("sample").files();
    let project = "sample_burrito_obs/ingredients/";
    let mut want: Vec<(String, &Vec<u8>)> = stored
        .iter()
        .filter_map(|(path, bytes)| {
            let rest = &path[path.find(project)? + project.len()..];
            (rest.starts_with("content/") && !rest.ends_with(".bak"))
                .then(|| (rest.to_owned(), bytes))
        })
        .collect();
    want.sort();
    for matter in [
        "content/front/title.md",
        "content/front/intro.md",
        "content/back/intro.md",
    ] {
        assert!(
            want.iter().any(|(p, _)| p == matter),
            "the fixture has {matter}"
        );
    }
    let mut got: Vec<(String, Vec<u8>)> = (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().to_owned(), bytes)
        })
        .collect();
    got.sort();
    let got: Vec<(String, &Vec<u8>)> = got.iter().map(|(p, b)| (p.clone(), b)).collect();
    assert_eq!(got, want);
}

/// A print export keeps its document for `GET /print/{job}`, which serves it only once the
/// job finished as a print: never for an unknown job, one still running, another export, or
/// an id written another way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_print_page_is_a_finished_print_exports_and_no_other_jobs() {
    let (_dir, t) = start("sample").await;
    give_downloads(&t.app);
    let at = opened(&t.app).await.session.at;
    let titus = export::Subject::Book {
        book: BookId::Titus,
    };
    let print = export::PrintRequest {
        setup: tc_api::model::print::PageSetup {
            chapter_opening: tc_api::model::print::ChapterOpening::Heading,
            ..Default::default()
        },
        title: "Tito".into(),
        words: english_words(),
    };
    let missing = post(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(71),
            at: at.clone(),
            producer: export::Producer::PrintHtml,
            subject: titus,
            print: None,
        },
    )
    .await;
    assert!(
        matches!(&missing, Err(ApiError::BadRequest { field, .. }) if field == "print"),
        "{missing:?}"
    );

    let (job, state) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(72),
            at: at.clone(),
            producer: export::Producer::PrintHtml,
            subject: titus,
            print: Some(print),
        },
    )
    .await;
    let export::ExportedFile::Print { file_name } = exported(state).file else {
        panic!("a print")
    };
    assert!(
        file_name.starts_with("TIT-") && file_name.ends_with(".pdf"),
        "{file_name}"
    );
    let page = t.app.print_page(&job.get().to_string()).await.unwrap();
    assert_eq!(page.content_type, "text/html; charset=utf-8");
    let html = String::from_utf8(page.bytes).unwrap();
    let title = file_name.trim_end_matches(".pdf");
    assert!(html.contains(&format!("<title>{title}</title>")), "{html}");
    assert!(
        html.contains(">Tito</h1>"),
        "the heading is the book's name"
    );
    assert!(
        html.contains("Chapter 1"),
        "the request's words fill the heading"
    );
    assert!(
        html.contains(".print-book"),
        "the print stylesheet is in the page"
    );

    assert!(t.app.print_page(&format!("0{}", job.get())).await.is_none());
    assert!(t.app.print_page("99999").await.is_none());
    assert!(t.app.print_page("x").await.is_none());

    let (usfm, _) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(73),
            at,
            producer: export::Producer::PlainUsfm,
            subject: titus,
            print: None,
        },
    )
    .await;
    assert!(t.app.print_page(&usfm.get().to_string()).await.is_none());

    let running =
        ManualJob::start(t.app.jobs(), JobSpec::new(JobKind::Export, Phase::Export)).await;
    assert!(
        t.app
            .print_page(&running.id.get().to_string())
            .await
            .is_none(),
        "a job still running"
    );
    running.finish(Err(tc_app::jobs::JobEnd::Cancelled));
}

/// The sample scenario's Bible and story projects written by tc-store on this computer's files,
/// as `tc4 serve --scenario` writes them, and an app over that store with Titus 1:2 saved.
struct OnDiskExports {
    t: super::common::TestApp,
    // Kept: the folders live as long as the case.
    _dir: tempfile::TempDir,
    projects: PathBuf,
    downloads: Arc<Downloads>,
    /// The Bible project, open on the screen with TIT 1:2 just saved.
    bible: Scope,
}

const BIBLE: &str = "sample_burrito";
const STORIES: &str = "sample_burrito_obs";

/// Opens `name` on the screen, which leaves the project open before.
async fn open_on_screen(app: &tc_app::App, name: &str) -> Scope {
    app.sessions()
        .open_screen(&super::common::folder(name), super::common::open_progress())
        .await
        .unwrap()
        .at
}

async fn on_disk_exports() -> OnDiskExports {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let backend = tc_app::backend::store::StoreBackend::start(
        tc_store::StoreConfig {
            app_data: dir.path().join("app-data"),
            projects: projects.clone(),
            vfs: Arc::new(tc_store::StdFs),
            platform: Arc::new(tc_store::StdPlatform),
            clock: Arc::new(tc_store::SystemClock),
            threads: 2,
            watch_debounce: Duration::from_millis(20),
        },
        Arc::default(),
    )
    .unwrap();
    backend.store().wait_cleanup();
    let (store, _) = tc_app::backend::scenario_store::ScenarioStore::write(
        backend,
        super::scenario::scenario("sample"),
    )
    .unwrap();
    let t = super::store_scenario::app_over(dir.path(), Arc::new(store)).await;
    let downloads = give_downloads(&t.app);
    let at = open_on_screen(&t.app, BIBLE).await;
    let session = t.app.sessions().reader(&at).unwrap();
    let minted = session
        .read()
        .mint(&super::common::tit(1, 2), None)
        .unwrap();
    let ack = post(
        &t.app,
        &tc_api::editor::SaveVerse {
            at: at.clone(),
            request: rid(80),
            shown: tc_app::wire::token::shown(&minted).unwrap(),
            words: "saved before the export".to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, tc_api::write::Stored::Written);
    OnDiskExports {
        t,
        _dir: dir,
        projects,
        downloads,
        bible: at,
    }
}

/// An export to run: what makes it, of what, and whether it is a print.
type ExportOf = (export::Producer, export::Subject, bool);

/// Every export the menu offers, of Titus and of the stories, by project.
fn every_export() -> [(&'static str, Vec<ExportOf>); 2] {
    let titus = export::Subject::Book {
        book: BookId::Titus,
    };
    [
        (
            BIBLE,
            vec![
                (export::Producer::AlignedUsfm, titus, false),
                (export::Producer::PlainUsfm, titus, false),
                (export::Producer::PrintHtml, titus, true),
            ],
        ),
        (
            STORIES,
            vec![
                (
                    export::Producer::StoryMarkdown,
                    export::Subject::Stories,
                    false,
                ),
                (export::Producer::PrintHtml, export::Subject::Stories, true),
            ],
        ),
    ]
}

/// Every file of the project folder but git's own.
fn project_files(folder: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut files = super::store::files_on_disk(folder);
    files.retain(|rel, _| !rel.starts_with(".git/"));
    files
}

/// Runs each of `exports` of the project open at `at` once, each to its end, with requests from
/// `base` on, and returns what each made.
async fn run_exports(
    w: &OnDiskExports,
    at: &Scope,
    exports: &[ExportOf],
    base: u8,
) -> Vec<(JobId, export::Exported)> {
    let mut made = Vec::new();
    for (&(producer, subject, print), request) in exports.iter().zip(base..) {
        let print = print.then(|| export::PrintRequest {
            setup: tc_api::model::print::PageSetup {
                pictures: tc_api::model::print::Pictures::Hidden,
                ..Default::default()
            },
            title: "Título".into(),
            words: english_words(),
        });
        let req = export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(request),
            at: at.clone(),
            producer,
            subject,
            print,
        };
        let (job, state) = run(&w.t.app, &req).await;
        made.push((job, exported(state)));
    }
    made
}

/// INV-AN-EXPORT-NEVER-WRITES-THE-PROJECT (tC4 J7): after the checkpoint each export runs
/// first, an export changes no byte of the project folder, and its file lands in the downloads
/// folder (a print stays with its job).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_never_writes_the_project() {
    let w = on_disk_exports().await;
    let mut made = Vec::new();
    for ((project, exports), base) in every_export().into_iter().zip([90, 110]) {
        let folder = w.projects.join(project);
        let at = open_on_screen(&w.t.app, project).await;
        run_exports(&w, &at, &exports, base).await;
        let before = project_files(&folder);
        made.extend(run_exports(&w, &at, &exports, base + 10).await);
        let after = project_files(&folder);
        let changed: Vec<&String> = before
            .keys()
            .chain(after.keys())
            .filter(|rel| before.get(*rel) != after.get(*rel))
            .collect();
        assert!(
            changed.is_empty(),
            "an export wrote into {project}: {changed:?}"
        );
    }

    let downloads_dir = w.downloads.dir.path();
    let written = w.downloads.written.lock().unwrap().clone();
    let files = w.downloads.files.lock().unwrap().clone();
    // Both runs' three downloads: two USFM files and the stories zip.
    assert_eq!(written.len(), 6, "{written:?}");
    for (path, (_, bytes)) in written.iter().zip(&files) {
        assert_eq!(path.parent(), Some(downloads_dir), "{}", path.display());
        assert_eq!(&std::fs::read(path).unwrap(), bytes, "{}", path.display());
    }
    let in_projects: Vec<String> = [BIBLE, STORIES]
        .iter()
        .flat_map(|p| project_files(&w.projects.join(p)).into_keys())
        .collect();
    for (job, done) in made {
        match done.file {
            export::ExportedFile::Download { file_name, .. } => {
                assert!(
                    files.iter().any(|(name, _)| *name == file_name),
                    "{file_name}"
                );
                assert!(
                    in_projects.iter().all(|rel| !rel.ends_with(&file_name)),
                    "{file_name} is in a project"
                );
            }
            export::ExportedFile::Print { .. } => {
                assert!(w.t.app.print_page(&job.get().to_string()).await.is_some());
            }
        }
    }
}

/// Each USFM export reads back with tc-usfm, the saved verse in it, and the projects stay valid
/// Scripture Burritos through the exports and the checkpoints before them (PLAN P5).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_export_validates() {
    let w = on_disk_exports().await;
    for ((project, exports), base) in every_export().into_iter().zip([90, 110]) {
        let at = open_on_screen(&w.t.app, project).await;
        run_exports(&w, &at, &exports, base).await;
    }
    let files = w.downloads.files.lock().unwrap().clone();
    let usfm: Vec<&(String, Vec<u8>)> = files
        .iter()
        .filter(|(name, _)| name.ends_with(".usfm"))
        .collect();
    assert_eq!(usfm.len(), 2, "aligned and plain");
    for (name, bytes) in usfm {
        let text = std::str::from_utf8(bytes).unwrap();
        let parts = tc_usfm::codec::decompose(text)
            .unwrap_or_else(|e| panic!("{name} does not parse: {e}"));
        assert_eq!(tc_usfm::codec::recompose(&parts).usfm, text, "{name}");
        let plain = tc_usfm::strip_alignment(text).unwrap();
        assert!(plain.contains("saved before the export"), "{name}");
    }
    for project in [BIBLE, STORIES] {
        let problems = tc_burrito::validate_dir(&w.projects.join(project));
        assert!(problems.is_empty(), "{project}: {problems:#?}");
    }
}

/// A carried `content/` file whose bytes this computer lacks fails the story export: a zip
/// without it would drop the translator's front or back matter.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_export_without_a_carried_files_bytes_fails() {
    let state = super::scenario::scenario("sample");
    let (seeded, report) = tc_app::backend::memory::MemoryBackend::from_scenario(state).unwrap();
    let backend = tc_app::backend::memory::MemoryBackend::new();
    let mut cleared = false;
    for (folder, _) in report.loaded {
        let mut project = seeded.project(&folder).unwrap();
        cleared |= !project.carried.is_empty();
        project.carried.clear();
        backend.add(folder, project).unwrap();
    }
    assert!(cleared, "the stories carry their front and back matter");
    let dir = tempfile::tempdir().unwrap();
    tc_app::backend::scenario::install_resources_into(
        &dir.path().join("app-data"),
        &state.resources().unwrap(),
    )
    .unwrap();
    let t =
        super::common::TestApp::start_with(dir.path(), &super::common::Parts::new(), |_, deps| {
            deps.store = tc_app::StoreChoice::Given(Arc::new(backend));
        })
        .await;
    let downloads = give_downloads(&t.app);
    let at = super::story::open_obs(&t.app).await.session.at;
    let (_, state) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(71),
            at,
            producer: export::Producer::StoryMarkdown,
            subject: export::Subject::Stories,
            print: None,
        },
    )
    .await;
    assert_eq!(
        state,
        JobState::Failed(ApiError::Refused(tc_api::Refusal::Export(
            tc_api::refusal::ExportRefusal::ReadFailed {
                error: "ingredients/content/back/intro.md has no bytes here".into()
            }
        )))
    );
    assert!(downloads.files.lock().unwrap().is_empty(), "no zip");
}

/// The checkpoint before an export (STORAGE §5.13, tC4 `runExport`): the project's last
/// commit holds what the file holds, a verse saved just before the export included.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_commits_the_book_it_exports() {
    let w = on_disk_exports().await;
    let at = w.bible.clone();
    let req = export::RunExport {
        utc_offset: tc_api::export::UtcOffset::UTC,
        request: rid(130),
        at,
        producer: export::Producer::PlainUsfm,
        subject: export::Subject::Book {
            book: BookId::Titus,
        },
        print: None,
    };
    let (_, state) = run(&w.t.app, &req).await;
    exported(state);
    let files = w.downloads.files.lock().unwrap().clone();
    let [(_, usfm)] = files.as_slice() else {
        panic!("one file")
    };
    assert!(String::from_utf8_lossy(usfm).contains("saved before the export"));
    let repo = git2::Repository::open(w.projects.join(BIBLE)).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.summary().ok().flatten(),
        Some("Checkpoint, before export: TIT (tC)"),
        "the export's checkpoint made the last commit"
    );
    let entry = head
        .tree()
        .unwrap()
        .get_path(Path::new("ingredients/TIT.usfm"))
        .unwrap();
    let committed = repo.find_blob(entry.id()).unwrap();
    assert_eq!(
        committed.content(),
        usfm.as_slice(),
        "the commit holds the file"
    );
}

/// A PDF of a book with no drafted verse fails as tC4's does: its diagnosis names the book in
/// the print's title, then the recovery sentence (tC4 `printDocument`, `failureText`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pdf_of_a_book_with_nothing_drafted_names_the_book() {
    let w = on_disk_exports().await;
    let (t, at) = (&w.t, w.bible.clone());
    let session = t.app.sessions().reader(&at).unwrap();
    for verse in 1..=5u8 {
        let minted = session
            .read()
            .mint(&super::common::tit(1, verse.into()), None)
            .unwrap();
        post(
            &t.app,
            &tc_api::editor::SaveVerse {
                at: at.clone(),
                request: rid(140 + verse),
                shown: tc_app::wire::token::shown(&minted).unwrap(),
                words: String::new(),
            },
        )
        .await
        .unwrap();
    }
    let (_, state) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(150),
            at,
            producer: export::Producer::PrintHtml,
            subject: export::Subject::Book {
                book: BookId::Titus,
            },
            print: Some(export::PrintRequest {
                setup: Default::default(),
                title: "Tito".into(),
                words: english_words(),
            }),
        },
    )
    .await;
    assert_eq!(
        state,
        JobState::Failed(ApiError::Refused(tc_api::Refusal::Export(
            tc_api::refusal::ExportRefusal::NothingDrafted {
                error: "Tito has no drafted verse yet, so the PDF has nothing to print.".into()
            }
        )))
    );
}

/// Downloads whose folder takes no file, as on a full disk.
struct FullDownloads(tempfile::TempDir);

impl tc_app::DownloadFiles for FullDownloads {
    fn dir(&self) -> PathBuf {
        self.0.path().to_owned()
    }

    fn add(&self, _: &Path, _: &str, _: &'static str) -> Result<DownloadId, String> {
        Err("No space left on device (os error 28)".to_owned())
    }
}

/// A file the downloads folder would not take fails the export with the words for a file
/// not written, and leaves no `.part` behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_that_cannot_write_its_file_leaves_nothing() {
    let (_dir, t) = start("sample").await;
    let downloads = Arc::new(FullDownloads(tempfile::tempdir().unwrap()));
    t.app.provide_downloads(downloads.clone());
    let at = opened(&t.app).await.session.at;
    let (_, state) = run(
        &t.app,
        &export::RunExport {
            utc_offset: tc_api::export::UtcOffset::UTC,
            request: rid(72),
            at,
            producer: export::Producer::PlainUsfm,
            subject: export::Subject::Book {
                book: BookId::Titus,
            },
            print: None,
        },
    )
    .await;
    assert_eq!(
        state,
        JobState::Failed(ApiError::Refused(tc_api::Refusal::Export(
            tc_api::refusal::ExportRefusal::WriteFailed {
                error: "No space left on device (os error 28)".into()
            }
        )))
    );
    let left: Vec<_> = std::fs::read_dir(downloads.0.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A file is dated by the translator's own day (tC4 dates it in its window), so an evening
/// export in Nairobi is not dated tomorrow: 28 hours apart, the two dates always differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_is_dated_by_the_translators_own_day() {
    let (_dir, t) = start("sample").await;
    give_downloads(&t.app);
    let at = opened(&t.app).await.session.at;
    let mut dates = Vec::new();
    for (request, minutes) in [(80, -14 * 60), (81, 14 * 60)] {
        let (_, state) = run(
            &t.app,
            &export::RunExport {
                utc_offset: tc_api::export::UtcOffset::new(minutes).unwrap(),
                request: rid(request),
                at: at.clone(),
                producer: export::Producer::PlainUsfm,
                subject: export::Subject::Book {
                    book: BookId::Titus,
                },
                print: None,
            },
        )
        .await;
        let export::ExportedFile::Download { file_name, .. } = exported(state).file else {
            panic!("a download");
        };
        let date = file_name
            .strip_prefix("TIT-")
            .and_then(|rest| rest.strip_suffix(".usfm"))
            .unwrap()
            .to_owned();
        dates.push(date);
    }
    assert!(
        dates[0] < dates[1],
        "west {} and east {}",
        dates[0],
        dates[1]
    );
}
