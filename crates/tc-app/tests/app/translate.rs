//! Translate (unit B4a) through `App::request`: the translation's text of the open book, the
//! source panes' versions and the original-language pane, on the parity scenarios.

use tc_api::editor::{Chapters, ChaptersView, PaneText, Sources, VerseRow};
use tc_api::shown::Direction;
use tc_app::backend::scenario::ScenarioState;
use tc_core::BookId;

use super::common::TestApp;
use super::scenario::scenario;
use super::understand::{at, ok, start, start_over};

async fn chapters(t: &TestApp, book: BookId) -> ChaptersView {
    let at = at(t).await;
    ok(&t.app, &Chapters { at, book }).await
}

fn row<'a>(view: &'a ChaptersView, chapter: u16, key: &str) -> &'a VerseRow {
    view.chapters
        .iter()
        .find(|c| c.chapter == chapter)
        .and_then(|c| c.verses.iter().find(|v| v.key.as_str() == key))
        .unwrap_or_else(|| panic!("{chapter}:{key} is not in the view"))
}

/// The sample with its Titus text changed by `edit`, as a teammate's file or an import would
/// bring it.
async fn start_with_titus(edit: impl Fn(&str) -> String) -> (tempfile::TempDir, TestApp) {
    let files = scenario("sample").files().iter().map(|(path, bytes)| {
        if path.ends_with("sample_burrito/ingredients/TIT.usfm") {
            let text = String::from_utf8(bytes.clone()).unwrap();
            (path.clone(), edit(&text).into_bytes())
        } else {
            (path.clone(), bytes.clone())
        }
    });
    start_over(&ScenarioState::from_files(files).unwrap()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_chapter_comes_in_one_read_with_each_verses_words_and_token() {
    let (_dir, t) = start("sample").await;
    let view = chapters(&t, BookId::Titus).await;
    let numbers: Vec<u16> = view.chapters.iter().map(|c| c.chapter).collect();
    assert_eq!(
        numbers,
        [1, 2, 3],
        "a chapter switch needs no other read (AD16)"
    );
    let one = &view.chapters[0];
    let keys: Vec<&str> = one.verses.iter().map(|v| v.key.as_str()).collect();
    assert_eq!(keys, (1..=16).map(|n| n.to_string()).collect::<Vec<_>>());
    let first = row(&view, 1, "1");
    assert!(first.drafted);
    assert_eq!(
        first.text,
        "Pablo, siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos de Dios y el conocimiento de la verdad que va con la piedad,"
    );
    assert_eq!(first.words.value, first.text);
    assert!(first.words.marker.is_none());
    assert!(
        serde_json::to_string(&first.words.token)
            .unwrap()
            .starts_with("\"st1.")
    );
    // tC4's baseline: verses 1 to 5 of chapter 1 are drafted, the rest are the stub.
    let drafted: Vec<&str> = one
        .verses
        .iter()
        .filter(|v| v.drafted)
        .map(|v| v.key.as_str())
        .collect();
    assert_eq!(drafted, ["1", "2", "3", "4", "5"]);
    let stub = row(&view, 1, "6");
    assert_eq!((stub.text.as_str(), stub.words.value.as_str()), ("", "___"));
    // Each verse's token is its own.
    assert_ne!(first.words.token, row(&view, 1, "2").words.token);
    assert_eq!(one.sections, [1], "the sample marks no section of its own");
    assert_eq!(view.conflicts, 0);
}

/// tC4 #786 (Q-20): the box edits the verse's whole words, every poem line with its markers,
/// and a verse that opens on its own poem line has its text. The grid reads each line's level.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verse_editor_shows_every_poem_line() {
    let (_dir, t) = start_with_titus(|usfm| {
        usfm.replacen(
            "\\c 3\n\\p\n\\v 1 ___\n\\v 2 ___\n",
            "\\c 3\n\\p\n\\v 1 Recuérdales\n\\q2 que se sujeten\n\\q1\n\\v 2 \\q2 que no hablen mal de nadie\n",
            1,
        )
    })
    .await;
    let view = chapters(&t, BookId::Titus).await;
    let one = row(&view, 3, "1");
    assert_eq!(one.words.value, "Recuérdales\n\\q2 que se sujeten");
    assert_eq!(one.text, "Recuérdales que se sujeten");
    let two = row(&view, 3, "2");
    assert!(
        two.drafted,
        "verse_starting_on_its_own_poem_line_opens_with_its_text"
    );
    assert_eq!(two.words.value, "\\q2 que no hablen mal de nadie");
    assert_eq!(two.text, "que no hablen mal de nadie");
    assert_eq!((two.lead, two.before), (Some(2), Some(1)));
}

/// tC4 #784: a `\v` right after a paragraph marker is a verse of its own, in its paragraph.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verse_right_after_a_marker_is_listed_in_its_paragraph() {
    let (_dir, t) = start_with_titus(|usfm| {
        usfm.replacen("\\v 6 ___\n", "\\p\\v 6 Si alguno es irreprensible\n", 1)
    })
    .await;
    let view = chapters(&t, BookId::Titus).await;
    let six = row(&view, 1, "6");
    assert_eq!(six.text, "Si alguno es irreprensible");
    assert_eq!(six.before, Some(0), "the paragraph opens at verse 6");
    assert_eq!(row(&view, 1, "5").before, None);
}

/// A span is one row under its range, and a `\ts\*` in the translation starts a section of its
/// own when the source marks none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_span_is_one_row_and_the_translations_own_sections_are_read() {
    let (_dir, t) = start_with_titus(|usfm| {
        usfm.replacen(
            "\\v 7 ___\n\\v 8 ___\n\\v 9 ___\n",
            "\\v 7-8 Porque es necesario\n\\ts\\*\n\\v 9 retenedor\n",
            1,
        )
    })
    .await;
    let view = chapters(&t, BookId::Titus).await;
    let keys: Vec<&str> = view.chapters[0]
        .verses
        .iter()
        .map(|v| v.key.as_str())
        .collect();
    assert_eq!(&keys[5..8], ["6", "7-8", "9"]);
    assert_eq!(view.chapters[0].sections, [1, 9]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_panes_name_their_versions_and_the_original_reads_in_its_direction() {
    let (_dir, t) = start("en-pins").await;
    let at = at(&t).await;
    let titus = ok(
        &t.app,
        &Sources {
            at: at.clone(),
            book: BookId::Titus,
            chapter: 1,
        },
    )
    .await;
    let versions: Vec<(&str, Option<&str>)> = titus
        .panes
        .iter()
        .map(|p| (p.id.as_str(), p.version.as_deref()))
        .collect();
    // tC4's caption: "unfoldingWord Literal Text · v91 · pinned".
    assert_eq!(versions, [("ult", Some("v91")), ("ust", Some("v91"))]);
    let original = titus.original.expect("the project pins the UGNT");
    assert_eq!(
        (original.id.as_str(), original.dir),
        ("orig", Direction::Ltr)
    );
    let PaneText::Ready(greek) = &original.text else {
        panic!("{:?}", original.text)
    };
    assert_eq!(greek.verses.len(), 16);
    assert!(
        greek.verses[0].text.starts_with("Παῦλος"),
        "{}",
        greek.verses[0].text
    );
    let jonah = ok(
        &t.app,
        &Sources {
            at,
            book: BookId::Jonah,
            chapter: 1,
        },
    )
    .await;
    assert_eq!(jonah.original.map(|o| o.dir), Some(Direction::Rtl));
}
