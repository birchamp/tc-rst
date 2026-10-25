//! Translate's decisions (unit B4a): the grid's sections, span labels and paragraphs, the source
//! keys of a section, what the verse box shows, a verse field's place, the helps' focus while a
//! card is open, the Comments tab, and the "changed while typing" notice.

use tc_api::buffer::{BufferKey, FieldKey, FieldKind, FieldPlace};
use tc_api::editor::{
    ChapterText, CommentView, FrameRow, PaneChapter, PaneVerse, RowSource, SourceFrame, VerseRow,
};
use tc_api::ids::{ProjectId, ProjectRef, RecordRef, RefPart, WallMs};
use tc_api::model::section::ParaFormat;
use tc_api::shown::{PersonLabel, Shown, Who};
use tc_core::BookId;
use tc_ui::app::shell::Editing;
use tc_ui::app::translate::{
    SourcePlace, box_text, card_tokens, card_verses, changed_while_typing, comments_in_focus,
    editing_focus, editing_section, parse_verse_place, range_span, section_key, section_place,
    section_ranges, sections, source_keys_for, source_place, target_paragraphs, verse_key,
    verse_place,
};

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| (*k).to_owned()).collect()
}

fn row(key: &str, lead: Option<u8>, before: Option<u8>) -> VerseRow {
    VerseRow {
        key: RefPart::parse(key).unwrap(),
        record: RecordRef::parse(r#"{"kind":"verse"}"#).unwrap(),
        drafted: true,
        text: format!("verse {key}"),
        lead,
        before,
        words: Shown {
            value: format!("verse {key}"),
            token: super::save_kit::tok(1),
            marker: None,
        },
        format: None,
        covered: Vec::new(),
    }
}

fn chapter(rows: Vec<VerseRow>, sections: Vec<u16>) -> ChapterText {
    ChapterText {
        chapter: 1,
        sections,
        verses: rows,
    }
}

fn pane(keys: &[&str], sections: Vec<u16>) -> PaneChapter {
    PaneChapter {
        sections,
        verses: keys
            .iter()
            .map(|k| PaneVerse {
                key: RefPart::parse(k).unwrap(),
                text: String::new(),
                tokens: Vec::new(),
                lead: None,
                before: None,
            })
            .collect(),
    }
}

#[test]
fn sections_split_where_the_source_marks_them_and_keep_every_verse() {
    let text = chapter(
        (1..=7).map(|n| row(&n.to_string(), None, None)).collect(),
        vec![1],
    );
    let ult = pane(&["1", "2", "3", "4", "5", "6", "7"], vec![1, 4, 6]);
    assert_eq!(
        sections(Some(&ult), &text, false),
        [keys(&["1", "2", "3"]), keys(&["4", "5"]), keys(&["6", "7"])]
    );
    // A source that marks nothing: the translation's own sections, else the whole chapter.
    let plain = pane(&["1"], vec![1]);
    let own = chapter(text.verses.clone(), vec![1, 5]);
    assert_eq!(
        sections(Some(&plain), &own, false),
        [keys(&["1", "2", "3", "4"]), keys(&["5", "6", "7"])]
    );
    assert_eq!(sections(None, &text, false).len(), 1);
    // In another numbering, one per verse (#118).
    assert_eq!(sections(Some(&ult), &text, true).len(), 7);
    // Keys before the first start are a section of their own, never dropped; a span counts by
    // its first verse.
    assert_eq!(
        section_ranges(&[3, 5], &keys(&["1", "2", "3-4", "5"])),
        [keys(&["1", "2"]), keys(&["3-4"]), keys(&["5"])]
    );
}

#[test]
fn a_sections_label_is_tc4s_span() {
    assert_eq!(range_span(&keys(&["1", "2", "3"])), "1–3");
    assert_eq!(range_span(&keys(&["9"])), "9");
    assert_eq!(range_span(&keys(&["9-10"])), "9-10");
    assert_eq!(range_span(&keys(&["7-8", "9-11"])), "7–11");
}

#[test]
fn a_sections_source_verses_follow_the_sources_keys() {
    let ult = pane(&["1", "2", "3-4", "5"], vec![1]);
    // A target span the source keeps apart, and a target verse inside a source span.
    assert_eq!(source_keys_for(&keys(&["1-2"]), &ult), keys(&["1", "2"]));
    assert_eq!(source_keys_for(&keys(&["3", "4"]), &ult), keys(&["3-4"]));
    assert_eq!(source_keys_for(&keys(&["6"]), &ult), keys(&["6"]));
}

#[test]
fn target_paragraphs_open_at_a_marker_with_its_poetry_indent() {
    let rows = [
        row("1", None, Some(0)),
        row("2", None, None),
        row("3", None, Some(1)),
        row("4", Some(2), Some(1)),
        row("5", None, Some(3)),
    ];
    let refs: Vec<&VerseRow> = rows.iter().collect();
    let paras: Vec<(Vec<&str>, u8)> = target_paragraphs(&refs)
        .into_iter()
        .map(|(verses, level)| (verses.iter().map(|v| v.key.as_str()).collect(), level))
        .collect();
    assert_eq!(
        paras,
        [
            (vec!["1", "2"], 0),
            (vec!["3"], 1),
            (vec!["4"], 2),
            (vec!["5"], 0)
        ]
    );
}

/// #786, #810: the box holds every line of a poem verse as the server sends it (poem lines as
/// tabs since tC4 rc.2, a first line's own marker as written); only the stub of an undrafted
/// verse shows as an empty box.
#[test]
fn verse_editor_shows_every_poem_line() {
    let poem = "Recuérdales\n\t\tque se sujeten";
    assert_eq!(box_text(poem), poem);
    assert_eq!(box_text("\\q2 que no hablen mal"), "\\q2 que no hablen mal");
    assert_eq!(box_text("___"), "");
    assert_eq!(box_text(" ___ "), "");
}

#[test]
fn a_verse_fields_place_names_its_book_chapter_and_verse() {
    assert_eq!(verse_place(BookId::Titus, 1, "4"), "TIT 1:4");
    assert_eq!(
        parse_verse_place("PSA 119:1-2"),
        Some((BookId::Psalms, 119, "1-2".to_owned()))
    );
    let project = ProjectRef::parse("sample_burrito").unwrap();
    let id = ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap();
    let key = verse_key(&project, &id, BookId::Titus, 1, "2").unwrap();
    assert_eq!(
        key,
        BufferKey {
            project,
            id,
            field: FieldKey {
                kind: FieldKind::Verse,
                place: FieldPlace::parse("TIT 1:2").unwrap(),
            },
        }
    );
    // The helps follow the open card in its chapter only (tC4's `editingFocus`).
    let editing = Editing::Verse(key);
    assert_eq!(
        editing_focus(&editing, BookId::Titus, 1),
        Some(keys(&["2"]))
    );
    assert_eq!(editing_focus(&editing, BookId::Titus, 2), None);
    assert_eq!(editing_focus(&Editing::None, BookId::Titus, 1), None);
}

#[test]
fn the_comments_tab_shows_each_verses_newest_comment_in_focus() {
    let comment = |verse: u16, text: &str, at: i64| CommentView {
        chapter: 1,
        verse,
        text: text.into(),
        at: WallMs(at),
        token: super::save_kit::tok(at.unsigned_abs()),
    };
    let comments = [
        comment(3, "older", 1),
        comment(3, "newer", 2),
        comment(1, "first", 5),
        CommentView {
            chapter: 2,
            ..comment(1, "another chapter", 9)
        },
    ];
    let shown: Vec<&str> = comments_in_focus(&comments, 1, None)
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(shown, ["first", "newer"]);
    let focused = comments_in_focus(&comments, 1, Some(&keys(&["3"])));
    assert_eq!(focused.len(), 1);
    assert_eq!(focused[0].text, "newer");
}

#[test]
fn the_notice_names_who_changed_the_verse() {
    let juan = PersonLabel {
        who: Who::Username("juan".into()),
        computer: None,
    };
    assert_eq!(
        changed_while_typing(Some(&juan)),
        "juan changed this verse while you were typing. Your text will be kept as a second version."
    );
    let mine = PersonLabel {
        who: Who::Mine,
        computer: None,
    };
    for by in [None, Some(&mine)] {
        assert_eq!(
            changed_while_typing(by),
            "This verse was changed while you were typing. Your text will be kept as a second version."
        );
    }
}

/// tC4 #131: in a project of another numbering each project verse shows the source verse its
/// numbering names, under that verse's place and number; a verse in another book or in no place
/// is stated, never shown as another verse's text.
#[test]
fn cross_frame_translate_shows_the_mapped_source_verse() {
    let row = |verse: &str, source: RowSource| FrameRow {
        verse: RefPart::parse(verse).unwrap(),
        source,
    };
    let at = |chapter: u16, verse: &str| RowSource::Verse {
        chapter,
        verse: RefPart::parse(verse).unwrap(),
    };
    let frame = SourceFrame::Mapped {
        rows: vec![
            row("1", at(1, "17")),
            row("2", at(2, "1-2")),
            row(
                "3",
                RowSource::OtherBook {
                    to: "NEH 1:1".into(),
                },
            ),
            row("4", RowSource::Unmapped),
        ],
    };
    let place = |key: &str| source_place(BookId::Jonah, 2, key, &frame);
    assert_eq!(
        place("1"),
        SourcePlace::Verse {
            label: "Jonah 1:17".into(),
            number: "17".into()
        }
    );
    // A fan-out: one project verse carries a source span.
    assert_eq!(
        place("2"),
        SourcePlace::Verse {
            label: "Jonah 2:1-2".into(),
            number: "1-2".into()
        }
    );
    assert_eq!(
        place("3"),
        SourcePlace::OtherBook {
            at: "2:3".into(),
            to: "NEH 1:1".into()
        }
    );
    assert_eq!(place("4"), SourcePlace::Unmapped { at: "2:4".into() });
    assert_eq!(place("9"), SourcePlace::Unmapped { at: "2:9".into() });
    assert_eq!(
        source_place(BookId::Jonah, 2, "1", &SourceFrame::Unknown),
        SourcePlace::Unknown
    );
    assert_eq!(
        source_place(BookId::Jonah, 2, "1", &SourceFrame::Same),
        SourcePlace::Same
    );
}

// ---------------------------------------------------------------------------------------------
// The section card (unit B4b).
// ---------------------------------------------------------------------------------------------

fn section_open(chapter: u16, section: &[&str]) -> Editing {
    let project = ProjectRef::parse("sample_burrito").unwrap();
    let id = ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap();
    let key = section_key(&project, &id, BookId::Titus, chapter, &keys(section)).unwrap();
    Editing::Section {
        key,
        keys: keys(section),
    }
}

#[test]
fn a_section_is_one_place_whatever_spans_it_holds() {
    assert_eq!(
        section_place(BookId::Titus, 2, &keys(&["9", "10"])),
        "TIT 2:9-10"
    );
    assert_eq!(
        section_place(BookId::Titus, 2, &keys(&["9-10"])),
        "TIT 2:9-10",
        "a join does not move the card's buffer"
    );
    assert_eq!(
        section_place(BookId::Titus, 2, &keys(&["11", "12-13"])),
        "TIT 2:11-13"
    );
    assert_eq!(section_place(BookId::Titus, 3, &keys(&["4"])), "TIT 3:4-4");
    let open = section_open(2, &["9", "10"]);
    let Editing::Section { key, .. } = &open else {
        panic!("{open:?}")
    };
    assert_eq!(key.field.kind, FieldKind::Section);
    assert_eq!(key.field.place.as_str(), "TIT 2:9-10");
}

#[test]
fn the_helps_follow_the_whole_open_section_in_its_chapter_only() {
    let open = section_open(2, &["9", "10-11", "12"]);
    assert_eq!(
        editing_focus(&open, BookId::Titus, 2),
        Some(keys(&["9", "10-11", "12"]))
    );
    assert_eq!(editing_focus(&open, BookId::Titus, 1), None);
    assert_eq!(
        editing_section(&open, BookId::Titus, 2),
        Some(&keys(&["9", "10-11", "12"])[..])
    );
    assert_eq!(editing_section(&open, BookId::Genesis, 2), None);
    assert_eq!(editing_section(&Editing::None, BookId::Titus, 2), None);
    assert!(open.is_open(), "an open card keeps the exits off");
}

#[test]
fn a_card_opens_on_its_rows_and_is_based_on_every_verse_it_displays() {
    let token = super::save_kit::tok;
    let mut one = row("9-10", None, None);
    one.words.value = "  Exhorta a los siervos \n".into();
    one.format = Some(ParaFormat::Q1);
    one.covered = vec![token(7)];
    let mut stub = row("11", None, None);
    stub.drafted = false;
    stub.words.value = "___".into();
    stub.words.token = token(8);
    let rows = [&one, &stub];
    let verses = card_verses(&rows);
    assert_eq!(verses[0].key, "9-10");
    assert_eq!(verses[0].body, "Exhorta a los siervos");
    assert_eq!(verses[0].format, Some(ParaFormat::Q1));
    assert!(!verses[1].drafted && verses[1].body.is_empty());
    assert_eq!(
        card_tokens(&rows),
        [token(1), token(7), token(8)],
        "each verse's token, then the verses it covers"
    );
}

/// FIX-HIDE2: a restored section's place names the verses its card was opened over, spans
/// included, and nothing outside its range.
#[test]
fn a_section_place_names_the_verses_its_card_covered() {
    use tc_ui::app::translate::section_keys_of;
    let keys: Vec<String> = ["8", "9", "10-11", "12"]
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
    assert_eq!(section_keys_of("9-11", &keys), ["9", "10-11"]);
    assert_eq!(section_keys_of("12", &keys), ["12"]);
    assert!(section_keys_of("x-2", &keys).is_empty());
}
