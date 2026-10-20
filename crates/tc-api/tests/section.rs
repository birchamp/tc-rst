//! Section drafting (APP-DESIGN §3.8, AD24, unit B4b): `tc_api::model::section` against what
//! tC4's `sectionDraft.js` answers (`fixtures/usfm/section-draft-vectors.json`, 729 texts, 160
//! lists of verses and 249 indents), the ports of tC4's `test/sectionDraft.test.ts`, and
//! properties: a placement never changes the words, and the Type text is a fixed point.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::Value;
use tc_api::model::section::{
    ParaFormat, SectionBuffer, SectionVerse, expand_keys, indent_line, initial_text,
};

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| (*k).to_owned()).collect()
}

fn format_of(name: &str) -> ParaFormat {
    match name {
        "p" => ParaFormat::P,
        "q1" => ParaFormat::Q1,
        "q2" => ParaFormat::Q2,
        other => panic!("{other} is not a format"),
    }
}

fn vectors() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/usfm/section-draft-vectors.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

/// A line that holds only a verse number (or a run) and asks for a format: tabs before it, or
/// a blank line.
fn number_alone_with_a_format(text: &str) -> bool {
    let mut blank_before = false;
    text.split('\n').any(|line| {
        let rest = line.trim_start_matches('\t');
        let number = rest.trim_end();
        let alone = !number.is_empty()
            && number.split('-').count() <= 2
            && number
                .split('-')
                .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        let found = alone && (rest.len() < line.len() || blank_before);
        blank_before = line.trim().is_empty();
        found
    })
}

/// The format before each verse that has words, keyed by the verse before it.
fn lines_before_words(b: &SectionBuffer) -> Vec<(String, Option<ParaFormat>)> {
    let groups = b.groups();
    let placed_after = |key: &str| {
        groups
            .windows(2)
            .any(|pair| pair[0].key == key && pair[1].at.is_some())
    };
    b.formats()
        .into_iter()
        .filter(|(key, _)| placed_after(key))
        .collect()
}

/// The non-blank lines of a Type text without their leading tabs.
fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.trim_start_matches('\t'))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

#[test]
fn expand_keys_is_tc4s() {
    for case in vectors()["expand"].as_array().unwrap() {
        assert_eq!(
            expand_keys(&strings(&case["keys"])),
            strings(&case["pins"]),
            "{case}"
        );
    }
}

/// Every text tC4 reads, the Rust port reads the same: the words, the separators, where each
/// pin sits, the blocks, the verses a save writes and their keys, the Type text written back,
/// the words each pin may be dropped on and the pins `dropPin` gives.
#[test]
fn the_port_reads_every_text_as_tc4_does() {
    let all = vectors();
    let cases = all["parse"].as_array().unwrap();
    assert!(cases.len() >= 700);
    for case in cases {
        let k = strings(&case["keys"]);
        let text = case["text"].as_str().unwrap();
        let b = SectionBuffer::parse(text, &k);
        let at = format!("{k:?} {text:?}");
        assert_eq!(b.words(), strings(&case["words"]), "words of {at}");
        assert_eq!(b.seps(), strings(&case["seps"]), "seps of {at}");
        let blocks: BTreeMap<usize, ParaFormat> = case["blocks"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(i, f)| (i.parse().unwrap(), format_of(f.as_str().unwrap())))
            .collect();
        assert_eq!(b.blocks(), &blocks, "blocks of {at}");
        let markers = case["markers"].as_object().unwrap();
        for pin in b.pins() {
            let wanted = markers.get(pin).map(|v| v.as_u64().unwrap() as usize);
            assert_eq!(b.marker(pin), wanted, "pin {pin} of {at}");
        }
        if b.is_empty() {
            continue;
        }
        let groups: Vec<(String, Vec<String>, Option<usize>)> = b
            .groups()
            .into_iter()
            .map(|g| (g.key, g.members, g.at))
            .collect();
        let wanted: Vec<(String, Vec<String>, Option<usize>)> = case["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                (
                    g["key"].as_str().unwrap().to_owned(),
                    strings(&g["members"]),
                    g["at"].as_u64().map(|n| n as usize),
                )
            })
            .collect();
        assert_eq!(groups, wanted, "groups of {at}");
        assert_eq!(b.new_keys(), strings(&case["newKeys"]), "keys of {at}");
        let verses: BTreeMap<String, String> = b.verses().into_iter().collect();
        let wanted: BTreeMap<String, String> = case["verses"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(verses, wanted, "verses of {at}");
        // A declared difference (B4b): when the section's first verse has no text, the first
        // line written keeps the paragraph or poetry line typed before it, which tC4's
        // `serializeDraft` drops, so a Place session's text saves the formats it holds.
        let first_is_stub = b.groups().first().is_some_and(|g| g.at.is_none());
        let lead = match b.block_at(0).filter(|_| first_is_stub) {
            Some(ParaFormat::P) => "\n",
            Some(ParaFormat::Q1) => "\t",
            Some(ParaFormat::Q2) => "\t\t",
            None => "",
        };
        let ours = b.serialize();
        let theirs = format!("{lead}{}", case["serialized"].as_str().unwrap());
        if number_alone_with_a_format(text) {
            // The other declared difference (B4b): a format typed on a verse number's own line
            // stays with that verse, where tC4 writes it before the next verse with words. The
            // lines are the same; only their formats may differ.
            assert_eq!(lines_of(&ours), lines_of(&theirs), "serialized {at}");
        } else {
            assert_eq!(ours, theirs, "serialized {at}");
        }
        for (pin, indices) in case["canDrop"].as_object().unwrap() {
            let allowed: Vec<usize> = indices
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect();
            for index in 0..=b.words().len() + 1 {
                assert_eq!(
                    b.can_drop(pin, index),
                    allowed.contains(&index),
                    "can_drop {pin} at {index} of {at}"
                );
            }
        }
        for drop in case["drops"].as_array().unwrap() {
            let (pin, index) = (
                drop["pin"].as_str().unwrap(),
                drop["index"].as_u64().unwrap() as usize,
            );
            let after = b.drop_pin(pin, index).expect("tC4 allowed the drop");
            let markers = drop["markers"].as_object().unwrap();
            for p in after.pins() {
                let wanted = markers.get(p).map(|v| v.as_u64().unwrap() as usize);
                assert_eq!(
                    after.marker(p),
                    wanted,
                    "pin {p} after {pin}@{index} of {at}"
                );
            }
        }
        for end in case["spanEnds"].as_array().unwrap() {
            let index = end[0].as_u64().unwrap() as usize;
            let held = end[1].as_str();
            assert_eq!(
                b.span_end(index, held),
                end[2].as_u64().unwrap() as usize,
                "span end {index} {held:?} of {at}"
            );
        }
    }
}

#[test]
fn the_port_opens_every_section_as_tc4_does() {
    for case in vectors()["initial"].as_array().unwrap() {
        let k = strings(&case["keys"]);
        let verses: Vec<SectionVerse> = case["verses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| SectionVerse {
                key: v["n"].as_str().unwrap().to_owned(),
                drafted: v["drafted"].as_bool().unwrap(),
                body: v["body"].as_str().unwrap().to_owned(),
                format: v["format"].as_str().map(format_of),
            })
            .collect();
        assert_eq!(
            initial_text(&verses, &expand_keys(&k)),
            case["text"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn the_port_indents_as_tc4_does() {
    for case in vectors()["indent"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let caret = case["caret"].as_u64().unwrap() as usize;
        let delta = case["delta"].as_i64().unwrap() as i32;
        let (out, at) = indent_line(text, caret, delta);
        assert_eq!(out, case["out"].as_str().unwrap(), "{case}");
        assert_eq!(at, case["caretOut"].as_u64().unwrap() as usize, "{case}");
    }
}

// ---------------------------------------------------------------------------------------------
// The ports of tC4's test/sectionDraft.test.ts, by name, over the same texts.
// ---------------------------------------------------------------------------------------------

fn two() -> Vec<String> {
    keys(&["9", "10"])
}

fn three() -> Vec<String> {
    keys(&["3", "4", "5"])
}

fn verses_of(b: &SectionBuffer) -> Vec<(String, String)> {
    b.verses()
}

fn pair(k: &str, v: &str) -> (String, String) {
    (k.to_owned(), v.to_owned())
}

/// `text` over `keys`, then each `(pin, index)` dropped, as a card's placements are.
fn placed(text: &str, k: &[String], drops: &[(&str, usize)]) -> SectionBuffer {
    let mut b = SectionBuffer::parse(text, k);
    for (pin, index) in drops {
        b = b.drop_pin(pin, *index).expect("the drop is allowed");
    }
    b
}

#[test]
fn a_number_that_is_not_one_of_the_section_keys_is_text_not_a_marker() {
    let b = SectionBuffer::parse("9 Exhorta\n12 talentos", &two());
    assert_eq!(b.words(), ["Exhorta", "12", "talentos"]);
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), None));
}

#[test]
fn the_first_verse_is_pinned_at_0_and_out_of_order_pins_return_to_the_bank() {
    let b = SectionBuffer::parse("antes\n9 Exhorta\n10 no", &two());
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), Some(2)));
    let out = SectionBuffer::parse("10 aaa\n9 bbb", &two());
    assert_eq!(out.words(), ["aaa", "bbb"]);
    assert_eq!((out.marker("9"), out.marker("10")), (Some(0), None));
}

#[test]
fn an_empty_section_has_no_words_and_no_markers() {
    for text in ["", "  \n "] {
        let b = SectionBuffer::parse(text, &two());
        assert!(b.is_empty() && b.seps().is_empty() && b.blocks().is_empty());
        assert_eq!((b.marker("9"), b.marker("10")), (None, None));
    }
}

#[test]
fn a_stub_verse_before_a_drafted_one_keeps_the_text_with_its_own_verse() {
    let b = SectionBuffer::parse("9 \n10 no defraudando", &two());
    assert_eq!((b.marker("9"), b.marker("10")), (None, Some(0)));
    assert_eq!(verses_of(&b), [pair("10", "no defraudando")]);
    assert_eq!(b.new_keys(), ["9", "10"], "no span: the keys are unchanged");
}

#[test]
fn bridged_keys_work_as_markers() {
    let b = SectionBuffer::parse("9-10 Exhorta a los siervos", &keys(&["9-10"]));
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), Some(0)));
    assert_eq!(b.words().first().map(String::as_str), Some("Exhorta"));
}

#[test]
fn every_separator_inside_a_verse_survives_so_a_save_does_not_rewrite_untouched_text() {
    let (nine, ten) = ("línea uno\nlínea dos", "no\u{a0}defraudando");
    let b = SectionBuffer::parse(&format!("9 {nine}\n10 {ten}"), &two());
    assert_eq!(verses_of(&b), [pair("9", nine), pair("10", ten)]);
}

#[test]
fn the_pinned_word_list_round_trips_through_type_mode_text() {
    let b = placed("a b c d e", &three(), &[("4", 2), ("5", 4)]);
    let text = b.serialize();
    assert_eq!(text, "3 a b\n4 c d\n5 e");
    let back = SectionBuffer::parse(&text, &three());
    assert_eq!(back.words(), b.words());
    for pin in ["3", "4", "5"] {
        assert_eq!(back.marker(pin), b.marker(pin));
    }
    // The line breaks the serializer wrote are separators, not lost bytes.
    assert_eq!(back.seps(), [" ", "\n", " ", "\n", ""]);
    assert_eq!(back.serialize(), text);
}

#[test]
fn an_unplaced_verse_gets_no_text_and_its_words_stay_with_the_verse_before_it() {
    let b = placed("a b c d e", &three(), &[("5", 3)]);
    assert_eq!(verses_of(&b), [pair("3", "a b c"), pair("5", "d e")]);
    assert_eq!(
        SectionBuffer::parse("a b c d e", &three()).serialize(),
        "3 a b c d e"
    );
}

#[test]
fn the_words_of_every_placement_concatenate_back_to_the_original_list() {
    let drops: [&[(&str, usize)]; 4] = [&[], &[("4", 1)], &[("4", 1), ("5", 2)], &[("5", 4)]];
    for drops in drops {
        let b = placed("a b c d e", &three(), drops);
        let joined: Vec<String> = b
            .verses()
            .iter()
            .flat_map(|(_, t)| t.split(' ').map(str::to_owned).collect::<Vec<_>>())
            .collect();
        assert_eq!(joined, ["a", "b", "c", "d", "e"], "{drops:?}");
    }
}

fn base() -> SectionBuffer {
    placed("a b c d e f", &three(), &[("4", 2), ("5", 4)])
}

#[test]
fn the_first_verse_cannot_be_moved_anywhere() {
    assert!(!base().can_drop("3", 1) && !base().can_drop("3", 0));
}

#[test]
fn a_middle_pin_may_move_between_its_neighbours_onto_either_but_never_past() {
    let b = base();
    assert!(b.can_drop("4", 1) && b.can_drop("4", 3));
    assert!(b.can_drop("4", 0), "onto the fixed first pin: span 3-4");
    assert!(b.can_drop("4", 4), "onto verse 5's pin: span 4-5");
    assert!(!b.can_drop("4", 5), "past verse 5");
}

#[test]
fn the_last_pin_may_move_anywhere_from_the_pin_before_it_on() {
    let b = base();
    assert!(b.can_drop("5", 3) && b.can_drop("5", 9));
    assert!(b.can_drop("5", 2), "onto verse 4's pin: span 4-5");
    assert!(!b.can_drop("5", 1));
}

#[test]
fn an_unplaced_pin_obeys_the_same_order_against_the_placed_ones() {
    let b = placed("a b c d e f", &three(), &[("5", 4)]);
    assert!(b.can_drop("4", 2) && b.can_drop("4", 4) && !b.can_drop("4", 6));
    let b = placed("a b c d e f", &three(), &[("4", 2)]);
    assert!(b.can_drop("5", 2) && !b.can_drop("5", 1) && b.can_drop("5", 3));
}

#[test]
fn a_pin_unknown_to_the_section_cannot_be_dropped() {
    assert!(!base().can_drop("7", 1));
}

#[test]
fn a_verse_runs_to_the_next_pin_after_it_the_held_pin_ignored() {
    let b = base();
    assert_eq!(b.span_end(0, None), 2);
    assert_eq!(b.span_end(1, Some("4")), 4);
    assert_eq!(b.span_end(4, None), 6);
}

fn wrapped() -> Vec<SectionVerse> {
    let verse = |key: &str, body: &str| SectionVerse {
        key: key.to_owned(),
        drafted: true,
        body: body.to_owned(),
        format: None,
    };
    vec![
        verse("9", "y le dio\n10 talentos"),
        verse("10", "no defraudando"),
    ]
}

#[test]
fn the_card_writes_a_line_that_looks_like_a_marker_escaped_and_the_parse_gives_every_verse_its_own_words_back()
 {
    let pins = expand_keys(&two());
    let text = initial_text(&wrapped(), &pins);
    assert_eq!(text, "9 y le dio\n 10 talentos\n10 no defraudando");
    let b = SectionBuffer::parse(&text, &two());
    assert_eq!(
        verses_of(&b),
        [
            pair("9", "y le dio\n10 talentos"),
            pair("10", "no defraudando")
        ]
    );
    // The escape survives a Type, Place, Type round trip.
    let back = b.serialize();
    assert_eq!(back, "9 y le dio\n 10 talentos\n10 no defraudando");
    assert_eq!(
        verses_of(&SectionBuffer::parse(&back, &two())),
        verses_of(&b)
    );
}

#[test]
fn a_body_line_that_already_begins_with_a_space_keeps_that_space() {
    let mut verses = wrapped();
    verses[0].body = "y le dio\n 10 talentos".to_owned();
    let text = initial_text(&verses, &expand_keys(&two()));
    assert_eq!(text, "9 y le dio\n  10 talentos\n10 no defraudando");
    let b = SectionBuffer::parse(&text, &two());
    let want = [
        pair("9", "y le dio\n 10 talentos"),
        pair("10", "no defraudando"),
    ];
    assert_eq!(verses_of(&b), want);
    assert_eq!(
        verses_of(&SectionBuffer::parse(&b.serialize(), &two())),
        want
    );
}

#[test]
fn a_number_that_is_not_a_section_key_needs_no_escape() {
    let mut verses = wrapped();
    verses[0].body = "y le dio\n12 talentos".to_owned();
    let text = initial_text(&verses, &expand_keys(&two()));
    assert_eq!(text, "9 y le dio\n12 talentos\n10 no defraudando");
    assert_eq!(
        verses_of(&SectionBuffer::parse(&text, &two())),
        [
            pair("9", "y le dio\n12 talentos"),
            pair("10", "no defraudando")
        ]
    );
}

#[test]
fn a_section_with_no_draft_opens_empty() {
    let stub = |key: &str| SectionVerse {
        key: key.to_owned(),
        drafted: false,
        body: String::new(),
        format: None,
    };
    assert_eq!(
        initial_text(&[stub("9"), stub("10")], &expand_keys(&two())),
        ""
    );
}

#[test]
fn expand_keys_turns_a_span_key_into_its_verse_numbers() {
    assert_eq!(expand_keys(&keys(&["9-10", "11"])), ["9", "10", "11"]);
    assert_eq!(expand_keys(&two()), ["9", "10"]);
}

#[test]
fn stacked_pins_are_one_span_and_a_pin_moved_past_text_breaks_it() {
    let stacked = placed("a b c d", &three(), &[("4", 0), ("5", 0)]);
    assert_eq!(stacked.new_keys(), ["3-5"]);
    let b = placed("a b c d", &three(), &[("4", 0), ("5", 2)]);
    assert_eq!(verses_of(&b), [pair("3-4", "a b"), pair("5", "c d")]);
    let b = placed("a b c d", &three(), &[("4", 2), ("5", 2)]);
    assert_eq!(b.new_keys(), ["3", "4-5"]);
}

#[test]
fn type_mode_reads_and_writes_a_span_as_one_line() {
    let b = SectionBuffer::parse("9-10 a b c d", &two());
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), Some(0)));
    assert_eq!(b.serialize(), "9-10 a b c d");
    let b = SectionBuffer::parse("3-4 a b\n5 c d", &three());
    assert_eq!(
        (b.marker("3"), b.marker("4"), b.marker("5")),
        (Some(0), Some(0), Some(2))
    );
    // A run that is not the section's pins is text.
    assert_eq!(
        SectionBuffer::parse("9-12 a", &two()).words(),
        ["9-12", "a"]
    );
}

#[test]
fn a_stub_span_stays_one_stub_key_while_all_its_pins_are_unplaced() {
    let k = keys(&["9-10", "11"]);
    let b = SectionBuffer::parse("9-10 \n11 c d", &k);
    assert_eq!(
        (b.marker("9"), b.marker("10"), b.marker("11")),
        (None, None, Some(0))
    );
    assert_eq!(b.new_keys(), ["9-10", "11"]);
}

#[test]
fn a_pin_never_passes_another_and_a_stack_cannot_swallow_an_unplaced_pin() {
    let only_first = placed("a b c d", &three(), &[]);
    assert!(
        !only_first.can_drop("5", 0),
        "4 would sit unplaced inside 3-5"
    );
    let two_stacked = placed("a b c d", &three(), &[("4", 0)]);
    assert!(two_stacked.can_drop("5", 0));
    let b = placed("a b c d", &three(), &[("5", 2)]);
    assert!(!b.can_drop("4", 3), "past 5");
    assert!(b.can_drop("4", 2), "onto 5: span 4-5");
}

#[test]
fn dropping_gives_the_first_verse_its_words_back_when_no_pin_begins_at_word_0_any_more() {
    let b = SectionBuffer::parse("9 \n10 a b c", &two());
    assert_eq!((b.marker("9"), b.marker("10")), (None, Some(0)));
    let b = b.drop_pin("10", 2).unwrap();
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), Some(2)));
    let b = b.drop_pin("10", 0).unwrap();
    assert_eq!((b.marker("9"), b.marker("10")), (Some(0), Some(0)));
}

#[test]
fn rule_1_leading_tabs_are_stripped_before_the_marker_match() {
    let b = SectionBuffer::parse("\t10 no defraudando", &two());
    assert_eq!(b.marker("10"), Some(0));
    assert_eq!(b.block_at(0), Some(ParaFormat::Q1));
    let b = SectionBuffer::parse("\t9-10 no defraudando", &keys(&["9-10"]));
    assert_eq!(
        (b.marker("9"), b.block_at(0)),
        (Some(0), Some(ParaFormat::Q1))
    );
}

#[test]
fn rule_2_enter_opens_a_paragraph() {
    assert_eq!(
        SectionBuffer::parse("9 a\n\n10 b", &two()).block_at(1),
        Some(ParaFormat::P)
    );
    assert_eq!(
        SectionBuffer::parse("9 todo\nno defraudando", &two()).block_at(1),
        Some(ParaFormat::P)
    );
    assert_eq!(SectionBuffer::parse("9 a\n10 b", &two()).block_at(1), None);
}

#[test]
fn rule_3_tab_gives_q1_a_second_tab_q2_and_tabs_win_over_a_blank_line() {
    let at = |text: &str| SectionBuffer::parse(text, &two()).block_at(1);
    assert_eq!(at("9 a\n\t10 b"), Some(ParaFormat::Q1));
    assert_eq!(at("9 a\n\t\t10 b"), Some(ParaFormat::Q2));
    assert_eq!(at("9 a\n\n\t10 b"), Some(ParaFormat::Q1));
    assert_eq!(indent_line("10 b", 0, 1), ("\t10 b".to_owned(), 1));
    assert_eq!(indent_line("\t10 b", 1, 1), ("\t\t10 b".to_owned(), 2));
    assert_eq!(
        indent_line("\t\t10 b", 2, 1),
        ("\t\t10 b".to_owned(), 2),
        "at most two"
    );
    assert_eq!(indent_line("\t\t10 b", 2, -1), ("\t10 b".to_owned(), 1));
    assert_eq!(indent_line("\t10 b", 1, -1), ("10 b".to_owned(), 0));
    assert_eq!(
        indent_line("10 b", 0, -1),
        ("10 b".to_owned(), 0),
        "no tabs"
    );
}

#[test]
fn rule_4_blocks_attached_to_words_survive_pin_placement() {
    let b = SectionBuffer::parse("9 a b\n\t10 c d", &two());
    assert_eq!(b.block_at(2), Some(ParaFormat::Q1));
    let moved = b.drop_pin("10", 3).unwrap();
    assert_eq!(moved.marker("10"), Some(3));
    assert_eq!(moved.block_at(2), Some(ParaFormat::Q1));
}

#[test]
fn rule_5_the_opening_text_writes_the_stored_formats() {
    let verse = |key: &str, format| SectionVerse {
        key: key.to_owned(),
        drafted: true,
        body: if key == "9" { "a" } else { "b" }.to_owned(),
        format,
    };
    let pins = expand_keys(&two());
    for (format, want) in [
        (Some(ParaFormat::Q1), "9 a\n\t10 b"),
        (Some(ParaFormat::Q2), "9 a\n\t\t10 b"),
        (Some(ParaFormat::P), "9 a\n\n10 b"),
    ] {
        assert_eq!(
            initial_text(&[verse("9", None), verse("10", format)], &pins),
            want
        );
    }
}

#[test]
fn serialize_of_parse_is_the_text_for_every_required_case() {
    let cases: [(&str, &[&str]); 5] = [
        ("9 a\n\n10 b", &["9", "10"]),
        ("9 a\n\t10 b", &["9", "10"]),
        ("9 a\n\t\t10 b", &["9", "10"]),
        ("9 a\n10 b", &["9", "10"]),
        ("9-10 a\n\t11 b", &["9-10", "11"]),
    ];
    for (text, k) in cases {
        assert_eq!(SectionBuffer::parse(text, &keys(k)).serialize(), text);
    }
}

/// #575 (D94): verses joined into a span are one paragraph; the blank line a `\p` left between
/// them becomes one space, and a single line break stays as typed.
#[test]
fn joined_span_collapses_blank_lines_to_one_space() {
    let b = placed("a b\n\nc d\ne", &keys(&["3", "4"]), &[("4", 0)]);
    assert_eq!(b.new_keys(), ["3-4"]);
    assert_eq!(verses_of(&b), [pair("3-4", "a b c d\ne")]);
    // Two verses keep their own blank lines.
    let b = SectionBuffer::parse("3 a b\n\nc d", &keys(&["3", "4"]));
    assert_eq!(verses_of(&b), [pair("3", "a b\n\nc d")]);
}

/// #598 in one Place session: a number moved from one word to another and back leaves the
/// verses as they were, and the text the session writes reads back the same. The card read its
/// own text again after each drop, and the line break each drop wrote stayed in verse 10 ("b\nc")
/// with a new paragraph before it.
#[test]
fn a_number_moved_twice_in_one_place_session_adds_no_line() {
    let k = keys(&["9", "10"]);
    let b = placed("9 a b c", &k, &[("10", 1), ("10", 2), ("10", 1)]);
    assert_eq!(verses_of(&b), [pair("9", "a"), pair("10", "b c")]);
    assert_eq!(b.formats(), [("9".to_owned(), None)]);
    let back = SectionBuffer::parse(&b.serialize(), &k);
    assert_eq!(verses_of(&back), verses_of(&b));
    assert_eq!(back.formats(), b.formats());
}

/// When the section's first verse has no words, the text a Place session writes keeps the
/// format typed before its first line, which tC4's `serializeDraft` drops, so the save reads the
/// session's formats.
#[test]
fn the_first_line_written_keeps_its_format_when_the_first_verse_has_no_words() {
    let k = keys(&["1", "2", "3", "4", "5"]);
    let b = placed("\t2-3 0 x", &k, &[("4", 1)]);
    assert_eq!(b.serialize(), "\t2-3 0\n4 x");
    let back = SectionBuffer::parse(&b.serialize(), &k);
    assert_eq!(back.formats(), b.formats());
    assert_eq!(
        b.formats().first(),
        Some(&("1".to_owned(), Some(ParaFormat::Q1)))
    );
}

/// A declared difference (B4b): a format typed on the line of a verse with no words stays the
/// line before that verse, where tC4 gives it to the next verse with words; a verse with no
/// words and no line of its own has no format, so the save leaves the line before it as it is.
#[test]
fn a_verse_with_no_words_keeps_the_line_before_it() {
    let b = SectionBuffer::parse("3 c\n\t4 \n5 e", &keys(&["3", "4", "5"]));
    assert_eq!(b.blocks().get(&1), Some(&ParaFormat::Q1), "tC4's reading");
    assert_eq!(
        b.formats(),
        [
            ("3".to_owned(), Some(ParaFormat::Q1)),
            ("4".to_owned(), None)
        ]
    );
    let b = SectionBuffer::parse("1 a", &keys(&["1", "2", "3", "4"]));
    assert_eq!(b.formats(), []);
}

// ---------------------------------------------------------------------------------------------
// Properties.
// ---------------------------------------------------------------------------------------------

fn text_strategy() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        "(\t|\t\t| )?[0-9]{1,2}( [a-z0-9]{1,3}){0,4}",
        "(\t|\t\t)?[0-9]-[0-9]( [a-z0-9]{1,3}){0,3}",
        "[a-z0-9]{1,3}( [a-z0-9]{1,3}){0,4}",
        Just(String::new()),
        Just("  ".to_owned()),
    ];
    proptest::collection::vec(line, 0..8).prop_map(|lines| lines.join("\n"))
}

fn keys_strategy() -> impl Strategy<Value = Vec<String>> {
    prop_oneof![
        Just(keys(&["1", "2", "3"])),
        Just(keys(&["1-2", "3", "4"])),
        Just(keys(&["1"])),
        Just(keys(&["1", "2", "3", "4", "5"])),
    ]
}

proptest! {
    /// Any text, any keys: nothing panics, every word is in exactly one verse in order, and
    /// no word of the text is lost.
    #[test]
    fn reading_never_loses_a_word(text in text_strategy(), k in keys_strategy()) {
        let b = SectionBuffer::parse(&text, &k);
        let from_verses: Vec<String> = b
            .verses()
            .iter()
            .flat_map(|(_, t)| t.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .collect();
        prop_assert_eq!(from_verses, b.words().to_vec());
    }

    /// A drop never changes the words or their separators, and the Type text it writes reads
    /// back as the same words with every pin where it was, the same verses and the same line
    /// before each verse with words: the card holds one reading through a Place session and the
    /// save reads the text it writes, as tC4's Save reads its session. (The text has no line
    /// for a verse with no words, so the save leaves the line before it as it was.)
    #[test]
    fn a_placement_never_changes_the_words(
        text in text_strategy(),
        k in keys_strategy(),
        drops in proptest::collection::vec((0usize..5, 0usize..12), 0..6),
    ) {
        let mut b = SectionBuffer::parse(&text, &k);
        let words = b.words().to_vec();
        for (pin, index) in drops {
            let pin = b.pins().get(pin).cloned().unwrap_or_default();
            // A pin is dropped on a word: a section with no words has none to drop it on.
            if index >= words.len() {
                continue;
            }
            if let Some(next) = b.drop_pin(&pin, index) {
                prop_assert_eq!(next.words(), &words[..]);
                prop_assert_eq!(next.seps(), b.seps());
                b = next;
            }
        }
        let back = SectionBuffer::parse(&b.serialize(), &k);
        prop_assert_eq!(back.words(), &words[..]);
        for pin in b.pins() {
            prop_assert_eq!(back.marker(pin), b.marker(pin), "pin {}", pin);
        }
        prop_assert_eq!(back.verses(), b.verses());
        prop_assert_eq!(lines_before_words(&back), lines_before_words(&b));
    }

    /// The Type text is a fixed point of reading it and writing it again: a card that switches
    /// to Place and back changes the text once, then never.
    #[test]
    fn the_type_text_is_a_fixed_point(text in text_strategy(), k in keys_strategy()) {
        let once = SectionBuffer::parse(&text, &k).serialize();
        let twice = SectionBuffer::parse(&once, &k).serialize();
        prop_assert_eq!(once, twice);
    }

    /// Whatever a verse's body holds, the card opens on it and a save with no change gives
    /// the same body back: the escape of a line that looks like a marker is reversible.
    #[test]
    fn an_untouched_section_gives_every_verse_its_body_back(
        bodies in proptest::collection::vec(
            proptest::collection::vec(
                prop_oneof![
                    "[a-z]{1,4}( [a-z]{1,4}){0,3}",
                    "( )?[0-9]{1,2}( [a-z]{1,4}){0,2}",
                    "[0-9]-[0-9]( [a-z]{1,4}){0,2}",
                ],
                1..4,
            )
            // A body is trimmed (tC4's `body` is), so its first line starts with its text.
            .prop_map(|lines| lines.join("\n").trim().to_owned()),
            1..5,
        ),
    ) {
        let k: Vec<String> = (1..=bodies.len()).map(|n| n.to_string()).collect();
        let verses: Vec<SectionVerse> = bodies
            .iter()
            .zip(&k)
            .map(|(body, key)| SectionVerse {
                key: key.clone(),
                drafted: true,
                body: body.clone(),
                format: None,
            })
            .collect();
        let text = initial_text(&verses, &expand_keys(&k));
        let b = SectionBuffer::parse(&text, &k);
        let got: Vec<(String, String)> = b.verses();
        let want: Vec<(String, String)> = k.iter().cloned().zip(bodies.iter().cloned()).collect();
        prop_assert_eq!(got, want);
    }

    #[test]
    fn indenting_never_goes_past_two_tabs_or_below_none(
        text in "[\t a-z\n]{0,12}",
        caret in 0usize..16,
    ) {
        let (up, _) = indent_line(&text, caret, 1);
        let (twice, _) = indent_line(&up, caret + 1, 1);
        let (thrice, _) = indent_line(&twice, caret + 2, 1);
        prop_assert!(thrice.len() <= text.len() + 2);
        let (down, at) = indent_line(&up, caret + 1, -1);
        prop_assert!(at <= down.encode_utf16().count());
    }
}
