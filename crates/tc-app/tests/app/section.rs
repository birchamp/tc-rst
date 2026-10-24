//! The section save (unit B4b, STORAGE §5.2) through the app: a card's text over the verses it
//! displayed becomes the values the store writes. The values are built by a pure function
//! (`tc_app::section::section_values`) tested on its own, and the whole save runs on the store
//! through the production `editor/saveSection` endpoint.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use proptest::prelude::*;
use tc_api::editor::{Chapters, ChaptersView, VerseRow};
use tc_api::editor::{SaveSection, SaveVerse};
use tc_api::ids::Scope;
use tc_api::model::js_trim;
use tc_api::model::section::{ParaFormat, SectionBuffer, SectionVerse, expand_keys, initial_text};
use tc_api::write::{Ack, Stored};
use tc_api::{ApiError, NotSaved, ShownToken};
use tc_app::backend::scenario::ScenarioState;
use tc_app::section::{gap_marker, section_values, splice_gap};
use tc_core::BookId;
use tc_journal::VerseNo;
use tc_journal::record::{MarkerWs, VerseValue};
use tc_store::fault::FaultFs;
use unicode_normalization::UnicodeNormalization;

use super::common::{TestApp, folder, open_progress, post, rid};
use super::scenario::scenario;
use super::store_scenario::{SAMPLE, app_over, scenario_state_on};
use crate::store_world::{PROJECTS, assert_valid_burrito};

// ---------------------------------------------------------------------------------------------
// The values, with no store.
// ---------------------------------------------------------------------------------------------

fn no(n: u16) -> VerseNo {
    VerseNo::new(n).unwrap()
}

fn text(words: &str, layout: &str, last: u16) -> VerseValue {
    VerseValue::Text {
        marker: MarkerWs::canonical(),
        words: words.to_owned(),
        layout: layout.to_owned(),
        last: no(last),
        from: None,
    }
}

fn covered(by: u16) -> VerseValue {
    VerseValue::Covered { by: no(by) }
}

fn words_of(value: &Option<VerseValue>) -> Option<&str> {
    match value {
        Some(VerseValue::Text { words, .. }) => Some(words),
        _ => None,
    }
}

fn layout_of(value: &Option<VerseValue>) -> Option<&str> {
    match value {
        Some(VerseValue::Text { layout, .. }) => Some(layout),
        _ => None,
    }
}

/// Two verses as the sample has them, drafted.
fn drafted() -> Vec<(u16, VerseValue)> {
    vec![
        (9, text("Exhorta a los siervos", "\n", 9)),
        (10, text("no defraudando", "\n", 10)),
    ]
}

#[test]
fn a_text_that_changes_nothing_changes_no_verse() {
    let values = section_values("9 Exhorta a los siervos\n10 no defraudando", &drafted()).unwrap();
    assert_eq!(values, [None, None]);
}

#[test]
fn a_changed_body_replaces_the_words_and_keeps_the_marker_and_the_layout() {
    let shown = vec![
        (
            9,
            VerseValue::Text {
                marker: MarkerWs::new("\t", tc_journal::record::MarkerTerm::Space).unwrap(),
                words: "Exhorta".to_owned(),
                layout: "\n\\p\n".to_owned(),
                last: no(9),
                from: None,
            },
        ),
        (10, text("no", "\n", 10)),
    ];
    let values = section_values("9 Exhorta a los\n\n10 no", &shown).unwrap();
    let Some(VerseValue::Text {
        marker,
        words,
        layout,
        ..
    }) = &values[0]
    else {
        panic!("{values:?}")
    };
    assert_eq!(marker.lead(), "\t", "the verse keeps its own marker");
    assert_eq!(words, "Exhorta a los");
    assert_eq!(
        layout, "\n\\p\n",
        "the paragraph the text asks for was there"
    );
    assert_eq!(values[1], None);
}

#[test]
fn an_empty_body_is_the_stub_and_a_verse_with_no_text_is_not_rewritten_as_one() {
    let shown = vec![(9, text("___", "\n", 9)), (10, text("no", "\n", 10))];
    let values = section_values("10 no", &shown).unwrap();
    assert_eq!(
        values,
        [None, None],
        "the stub stays the stub, byte for byte"
    );
    let values = section_values("9 a\n10 no", &shown).unwrap();
    assert_eq!(words_of(&values[0]), Some("a"));
}

#[test]
fn stacking_one_verse_on_another_writes_a_span_and_the_covered_verse() {
    let values = section_values("9-10 Exhorta a los siervos no defraudando", &drafted()).unwrap();
    assert_eq!(
        words_of(&values[0]),
        Some("Exhorta a los siervos no defraudando")
    );
    assert!(matches!(&values[0], Some(VerseValue::Text { last, .. }) if last.get() == 10));
    assert_eq!(values[1], Some(covered(9)));
}

/// #575 (D94): the blank line a paragraph left between two joined verses becomes a space.
#[test]
fn joined_span_collapses_blank_lines_to_one_space() {
    let values = section_values("9-10 Exhorta\n\nno defraudando", &drafted()).unwrap();
    assert_eq!(words_of(&values[0]), Some("Exhorta no defraudando"));
}

#[test]
fn breaking_a_span_writes_two_verses_and_the_layout_stays_with_the_last() {
    let shown = vec![
        (9, text("a b c d", "\n\\ts\\*\n", 10)),
        (10, covered(9)),
        (11, text("e", "\n", 11)),
    ];
    let values = section_values("9 a b\n10 c d\n11 e", &shown).unwrap();
    assert_eq!(words_of(&values[0]), Some("a b"));
    assert_eq!(
        layout_of(&values[0]),
        Some("\n"),
        "inside the span there was none"
    );
    assert_eq!(words_of(&values[1]), Some("c d"));
    assert_eq!(
        layout_of(&values[1]),
        Some("\n\\ts\\*\n"),
        "the span's layout followed its last verse"
    );
    assert_eq!(values[2], None);
}

#[test]
fn a_section_emptied_keeps_its_spans_and_its_paragraphs_and_stubs_the_words() {
    let shown = vec![
        (9, text("a b", "\n\\p\n", 10)),
        (10, covered(9)),
        (11, text("e", "\n", 11)),
    ];
    let values = section_values("", &shown).unwrap();
    assert_eq!(words_of(&values[0]), Some("___"));
    assert_eq!(
        layout_of(&values[0]),
        Some("\n\\p\n"),
        "#600 changes the bodies only"
    );
    assert!(matches!(&values[0], Some(VerseValue::Text { last, .. }) if last.get() == 10));
    assert_eq!(values[1], None, "the span stays a span");
    assert_eq!(words_of(&values[2]), Some("___"));
}

#[test]
fn a_typed_paragraph_or_poetry_line_sets_the_layout_of_the_verse_before() {
    let shown = vec![
        (9, text("a", "\n", 9)),
        (10, text("b", "\n", 10)),
        (11, text("c", "\n", 11)),
    ];
    let values = section_values("9 a\n\n10 b\n\t11 c", &shown).unwrap();
    assert_eq!(layout_of(&values[0]), Some("\n\\p\n"));
    assert_eq!(layout_of(&values[1]), Some("\n\\q1\n"));
    assert_eq!(
        layout_of(&values[2]),
        None,
        "the last verse's layout is the next section's"
    );
    // And a text with no paragraph takes the marker away again.
    let shown = vec![(9, text("a", "\n\\p\n", 9)), (10, text("b", "\n", 10))];
    let values = section_values("9 a\n10 b", &shown).unwrap();
    assert_eq!(layout_of(&values[0]), Some("\n"));
}

/// A seeded book has the source's paragraph and poetry lines before its undrafted verses. A
/// section save that leaves such a verse undrafted keeps the line before it, and gives it to no
/// other verse; tC4 moved it onto the next verse with words (a declared difference, B4b).
#[test]
fn a_verse_left_undrafted_keeps_the_line_before_it() {
    let shown = vec![
        (3, text("c", "\n\\q1\n", 3)),
        (4, text("___", "\n", 4)),
        (5, text("e", "\n", 5)),
    ];
    let (_, opened) = reopened(&shown);
    assert_eq!(opened, "3 c\n\t4 \n5 e");
    let values = section_values("3 c2\n\t4 \n5 e", &shown).unwrap();
    assert_eq!(words_of(&values[0]), Some("c2"));
    assert_eq!(layout_of(&values[0]), Some("\n\\q1\n"));
    assert_eq!((&values[1], &values[2]), (&None, &None));
    // Every verse a stub: the card opens empty, and typing verse 1 keeps every line.
    let shown: Vec<(u16, VerseValue)> = (1..=4).map(|n| (n, text("___", "\n\\q1\n", n))).collect();
    let values = section_values("1 a", &shown).unwrap();
    assert_eq!(words_of(&values[0]), Some("a"));
    assert_eq!(layout_of(&values[0]), Some("\n\\q1\n"));
    assert_eq!(&values[1..], [None, None, None]);
}

/// The verses as the card opens them after `after` is stored: a span's key, the format from the
/// layout of the verse before, the stub as no text (tc-app's `editor/chapters` and tc-ui's
/// `card_verses`).
fn reopened(after: &[(u16, VerseValue)]) -> (Vec<String>, String) {
    let mut verses = Vec::new();
    let mut format = None;
    for (verse, value) in after {
        let VerseValue::Text {
            words,
            layout,
            last,
            ..
        } = value
        else {
            continue;
        };
        let body = js_trim(words);
        let drafted = !body.is_empty() && body != "___";
        verses.push(SectionVerse {
            key: if last.get() > *verse {
                format!("{verse}-{last}")
            } else {
                verse.to_string()
            },
            drafted,
            // As the app shows the words: poem lines as tabs (tC4 #810, REF1b).
            body: if drafted {
                tc_usfm::poem::editor_text(body)
            } else {
                String::new()
            },
            format,
        });
        format = gap_marker(layout);
    }
    let keys: Vec<String> = verses.iter().map(|v| v.key.clone()).collect();
    let text = initial_text(&verses, &expand_keys(&keys));
    (keys, text)
}

fn shown_strategy() -> impl Strategy<Value = Vec<(u16, VerseValue)>> {
    let words = prop_oneof![
        Just("___".to_owned()),
        Just(String::new()),
        "[a-z]{1,3}( [a-z]{1,3}){0,3}",
        "[a-z]{1,3}\n\\\\q2 [a-z]{1,3}",
    ];
    let layout = prop_oneof![
        Just("\n".to_owned()),
        Just("\n\\p\n".to_owned()),
        Just("\n\\q1\n".to_owned()),
        Just("\n\\q2\n".to_owned()),
        Just("\n\\ts\\*\n\\p\n".to_owned()),
    ];
    let spans = prop_oneof![
        Just(vec![1u16, 1, 1]),
        Just(vec![2, 1, 1]),
        Just(vec![1]),
        Just(vec![1, 2, 1, 1]),
    ];
    spans
        .prop_flat_map(move |spans| {
            let parts = proptest::collection::vec((words.clone(), layout.clone()), spans.len());
            (Just(spans), parts)
        })
        .prop_map(|(spans, parts)| {
            let mut shown = Vec::new();
            let mut head = 1u16;
            for (len, (words, layout)) in spans.into_iter().zip(parts) {
                shown.push((head, text(&words, &layout, head + len - 1)));
                for n in head + 1..head + len {
                    shown.push((n, covered(head)));
                }
                head += len;
            }
            shown
        })
}

fn typed_strategy() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        "(\t|\t\t)?[1-5]( [a-z]{1,3}){0,4}",
        "(\t|\t\t)?[1-4]-[2-5]( [a-z]{1,3}){0,3}",
        "[a-z]{1,3}( [a-z]{1,3}){0,4}",
        Just(String::new()),
    ];
    proptest::collection::vec(line, 0..7).prop_map(|lines| lines.join("\n"))
}

/// The `\\p`, `\\q1` or `\\q2` line after verse `n` as displayed: the line before the next verse.
fn shown_line_after(shown: &[(u16, VerseValue)], n: u16) -> Option<ParaFormat> {
    shown.iter().find_map(|(verse, value)| match value {
        VerseValue::Text { layout, last, .. } if last.get().max(*verse) == n => gap_marker(layout),
        _ => None,
    })
}

proptest! {
    /// The differential test of AD24: the values tc-app writes for a card's text, opened again
    /// in a card, read as the same verses with the same bodies and the same line before each
    /// verse as the card held when Save was pressed: what a translator saw is stored. A verse
    /// with no words that the text gives no line keeps the line it had.
    #[test]
    fn a_saved_section_opens_again_as_the_card_showed_it(
        shown in shown_strategy(),
        typed in typed_strategy(),
    ) {
        let (keys, _) = reopened(&shown);
        let card = SectionBuffer::parse(&typed, &keys);
        let values = section_values(&typed, &shown).unwrap();
        let after: Vec<(u16, VerseValue)> = shown
            .iter()
            .zip(values)
            .map(|((verse, old), new)| (*verse, new.unwrap_or_else(|| old.clone())))
            .collect();
        let (new_keys, text) = reopened(&after);
        prop_assert_eq!(&new_keys, &card.new_keys(), "{:?}", after);
        let again = SectionBuffer::parse(&text, &new_keys);
        prop_assert_eq!(again.verses(), card.verses(), "{:?}", text);
        if card.is_empty() {
            // #600: the lines between the verses stay; the reopened card is empty.
            return Ok(());
        }
        let typed_lines: BTreeMap<String, Option<ParaFormat>> = card.formats().into_iter().collect();
        let lines: BTreeMap<String, Option<ParaFormat>> = again.formats().into_iter().collect();
        for pair in card.groups().windows(2) {
            let before = &pair[0];
            let last: u16 = before.members.last().unwrap().parse().unwrap();
            let want = typed_lines
                .get(&before.key)
                .copied()
                .unwrap_or_else(|| shown_line_after(&shown, last));
            prop_assert_eq!(lines.get(&before.key), Some(&want), "{:?} {:?}", text, after);
        }
    }
}

#[test]
fn a_view_that_does_not_fit_together_is_refused() {
    let cases: [(&str, Vec<(u16, VerseValue)>); 4] = [
        ("covered with no head", vec![(10, covered(9))]),
        ("a span shown in part", vec![(9, text("a", "\n", 10))]),
        (
            "a verse the translation does not have",
            vec![(9, VerseValue::Absent { moved_to: None })],
        ),
        (
            "a verse twice",
            vec![(9, text("a", "\n", 9)), (9, text("a", "\n", 9))],
        ),
    ];
    for (name, shown) in cases {
        assert!(section_values("9 a", &shown).is_err(), "{name}");
    }
}

#[test]
fn the_gap_marker_is_the_first_of_the_three_lines_and_a_splice_changes_only_that_line() {
    assert_eq!(gap_marker("\n\\p\n"), Some(ParaFormat::P));
    assert_eq!(gap_marker("\n\\ts\\*\n\\q2\r\n"), Some(ParaFormat::Q2));
    assert_eq!(gap_marker("\n\\q3\n\\m\n"), None);
    assert_eq!(splice_gap("\n\\p\n", Some(ParaFormat::Q1)), "\n\\q1\n");
    assert_eq!(splice_gap("\n\\p\n", None), "\n");
    assert_eq!(splice_gap("\n", Some(ParaFormat::P)), "\n\\p\n");
    assert_eq!(splice_gap("", Some(ParaFormat::P)), "\n\\p\n");
    assert_eq!(
        splice_gap("\n\\ts\\*\n", Some(ParaFormat::Q2)),
        "\n\\ts\\*\n\\q2\n"
    );
    assert_eq!(
        splice_gap("\r\n\\p\r\n", Some(ParaFormat::Q1)),
        "\r\n\\q1\r\n"
    );
    assert_eq!(
        splice_gap("\n\\p", Some(ParaFormat::Q1)),
        "\n\\q1",
        "no line end to invent"
    );
    for layout in ["\n\\p\n", "\n\\q1", "\r\n\\q2\r\n", "\n\\ts\\*\n", ""] {
        assert_eq!(splice_gap(layout, gap_marker(layout)), layout);
    }
    assert_eq!(
        splice_gap("\n\\q3\n", None),
        "\n\\q3\n",
        "a marker of another kind is not touched"
    );
}

// ---------------------------------------------------------------------------------------------
// The whole save, on the store.
// ---------------------------------------------------------------------------------------------

struct Rig {
    t: TestApp,
    at: Scope,
    fs: FaultFs,
    _dir: tempfile::TempDir,
}

impl Rig {
    async fn over(state: &ScenarioState) -> Rig {
        let fs = FaultFs::new();
        let (store, _) = scenario_state_on(&fs, state);
        let dir = tempfile::tempdir().unwrap();
        let t = app_over(dir.path(), Arc::new(store)).await;
        let open = t
            .app
            .sessions()
            .open_screen(&folder(SAMPLE), open_progress())
            .await
            .unwrap();
        Rig {
            t,
            at: open.at,
            fs,
            _dir: dir,
        }
    }

    async fn sample() -> Rig {
        Self::over(scenario("sample")).await
    }

    async fn chapters(&self) -> ChaptersView {
        post(
            &self.t.app,
            &Chapters {
                at: self.at.clone(),
                book: BookId::Titus,
            },
        )
        .await
        .unwrap()
    }

    async fn save(
        &self,
        n: u8,
        view: &ChaptersView,
        chapter: u16,
        keys: &[&str],
        text: &str,
    ) -> Result<Ack, ApiError> {
        post(
            &self.t.app,
            &SaveSection {
                at: self.at.clone(),
                request: rid(n),
                book: BookId::Titus,
                displayed: tokens(view, chapter, keys),
                text: text.to_owned(),
            },
        )
        .await
    }

    /// The card's opening text for `keys`, saved with `edit` applied to its text.
    async fn finish(&self) {
        self.t.app.sessions().leave_screen(&self.at).unwrap();
        self.t.app.quit().await;
        assert_valid_burrito(&self.fs, &Path::new(PROJECTS).join(SAMPLE));
    }
}

fn row<'a>(view: &'a ChaptersView, chapter: u16, key: &str) -> &'a VerseRow {
    view.chapters
        .iter()
        .find(|c| c.chapter == chapter)
        .and_then(|c| c.verses.iter().find(|v| v.key.as_str() == key))
        .unwrap_or_else(|| panic!("{chapter}:{key} is not in the view"))
}

fn has(view: &ChaptersView, chapter: u16, key: &str) -> bool {
    view.chapters
        .iter()
        .find(|c| c.chapter == chapter)
        .is_some_and(|c| c.verses.iter().any(|v| v.key.as_str() == key))
}

/// The tokens a card displays for the section `keys`: each verse's, then the verses it covers.
fn tokens(view: &ChaptersView, chapter: u16, keys: &[&str]) -> Vec<ShownToken> {
    keys.iter()
        .flat_map(|key| {
            let r = row(view, chapter, key);
            std::iter::once(r.words.token.clone()).chain(r.covered.iter().cloned())
        })
        .collect()
}

/// What the card opens with: one `key body` line per verse, the formats as tabs and blank lines.
fn opening(view: &ChaptersView, chapter: u16, keys: &[&str]) -> String {
    let verses: Vec<SectionVerse> = keys
        .iter()
        .map(|key| {
            let r = row(view, chapter, key);
            SectionVerse {
                key: (*key).to_owned(),
                drafted: r.drafted,
                body: if r.drafted {
                    js_trim(&r.words.value).to_owned()
                } else {
                    String::new()
                },
                format: r.format,
            }
        })
        .collect();
    let keys: Vec<String> = keys.iter().map(|k| (*k).to_owned()).collect();
    initial_text(&verses, &expand_keys(&keys))
}

const NINE: &str = "Exhorta a los siervos a que se sujeten a sus amos";
const TEN: &str = "no defraudando sino mostrando toda buena fe";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drafting_a_section_writes_each_verse_and_nothing_else() {
    let rig = Rig::sample().await;
    let before = rig.chapters().await;
    assert_eq!(row(&before, 2, "9").words.value, "___");
    let ack = rig
        .save(1, &before, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    let after = rig.chapters().await;
    assert_eq!(row(&after, 2, "9").words.value, NINE);
    assert_eq!(row(&after, 2, "10").words.value, TEN);
    // The Ack hands back the token the next save of each verse uses.
    let next: Vec<&ShownToken> = ack.tokens.iter().map(|t| &t.token).collect();
    assert!(next.contains(&&row(&after, 2, "9").words.token));
    assert!(next.contains(&&row(&after, 2, "10").words.token));
    // Every other verse of the book is as it was, token and all.
    for chapter in &before.chapters {
        for old in &chapter.verses {
            if chapter.chapter == 2 && matches!(old.key.as_str(), "9" | "10") {
                continue;
            }
            let now = row(&after, chapter.chapter, old.key.as_str());
            assert_eq!(now.words, old.words, "{}:{}", chapter.chapter, old.key);
        }
    }
    // The same card saved again over the tokens it got changes nothing.
    let again = rig
        .save(2, &after, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    assert_eq!(again.stored, Stored::Unchanged);
    rig.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_section_save_that_changes_nothing_writes_nothing() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    let keys = ["1", "2", "3"];
    let sent = opening(&view, 1, &keys);
    assert!(sent.starts_with("1 Pablo, siervo de Dios"), "{sent}");
    let ack = rig.save(1, &view, 1, &keys, &sent).await.unwrap();
    assert_eq!(ack.stored, Stored::Unchanged);
    assert_eq!(ack.saved[0].text, sent);
    let tokens_back: Vec<&ShownToken> = ack.tokens.iter().map(|t| &t.token).collect();
    for token in tokens(&view, 1, &keys) {
        assert!(
            tokens_back.contains(&&token),
            "every displayed verse's token comes back"
        );
    }
    assert_eq!(rig.chapters().await, view, "the read is the same");
    rig.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_span_is_made_and_broken_by_the_text() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    rig.save(1, &view, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    // Stack 10 on 9.
    let view = rig.chapters().await;
    let joined = format!("9-10 {NINE} {TEN}");
    rig.save(2, &view, 2, &["9", "10"], &joined).await.unwrap();
    let view = rig.chapters().await;
    assert!(!has(&view, 2, "9") && !has(&view, 2, "10"));
    let span = row(&view, 2, "9-10");
    assert_eq!(span.words.value, format!("{NINE} {TEN}"));
    assert_eq!(
        span.covered.len(),
        1,
        "the covered verse is displayed with it"
    );
    // Break it where 10 begins.
    rig.save(3, &view, 2, &["9-10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "9").words.value, NINE);
    assert_eq!(row(&view, 2, "10").words.value, TEN);
    assert!(!has(&view, 2, "9-10"));
    rig.finish().await;
}

/// #600: Keep editing writes nothing and the save of an emptied card writes a stub for each
/// verse, one change per verse; a span stays a span and a paragraph between verses stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn emptied_section_save_asks_then_writes_stubs() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    rig.save(1, &view, 2, &["9", "10"], &format!("9 {NINE}\n\n10 {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "10").format, Some(ParaFormat::P));
    rig.save(2, &view, 2, &["9", "10"], "").await.unwrap();
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "9").words.value, "___");
    assert_eq!(row(&view, 2, "10").words.value, "___");
    assert_eq!(
        row(&view, 2, "10").format,
        Some(ParaFormat::P),
        "the paragraph break stays"
    );
    // Drafted again with no blank line, the break is taken away (J2).
    rig.save(3, &view, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "10").format, None);
    // A span, emptied, is the stub span it was.
    rig.save(4, &view, 2, &["9", "10"], &format!("9-10 {NINE} {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    rig.save(5, &view, 2, &["9-10"], "").await.unwrap();
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "9-10").words.value, "___");
    assert_eq!(row(&view, 2, "9-10").covered.len(), 1);
    rig.finish().await;
}

/// A teammate saves a verse of the section after the card was drawn: the save goes through over
/// the tokens the card showed, and the two versions stand side by side, never one replacing the
/// other unseen (I4).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_that_arrived_meanwhile_becomes_a_conflict_never_a_revert() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    let mate = post(
        &rig.t.app,
        &SaveVerse {
            at: rig.at.clone(),
            request: rid(50),
            shown: row(&view, 2, "9").words.token.clone(),
            words: "Los siervos, que se sujeten".to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(mate.stored, Stored::Written);
    let ack = rig
        .save(1, &view, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    assert_eq!(ack.new_conflicts.len(), 1, "{ack:?}");
    let after = rig.chapters().await;
    assert_eq!(after.conflicts, 1);
    assert_eq!(
        row(&after, 2, "9")
            .words
            .marker
            .as_ref()
            .map(|m| m.versions),
        Some(2)
    );
    assert_eq!(
        row(&after, 2, "10").words.value,
        TEN,
        "the verse nobody else touched is saved"
    );
    rig.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_view_that_does_not_fit_together_is_not_saved() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    rig.save(1, &view, 2, &["9", "10"], &format!("9-10 {NINE} {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    // The span's head without the verse it covers.
    let lone = SaveSection {
        at: rig.at.clone(),
        request: rid(2),
        book: BookId::Titus,
        displayed: vec![row(&view, 2, "9-10").words.token.clone()],
        text: "9 a".to_owned(),
    };
    let refused = post(&rig.t.app, &lone).await.unwrap_err();
    assert_eq!(
        refused,
        ApiError::NotSaved(NotSaved::Refused(
            tc_api::write::SaveRefusal::IncoherentView
        ))
    );
    assert_eq!(rig.chapters().await, view, "nothing was written");
    rig.finish().await;
}

// ---------------------------------------------------------------------------------------------
// #786: poems come back byte for byte.
// ---------------------------------------------------------------------------------------------

/// The sample with Titus 3 replaced by `chapter`, a chapter of poetry.
fn with_titus_3(chapter: &str) -> ScenarioState {
    let files = scenario("sample").files().iter().map(|(path, bytes)| {
        if path.ends_with("sample_burrito/ingredients/TIT.usfm") {
            let usfm = String::from_utf8(bytes.clone()).unwrap();
            let at = usfm.find("\\c 3\n").expect("the sample has a Titus 3");
            (
                path.clone(),
                format!("{}{chapter}", &usfm[..at]).into_bytes(),
            )
        } else {
            (path.clone(), bytes.clone())
        }
    });
    ScenarioState::from_files(files).unwrap()
}

/// Psalm 3 of tC4's aligned ULT, from its `\c 3`: poetry lines, a heading before verse 1, alignment
/// markup inside the words, and a verse that opens on a poetry marker.
fn psalm_3() -> String {
    let usfm = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/tc4/test-fixtures/en_ult/PSA-3.usfm"
    ))
    .unwrap();
    let at = usfm.find("\\c 3\n").unwrap();
    usfm[at..].to_owned()
}

/// Genesis 3:14-17 as the ULT sets it: the curse in poetry, each verse of several lines.
const GENESIS_3_14: &str = "\\c 3\n\\p\n\\v 14 Yahweh God said to the serpent,\n\\q1 \"Because you have done this,\n\\q2 you are cursed above all livestock,\n\\q2 and above every animal of the field.\n\\q1 On your belly you will go,\n\\q2 and dust you will eat\n\\q2 all the days of your life.\n\\q1\n\\v 15 I will put hostility between you and the woman,\n\\q2 and between your offspring and her offspring.\n\\q1 He will bruise your head,\n\\q2 and you will bruise his heel.\"\n\\p\n\\v 16 To the woman he said,\n\\q1 \"I will greatly increase your pain in childbearing;\n\\q2 in pain you will give birth to children.\n\\q1 Your desire will be for your husband,\n\\q2 and he will rule over you.\"\n\\p\n\\v 17 To Adam he said, \"Because you have listened to the voice of your wife\n\\q1 and have eaten from the tree,\n\\q2 cursed is the ground because of you.\n";

/// For every section of the poem, as one card or one verse at a time, the card's opening text
/// saves as no change, and a change to one verse writes that verse's words only, each of its
/// lines kept.
async fn a_poem_comes_back_byte_for_byte(chapter: &str) {
    let rig = Rig::over(&with_titus_3(chapter)).await;
    let view = rig.chapters().await;
    let keys: Vec<&str> = view.chapters[2]
        .verses
        .iter()
        .map(|v| v.key.as_str())
        .collect();
    assert!(keys.len() >= 2, "{keys:?}");
    assert!(
        view.chapters[2]
            .verses
            .iter()
            .any(|v| v.words.value.contains("\n\t")),
        "a verse of the poem holds several lines"
    );
    let mut request = 0;
    let mut check = async |section: &[&str]| {
        request += 1;
        let sent = opening(&view, 3, section);
        let ack = rig.save(request, &view, 3, section, &sent).await.unwrap();
        assert_eq!(ack.stored, Stored::Unchanged, "{section:?} {sent:?}");
    };
    check(&keys).await;
    for pair in keys.windows(2) {
        check(pair).await;
    }
    for key in &keys {
        check(&[key]).await;
    }
    // One verse changed: its words are the text typed, the other verses are as they were.
    let target = keys[1];
    let old = row(&view, 3, target).words.value.clone();
    let verses: Vec<SectionVerse> = keys
        .iter()
        .map(|key| {
            let r = row(&view, 3, key);
            SectionVerse {
                key: (*key).to_owned(),
                drafted: r.drafted,
                body: if *key == target {
                    format!("{} Selah", js_trim(&old))
                } else {
                    js_trim(&r.words.value).to_owned()
                },
                format: r.format,
            }
        })
        .collect();
    let typed = initial_text(
        &verses,
        &expand_keys(&keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>()),
    );
    let ack = rig.save(99, &view, 3, &keys, &typed).await.unwrap();
    assert_eq!(ack.stored, Stored::Written);
    let after = rig.chapters().await;
    for key in &keys {
        let (was, now) = (row(&view, 3, key), row(&after, 3, key));
        if *key == target {
            // A save stores the NFC form of what was typed (STORAGE §5.1).
            let typed: String = format!("{} Selah", js_trim(&old)).nfc().collect();
            assert_eq!(now.words.value, typed);
            assert_eq!(now.format, was.format);
        } else {
            assert_eq!(
                now.words, was.words,
                "verse {key} is untouched, token and all"
            );
        }
    }
    rig.finish().await;
}

/// REF1b, INV-EVERY-POEM-LINE-STAYS: the cards show poem lines as tabs (tC4 #810), and what a
/// person saves from them is written to the book as `\q` markers, every other line kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tabs_saved_from_a_card_are_written_as_q_markers() {
    let rig = Rig::over(&with_titus_3(GENESIS_3_14)).await;
    let view = rig.chapters().await;
    let fifteen = row(&view, 3, "15");
    assert_eq!(
        fifteen.words.value,
        "I will put hostility between you and the woman,\n\t\tand between your offspring and her offspring.\n\tHe will bruise your head,\n\t\tand you will bruise his heel.\""
    );
    // The verse card: one line changed, one line added at the third level.
    let typed =
        fifteen.words.value.replace("his heel.\"", "his heel,") + "\n\t\t\tand it will be so.\"";
    let ack = post(
        &rig.t.app,
        &SaveVerse {
            at: rig.at.clone(),
            request: rid(70),
            shown: fifteen.words.token.clone(),
            words: typed.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    assert_eq!(
        ack.saved[0].text, typed,
        "the card's text comes back as it was typed"
    );
    // The section card: verse 16's second line moves to the first level.
    let after = rig.chapters().await;
    let sixteen = row(&after, 3, "16").words.value.clone();
    assert!(sixteen.contains("\n\t\tin pain"), "{sixteen:?}");
    let verses: Vec<SectionVerse> = ["16"]
        .iter()
        .map(|key| SectionVerse {
            key: (*key).to_owned(),
            drafted: true,
            body: sixteen.replace("\n\t\tin pain", "\n\tin pain"),
            format: row(&after, 3, key).format,
        })
        .collect();
    let text = initial_text(&verses, &expand_keys(&["16".to_owned()]));
    let ack = rig.save(71, &after, 3, &["16"], &text).await.unwrap();
    assert_eq!(ack.stored, Stored::Written);
    rig.finish().await;
    let book = rig
        .fs
        .files_under(&Path::new(PROJECTS).join(SAMPLE))
        .into_iter()
        .find(|(path, _)| path.ends_with("ingredients/TIT.usfm"))
        .map(|(_, bytes)| String::from_utf8(bytes).unwrap())
        .expect("the book is written");
    assert!(!book.contains('\t'), "no tab reaches the book");
    assert!(book.contains(
        "\\v 15 I will put hostility between you and the woman,\n\\q2 and between your offspring and her offspring.\n\\q1 He will bruise your head,\n\\q2 and you will bruise his heel,\n\\q3 and it will be so.\"\n"
    ), "{book}");
    assert!(
        book.contains("\\q1 in pain you will give birth to children."),
        "{book}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_change_section_save_on_psalm_3_writes_nothing() {
    a_poem_comes_back_byte_for_byte(&psalm_3()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_change_section_save_on_genesis_3_14_to_17_writes_nothing() {
    a_poem_comes_back_byte_for_byte(GENESIS_3_14).await;
}

impl Rig {
    /// Home's Resume card: the verse it names and its opening text.
    async fn resume(&self) -> (String, String) {
        let home: tc_api::home::HomeView = post(&self.t.app, &tc_api::home::List {}).await.unwrap();
        let resume = home.resume.expect("a Resume card");
        let tc_api::home::ResumePlace::Book { verse, .. } = resume.place else {
            panic!("{:?}", resume.place)
        };
        (verse.as_str().to_owned(), resume.snippet)
    }
}

/// tC4 #576: after a section save, Home's Resume card names the verse as it now stands: a
/// join names the span ("9-10"), a break the first verse of the old span.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_edited_names_the_span_then_its_first_verse() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    rig.save(1, &view, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    let view = rig.chapters().await;
    rig.save(2, &view, 2, &["9", "10"], &format!("9-10 {NINE} {TEN}"))
        .await
        .unwrap();
    assert_eq!(rig.resume().await.0, "9-10", "a join names the span");
    let view = rig.chapters().await;
    rig.save(3, &view, 2, &["9-10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    assert_eq!(rig.resume().await.0, "9", "a break names the first verse");
    rig.finish().await;
}

/// tC4 #831: the Resume card's text is the text of the verse it names, never the whole
/// section with its verse numbers, and never the stub "___".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_card_names_the_verse_of_its_text_and_never_shows_the_stub() {
    let rig = Rig::sample().await;
    let view = rig.chapters().await;
    assert_eq!(row(&view, 2, "9").words.value, "___");
    // Only verse 10 is drafted: the card names 9, the section's first verse, which holds the
    // stub, so its text is empty rather than "___" or the section's text.
    rig.save(1, &view, 2, &["9", "10"], &format!("9 \n10 {TEN}"))
        .await
        .unwrap();
    let (verse, snippet) = rig.resume().await;
    assert_eq!(verse, "9");
    assert!(
        !snippet.contains("___") && !snippet.contains(TEN),
        "{snippet:?}"
    );
    let view = rig.chapters().await;
    rig.save(2, &view, 2, &["9", "10"], &format!("9 {NINE}\n10 {TEN}"))
        .await
        .unwrap();
    let (verse, snippet) = rig.resume().await;
    assert_eq!(verse, "9");
    assert_eq!(snippet, NINE, "the verse's own text, not the section's");
    rig.finish().await;
}
