//! INV-EVERY-POEM-LINE-STAYS (REF1b): the editors show a verse's poem lines with tabs, its
//! breaks as empty lines, and saving gives back the markers; an unchanged verse saves its exact
//! bytes. Cases ported from tC4's `test/poemLines.test.ts` (d25b7871), on its fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use tc_usfm::poem::{editor_text, shows_tabs, words_from_editor_text};

const BODY: &str =
    "I will also put hostility\n\\q2 between you and the woman,\n\\q1 and between your offspring.";

#[test]
fn poem_lines_show_one_tab_per_level() {
    assert_eq!(
        editor_text(BODY),
        "I will also put hostility\n\t\tbetween you and the woman,\n\tand between your offspring."
    );
    assert_eq!(editor_text("a\n\\q3 deeper"), "a\n\t\t\tdeeper");
}

#[test]
fn an_unchanged_verse_saves_its_exact_bytes() {
    for words in [
        BODY,
        "a\n\\q2 two  spaces\tand a tab \n\\q1 b",
        "one\r\n\\q2 two\r\n\\ts\\*\r\n\\q1 three",
        "saying,\n\\pi1\nAmen! Praise\n\\ts\\*\n\\m\nand glory.\n\\q3 deeper\n\\s1 A heading\n\\pmo “Quoted”",
    ] {
        assert!(shows_tabs(words), "{words:?}");
        assert_eq!(words_from_editor_text(&editor_text(words), words), words);
    }
}

#[test]
fn a_crlf_verse_keeps_crlf_after_an_edit() {
    let crlf = "one\r\n\\q2 two\r\n\\ts\\*\r\n\\q1 three";
    let text = editor_text(crlf);
    assert_eq!(text, "one\n\t\ttwo\n\n\tthree");
    assert_eq!(
        words_from_editor_text(&text.replace("two", "dos"), crlf),
        "one\r\n\\q2 dos\r\n\\ts\\*\r\n\\q1 three"
    );
}

#[test]
fn breaks_show_as_empty_lines_and_come_back_in_order() {
    let body = "saying,\n\\pi1\nAmen! Praise\n\\ts\\*\n\\m\nand glory.\n\\q3 deeper\n\\s1 A heading\n\\pmo “Quoted”";
    let text = editor_text(body);
    assert_eq!(
        text,
        "saying,\n\nAmen! Praise\n\n\nand glory.\n\t\t\tdeeper\n\\s1 A heading\n\\pmo “Quoted”"
    );
    assert_eq!(
        words_from_editor_text(&text.replace("Amen! Praise", "Amén, alabanza"), body),
        body.replace("Amen! Praise", "Amén, alabanza")
    );
}

#[test]
fn a_new_empty_line_is_a_paragraph_and_tabs_become_markers() {
    assert_eq!(words_from_editor_text("one\n\ntwo", "one"), "one\n\\p\ntwo");
    assert_eq!(
        words_from_editor_text("one\n\ntwo", "one\n\\ts\\*\ntwo\n\\b\nthree"),
        "one\n\\ts\\*\ntwo"
    );
    assert_eq!(
        words_from_editor_text("\tfirst\nplain break\n\tpoem", ""),
        "first\nplain break\n\\q1 poem"
    );
    assert_eq!(
        words_from_editor_text("a\n\t\t\tdeeper", "a"),
        "a\n\\q3 deeper"
    );
}

/// tC4 writes a bare `\q` back as `\q1` and `\Q2` as `\q2`: the rebuild shows such a line as it
/// is written, and keeps a verse on its markers where tabs could not give its bytes back (a line
/// that starts with a tab, mixed line ends), so nothing the translator did not touch changes.
#[test]
fn a_line_tabs_would_rewrite_stays_as_written() {
    for words in ["a\n\\q b", "a\n\\Q2 b"] {
        assert!(shows_tabs(words), "{words:?}");
        assert_eq!(editor_text(words), words);
        assert_eq!(words_from_editor_text(words, words), words);
    }
    for words in ["a\n\tled by a tab", "a\r\n\\q2 b\n\\q1 c"] {
        assert!(!shows_tabs(words), "{words:?}");
        assert_eq!(editor_text(words), words);
        assert_eq!(words_from_editor_text(words, words), words);
    }
}

#[test]
fn four_tabs_are_not_a_q3_line() {
    assert_eq!(
        words_from_editor_text("a\n\t\t\t\tfour", "a"),
        "a\n\t\t\t\tfour"
    );
}

#[test]
fn every_verse_of_tc4s_poetry_fixtures_round_trips() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc4/test-fixtures");
    let mut tabbed = 0;
    for file in [
        "en_ult/GEN-773.usfm",
        "en_ult/PSA-3.usfm",
        "en_ult/1SA-2-812.usfm",
        "en_ult/REV-7-812.usfm",
        "en_ult/PSA-9-812.usfm",
        "en_ult/MAT-27-812.usfm",
        "en_ust/GEN-29-812.usfm",
    ] {
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        let parts = tc_usfm::codec::decompose(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
        for (c, chapter) in &parts.chapters {
            for (v, part) in &chapter.verses {
                let tc_usfm::codec::VersePart::Text(verse) = part else {
                    continue;
                };
                let words = &verse.words;
                assert_eq!(
                    words_from_editor_text(&editor_text(words), words),
                    *words,
                    "{file} {c}:{v}"
                );
                if shows_tabs(words) && editor_text(words).contains('\t') {
                    tabbed += 1;
                }
            }
        }
    }
    // Aligned ULT puts most `\q1` markers on lines of their own, which show as empty lines.
    assert_eq!(tabbed, 8, "the fixtures' verses with a `\\qN text` line");
}

fn poem_words() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        "[a-z]{1,6}( [a-z]{1,6}){0,3}".prop_map(|t| format!("\\q1 {t}")),
        "[a-z]{1,6}( [a-z]{1,6}){0,3}".prop_map(|t| format!("\\q2 {t}")),
        "[a-z]{1,6}( [a-z]{1,6}){0,3}".prop_map(|t| format!("\\q3 {t}")),
        Just("\\p".to_owned()),
        Just("\\b".to_owned()),
        Just("\\ts\\*".to_owned()),
        "[a-z]{1,6}( [a-z]{1,6}){0,3}",
    ];
    (
        "[a-z]{1,6}( [a-z]{1,6}){0,3}",
        prop::collection::vec(line, 0..8),
        any::<bool>(),
    )
        .prop_map(|(first, rest, crlf)| {
            let nl = if crlf { "\r\n" } else { "\n" };
            std::iter::once(first)
                .chain(rest)
                .collect::<Vec<_>>()
                .join(nl)
        })
}

proptest! {
    /// Editing the words of one line of the editor text changes that line only.
    #[test]
    fn an_edit_of_one_line_changes_only_that_line(words in poem_words(), pick in any::<prop::sample::Index>()) {
        prop_assume!(shows_tabs(&words));
        let text = editor_text(&words);
        let lines: Vec<&str> = text.split('\n').collect();
        let at = pick.index(lines.len());
        prop_assume!(lines[at].trim_start_matches('\t').chars().next().is_some_and(char::is_alphabetic));
        let edited: Vec<String> = lines.iter().enumerate()
            .map(|(i, l)| if i == at { format!("{l}x") } else { (*l).to_owned() })
            .collect();
        let saved = words_from_editor_text(&edited.join("\n"), &words);
        let nl = if words.contains("\r\n") { "\r\n" } else { "\n" };
        let before: Vec<&str> = words.split(nl).collect();
        let after: Vec<&str> = saved.split(nl).collect();
        prop_assert_eq!(before.len(), after.len());
        for (i, (b, a)) in before.iter().zip(&after).enumerate() {
            if i == at { prop_assert_eq!(format!("{b}x"), (*a).to_owned()); } else { prop_assert_eq!(b, a); }
        }
    }
}
