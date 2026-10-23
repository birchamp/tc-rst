//! The open overlay's and Understand's decisions (unit B3): tC4's `OpenProgressView` timer
//! and stages, the units, sections and paragraphs of a passage, the help focus, the comment
//! box's selection rule, and the note preview.

use tc_api::editor::{CommentView, FrameRow, PaneChapter, PaneVerse, RowSource};
use tc_api::helps::{LitVerse, PaneFocus, Slot};
use tc_api::ids::{RefPart, WallMs};
use tc_api::jobs::{OpenPhase, Phase, Progress};
use tc_api::model::{ArticleBlock, ArticleSpan, BlockKind};
use tc_core::BookId;
use tc_ui::app::understand::{
    HelpFocus, OpenStage, Opening, SHOW_THRESHOLD_MS, UnitMode, UnitNotice, clamp_helps,
    drag_width, frame_units, in_focus, key_carries, paragraphs, preview_blocks, ref_label,
    shown_comment, simplified_label, slot_state, units,
};

fn progress(phase: OpenPhase, done: u64, total: Option<u64>) -> Progress {
    Progress {
        phase: Phase::Open(phase),
        done,
        total,
        item: None,
    }
}

#[test]
fn the_overlay_waits_300_ms_from_the_action_with_or_without_a_report() {
    let mut opening = Opening::new(1_000.0);
    assert_eq!(opening.wait(1_000.0), SHOW_THRESHOLD_MS);
    assert_eq!(opening.wait(1_200.0), 100.0);
    assert_eq!(opening.wait(1_300.0), 0.0, "shown at 300 ms with no event");
    // A report changes what it says, never when it shows.
    opening.report(&progress(OpenPhase::ReadingChanges, 10, Some(40)));
    assert_eq!(opening.wait(1_100.0), 200.0);
}

#[test]
fn the_five_phases_show_as_tc4s_three_stages() {
    let mut opening = Opening::new(0.0);
    assert_eq!(
        opening.stage,
        OpenStage::Journal,
        "tC4 starts in the journal stage"
    );
    assert_eq!(opening.percent(), None, "indeterminate until a count");
    for (phase, stage) in [
        (OpenPhase::FinishingWork, OpenStage::State),
        (OpenPhase::ReadingChanges, OpenStage::Journal),
        (OpenPhase::Folding, OpenStage::Journal),
        (OpenPhase::CheckingFiles, OpenStage::State),
        (OpenPhase::WritingFiles, OpenStage::Prepare),
    ] {
        opening.report(&progress(phase, 3, Some(7)));
        assert_eq!(opening.stage, stage, "{phase:?}");
        assert_eq!(
            opening.percent().is_some(),
            phase == OpenPhase::ReadingChanges,
            "{phase:?}: only the reading has a bar with a count"
        );
    }
    opening.report(&progress(OpenPhase::ReadingChanges, 199, Some(200)));
    assert_eq!(
        opening.percent(),
        Some(99),
        "floored: 100 means every file is read"
    );
    assert!(
        opening.caption().ends_with("199 of 200 files"),
        "{}",
        opening.caption()
    );
    opening.preparing();
    assert_eq!(
        (opening.stage, opening.percent()),
        (OpenStage::Prepare, None)
    );
    assert_eq!(opening.stage.as_str(), "prepare");
}

#[test]
fn a_verse_key_carries_the_verses_it_overlaps() {
    assert!(key_carries("4-5", "5"));
    assert!(key_carries("4", "3-4"));
    assert!(!key_carries("4", "5"));
    assert!(!key_carries("intro", "1"));
    assert!(in_focus("anything", None));
    let focus = vec!["1".to_owned(), "2".to_owned(), "3".to_owned()];
    assert!(
        in_focus("intro", Some(&focus)),
        "a chapter introduction goes with verse 1"
    );
    assert!(in_focus("2-4", Some(&focus)));
    assert!(!in_focus("4", Some(&focus)));
}

fn verse(key: &str, lead: Option<u8>, before: Option<u8>) -> PaneVerse {
    PaneVerse {
        key: RefPart::parse(key).unwrap(),
        text: format!("text {key}"),
        tokens: vec![format!("text {key}")],
        lead,
        before,
    }
}

fn titus_1() -> PaneChapter {
    PaneChapter {
        sections: vec![1, 4, 6],
        verses: vec![
            verse("1", None, None),
            verse("2", None, None),
            verse("3", None, None),
            verse("4", None, Some(0)),
            verse("5", None, Some(0)),
            verse("6", Some(1), None),
            verse("7-8", None, Some(2)),
        ],
    }
}

#[test]
fn units_follow_the_sections_or_the_verses() {
    let pane = titus_1();
    let sections = units(UnitMode::Section, BookId::Titus, 1, Some(&pane));
    let shown: Vec<(&str, &str)> = sections
        .iter()
        .map(|u| (u.key.as_str(), u.label.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("s1", "Titus 1:1–3"),
            ("s4", "Titus 1:4–5"),
            ("s6", "Titus 1:6–8")
        ]
    );
    let verses = units(UnitMode::Verse, BookId::Titus, 1, Some(&pane));
    assert_eq!(verses.len(), 7);
    assert_eq!(verses[6].label, "Titus 1:7–8", "a bridge's own label");
    assert_eq!(verses[0].key, "v1");
    let whole = units(
        UnitMode::Section,
        BookId::Titus,
        2,
        Some(&PaneChapter {
            sections: vec![],
            ..titus_1()
        }),
    );
    assert_eq!(whole.len(), 1);
    assert_eq!(
        (whole[0].key.as_str(), whole[0].label.as_str()),
        ("whole", "Titus 2")
    );
    assert!(units(UnitMode::Section, BookId::Titus, 1, None).is_empty());
    assert_eq!(
        ref_label(BookId::Jonah, 1, &["17-18".to_owned()]),
        "Jonah 1:17–18"
    );
}

#[test]
fn a_paragraph_mark_opens_a_paragraph_with_its_level() {
    let pane = titus_1();
    let sections = units(UnitMode::Section, BookId::Titus, 1, Some(&pane));
    let shape = |i: usize| -> Vec<(Vec<String>, u8)> {
        paragraphs(&sections[i], &pane)
            .into_iter()
            .map(|(vs, level)| {
                (
                    vs.iter().map(|v| v.key.as_str().to_owned()).collect(),
                    level,
                )
            })
            .collect()
    };
    assert_eq!(
        shape(0),
        vec![(vec!["1".into(), "2".into(), "3".into()], 0)]
    );
    assert_eq!(shape(1), vec![(vec!["4".into()], 0), (vec!["5".into()], 0)]);
    // A unit's first paragraph takes only its own leading marker's level; a later one the
    // marker before it.
    assert_eq!(
        shape(2),
        vec![(vec!["6".into()], 1), (vec!["7-8".into()], 2)]
    );
}

fn comment(chapter: u16, verse: u16, text: &str, at: i64) -> CommentView {
    CommentView {
        chapter,
        verse,
        text: text.into(),
        at: WallMs(at),
        token: super::save_kit::tok(at.unsigned_abs()),
    }
}

#[test]
fn the_box_shows_the_head_comment_or_the_only_one_and_counts_the_rest() {
    let pane = titus_1();
    let sections = units(UnitMode::Section, BookId::Titus, 1, Some(&pane));
    let unit = &sections[0];
    assert_eq!(shown_comment(&[], 1, unit).text, "");
    // One comment in the unit, not at its head: shown, and saved on its own verse.
    let only = shown_comment(&[comment(1, 2, "only", 1)], 1, unit);
    assert_eq!(
        (only.text.as_str(), only.target.as_str(), only.hidden),
        ("only", "2", 0)
    );
    // A teammate's comment on verse 3 beside one at the head: the head's shows, one is
    // counted, and none is hidden away.
    let two = shown_comment(
        &[comment(1, 1, "head", 1), comment(1, 3, "mate", 2)],
        1,
        unit,
    );
    assert_eq!((two.text.as_str(), two.hidden, two.any), ("head", 1, true));
    // Several comments elsewhere and none at the head: the box shows none.
    let none = shown_comment(&[comment(1, 2, "a", 1), comment(1, 3, "b", 2)], 1, unit);
    assert_eq!(
        (none.text.as_str(), none.hidden, none.target.as_str()),
        ("", 2, "1")
    );
    // A verse with two comments shows its newest; another chapter's are not the unit's.
    let newest = shown_comment(
        &[
            comment(1, 1, "new", 9),
            comment(1, 1, "old", 3),
            comment(2, 1, "other", 1),
        ],
        1,
        unit,
    );
    assert_eq!((newest.text.as_str(), newest.hidden), ("new", 0));
    assert_eq!(
        newest.token,
        Some(super::save_kit::tok(9)),
        "the editable token belongs to exactly the text the box selected"
    );
}

/// tC4 #118 and #121: in another numbering each project verse is a unit labelled with the
/// source verse it shows (`rsc` Jonah 2:1 is the sources' 1:17); one the sources have in
/// another book, or nowhere, is a statement and never this book's text; the comment box keeps
/// the project's verse.
#[test]
fn cross_book_verse_shows_the_statement_and_keys_comments_to_the_project_verse() {
    let part = |s: &str| RefPart::parse(s).unwrap();
    let rows = [
        FrameRow {
            verse: part("1"),
            source: RowSource::Verse {
                chapter: 1,
                verse: part("17"),
            },
        },
        FrameRow {
            verse: part("2"),
            source: RowSource::OtherBook {
                to: "NEH 1:1".into(),
            },
        },
        FrameRow {
            verse: part("3"),
            source: RowSource::Unmapped,
        },
    ];
    let units = frame_units(BookId::Jonah, 2, &rows);
    let keys: Vec<&str> = units.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, ["m2:1", "x2:2", "u2:3"]);
    assert_eq!(units[0].label, "Jonah 1:17");
    assert_eq!(units[0].number.as_deref(), Some("17"));
    assert_eq!(units[0].notice, None);
    assert_eq!(
        units[1].notice,
        Some(UnitNotice::OtherBook {
            at: "2:2".into(),
            to: "NEH 1:1".into()
        })
    );
    assert_eq!(
        units[2].notice,
        Some(UnitNotice::Unmapped { at: "2:3".into() })
    );
    let shown = shown_comment(
        &[
            comment(2, 1, "mine", 1),
            comment(1, 17, "the source's number", 2),
        ],
        2,
        &units[0],
    );
    assert_eq!((shown.text.as_str(), shown.target.as_str()), ("mine", "1"));
}

fn block(text: &str) -> ArticleBlock {
    ArticleBlock::new(
        BlockKind::P,
        0,
        vec![ArticleSpan {
            text: text.into(),
            bold: false,
            italic: false,
        }],
    )
}

#[test]
fn a_long_note_is_cut_between_words_with_an_ellipsis() {
    let blocks = vec![block("one two three"), block("four five six")];
    let cut = preview_blocks(&blocks, 18);
    assert_eq!(cut.len(), 2);
    assert_eq!(cut[1].text(), "four…", "cut on a word boundary");
    // A budget that cannot hold the next block's first word stops before it, with "…".
    let before = preview_blocks(&[block("one two"), block("seventeen")], 9);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].text(), "one two…");
    assert_eq!(
        preview_blocks(&blocks, 400),
        blocks,
        "a short note is whole"
    );
}

#[test]
fn the_focus_lights_only_a_verse_that_carries_the_help() {
    let focus = HelpFocus {
        id: "abcd".into(),
        verse: "1".into(),
        panes: vec![PaneFocus {
            pane: "ult".into(),
            title: Some("of God".into()),
            lit: vec![LitVerse {
                verse: RefPart::parse("1").unwrap(),
                tokens: vec![6, 8],
            }],
        }],
    };
    assert_eq!(focus.lit("ult", "1"), vec![6, 8]);
    assert!(focus.lit("ult", "2").is_empty());
    assert!(focus.lit("ust", "1").is_empty());
    assert_eq!(
        HelpFocus::title(&focus.panes, "ult").as_deref(),
        Some("of God")
    );
}

#[test]
fn the_simplified_tab_is_named_after_its_pin() {
    assert_eq!(simplified_label(Some("en_ust")), "UST");
    assert_eq!(simplified_label(Some("es-419_tps")), "TPS");
    assert_eq!(simplified_label(None), "UST");
    assert_eq!(slot_state::<()>(&Slot::Fetch).unwrap().0, "fetch");
    assert_eq!(slot_state::<()>(&Slot::OldTwl).unwrap().0, "old-twl");
}

#[test]
fn helps_panel_width_follows_the_divider_drag() {
    // tC4 #602: the panel grows toward the editing pane and stays within 280 to 720 px.
    assert_eq!(drag_width(500.0, 360.0, false, 450.0), 410.0);
    assert_eq!(drag_width(500.0, 360.0, true, 450.0), 310.0);
    assert_eq!(drag_width(500.0, 360.0, false, -900.0), 720.0);
    assert_eq!(clamp_helps(100.4), 280.0);
}
