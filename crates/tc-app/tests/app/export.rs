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

/// The server's downloads, kept in memory: each file registered, by its name.
struct Downloads {
    dir: tempfile::TempDir,
    files: Mutex<Vec<(String, Vec<u8>)>>,
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
        let mut files = self.files.lock().unwrap();
        files.push((file_name.to_owned(), bytes));
        Ok(DownloadId::parse(&format!("{:032x}", files.len())).unwrap())
    }
}

fn give_downloads(app: &tc_app::App) -> Arc<Downloads> {
    let downloads = Arc::new(Downloads {
        dir: tempfile::tempdir().unwrap(),
        files: Mutex::default(),
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
        file_name.starts_with("sample_burrito_obs-stories-") && file_name.ends_with(".zip"),
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
        file_name.starts_with("tit-") && file_name.ends_with(".pdf"),
        "{file_name}"
    );
    let page = t.app.print_page(&job.get().to_string()).await.unwrap();
    assert_eq!(page.content_type, "text/html; charset=utf-8");
    let html = String::from_utf8(page.bytes).unwrap();
    assert!(html.contains("<title>Tito</title>"), "{html}");
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
