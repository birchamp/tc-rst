//! What the Community Checking preview prints (unit B8) through `App::request`: the sample's
//! Titus and Jonah and its story project, each as tC4's own print code states it in
//! `fixtures/export/print-vectors.json`.

use std::sync::OnceLock;

use serde_json::Value;
use tc_api::export::{self, PrintFrame, PrintItem, PrintStoryItem};
use tc_api::ids::Scope;
use tc_core::BookId;

use super::common::repo_root;
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
