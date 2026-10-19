//! Translate's decisions (unit B4a): the grid's sections, span labels and paragraphs, the source
//! keys of a section, what the verse box shows, a verse field's place, the helps' focus while a
//! card is open, the Comments tab, and the "changed while typing" notice.

use tc_api::buffer::{BufferKey, FieldKey, FieldKind, FieldPlace};
use tc_api::editor::{ChapterText, CommentView, PaneChapter, PaneVerse, VerseRow};
use tc_api::ids::{ProjectId, ProjectRef, RecordRef, RefPart, WallMs};
use tc_api::shown::{PersonLabel, Shown, Who};
use tc_core::BookId;
use tc_ui::app::shell::Editing;
use tc_ui::app::translate::{
    box_text, changed_while_typing, comments_in_focus, editing_focus, parse_verse_place,
    range_span, section_ranges, sections, source_keys_for, target_paragraphs, verse_key,
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

/// #786: the box holds every line of a poem verse, markers included; only the stub of an
/// undrafted verse shows as an empty box.
#[test]
fn verse_editor_shows_every_poem_line() {
    let poem = "Recuérdales\n\\q2 que se sujeten";
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
