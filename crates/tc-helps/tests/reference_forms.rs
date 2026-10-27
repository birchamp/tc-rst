//! A Bible table's `Reference` in a form this crate gives no place to (`1:1a`, `0:front`,
//! `3:front `, `1:0`, an empty cell) is read and kept as written, as tC4 reads every
//! Reference cell. A CR at the end of a cell is not part of it, as in tC4's `tsvRows`. The
//! tables and tC4's rows come from
//! `fixtures/check/reference-forms-vectors.json`, which
//! `crates/tc-check/tools/gen-psalms-vectors.cjs` makes with tC4's own readers.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::fixture;
use serde_json::{Value, json};
use tc_core::BookId;
use tc_helps::metadata::{DcsPin, HelpsSource};
use tc_helps::tsv::{
    NotesLayout, TsvScope, WordLinkStyle, parse_notes, parse_questions, parse_word_links,
    write_notes, write_questions, write_word_links,
};
use tc_helps::{BibleLocus, GitOid, HelpItemId, HelpRef, Helps, Note, Occurrence, WrittenRef};

const PSA: TsvScope = TsvScope::Book(BookId::Psalms);

fn vectors() -> Value {
    serde_json::from_str(&fixture("check/reference-forms-vectors.json")).unwrap()
}

fn table(tool: &str) -> String {
    vectors()["tables"][tool].as_str().unwrap().to_owned()
}

/// tC4's `refPart`: digits become a number, anything else stays text.
fn ref_part(part: Option<&str>) -> Value {
    match part {
        None => Value::Null,
        Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            json!(p.parse::<u64>().unwrap())
        }
        Some(p) => json!(p),
    }
}

/// A row as tC4's reader returns it, `[checkId, chapter, verse, occurrence]`, from the
/// `Reference` cell the row writes back.
fn tc4_row(id: &HelpItemId, reference: &HelpRef, occurrence: Occurrence) -> Value {
    let HelpItemId::Row(id) = id else {
        panic!("{id:?}")
    };
    let cell = reference.tsv_text();
    let mut parts = cell.split(':');
    let occurrence = match occurrence {
        Occurrence::Nth(n) => i64::from(n.get()),
        Occurrence::NotTied | Occurrence::Unrecorded => 0,
        Occurrence::All => -1,
    };
    json!([
        id.as_str(),
        ref_part(parts.next()),
        ref_part(parts.next()),
        occurrence
    ])
}

fn locus(reference: &HelpRef) -> &BibleLocus {
    match reference {
        HelpRef::Bible(r) => &r.locus,
        HelpRef::Obs(r) => panic!("{r}"),
    }
}

#[test]
fn every_reference_form_tc4_reads_is_read_and_written_back() {
    let vectors = vectors();
    let rows = |tool: &str| vectors["rows"][tool].as_array().unwrap().clone();

    let text = table("translationNotes");
    let notes = parse_notes(&text, PSA).unwrap().notes;
    let ours: Vec<Value> = notes
        .iter()
        .map(|n| tc4_row(&n.id, &n.reference, n.occurrence))
        .collect();
    assert_eq!(ours, rows("translationNotes"));
    assert_eq!(write_notes(&notes, PSA, NotesLayout::Seven).unwrap(), text);

    let text = table("translationWords");
    let links = parse_word_links(&text, PSA).unwrap();
    let ours: Vec<Value> = links
        .iter()
        .map(|l| tc4_row(&l.id, &l.reference, l.occurrence))
        .collect();
    assert_eq!(ours, rows("translationWords"));
    assert_eq!(
        write_word_links(&links, PSA, WordLinkStyle::Rc).unwrap(),
        text
    );

    let text = table("translationQuestions");
    let questions = parse_questions(&text, PSA).unwrap();
    let ours: Vec<Value> = questions
        .iter()
        .map(|q| tc4_row(&q.id, &q.reference, q.occurrence))
        .collect();
    assert_eq!(ours, rows("translationQuestions"));
    assert_eq!(write_questions(&questions, PSA).unwrap(), text);

    // The forms this crate knows keep their own places; only the others are as written.
    let place = |cell: &str| -> &BibleLocus {
        let cell_of = |n: &Note| {
            n.read_from
                .as_ref()
                .and_then(|r| r.get("Reference"))
                .map_or_else(|| n.reference.tsv_text(), str::to_owned)
        };
        locus(&notes.iter().find(|n| cell_of(n) == cell).unwrap().reference)
    };
    assert_eq!(place("03:front"), &BibleLocus::ChapterFront(3));
    assert_eq!(place("3:intro"), &BibleLocus::ChapterIntro(3));
    assert!(matches!(place("3:01"), BibleLocus::Verses(_)));
    for cell in [
        "1:1a", "0:front", "3:front ", "3:Front", "1:0", "", "0:intro",
    ] {
        assert_eq!(
            place(cell),
            &BibleLocus::AsWritten(WrittenRef::new(cell).unwrap()),
            "{cell:?}"
        );
    }

    // A CR before a tab ends the cell's text, in every cell but the last.
    let row = |id: &str| {
        notes
            .iter()
            .find(|n| matches!(&n.id, HelpItemId::Row(r) if r.as_str() == id))
            .unwrap()
    };
    assert_eq!(row("f026").reference.tsv_text(), "3:1");
    assert_eq!(locus(&row("f027").reference), &BibleLocus::ChapterFront(3));
    let every_cell = row("f028");
    assert_eq!(every_cell.reference.tsv_text(), "3:2");
    assert!(every_cell.tags.is_empty());
    assert_eq!(
        every_cell.support_reference.as_ref().and_then(|s| s.slug()),
        Some("figs-metaphor")
    );
    assert_eq!(every_cell.quote, row("f000").quote);
    assert_eq!(every_cell.occurrence, Occurrence::nth(1).unwrap());
    assert_eq!(every_cell.text, "A note");
}

#[test]
fn a_reference_kept_as_written_is_on_no_verse_and_listed_with_its_chapter() {
    let notes = parse_notes(&table("translationNotes"), PSA).unwrap().notes;
    let links = parse_word_links(&table("translationWords"), PSA).unwrap();
    let questions = parse_questions(&table("translationQuestions"), PSA).unwrap();
    let mut helps = Helps::new();
    let pin = |repo: &str| {
        HelpsSource::Dcs(DcsPin {
            owner: "unfoldingWord".into(),
            repo: repo.into(),
            tag: Some("v91".into()),
            commit: GitOid::parse("e586762e330f482a60c52aedd1c7b3a2f155df8a").unwrap(),
        })
    };
    helps.add_notes(&pin("en_tn"), notes.clone()).unwrap();
    helps.add_word_links(&pin("en_twl"), links).unwrap();
    helps.add_questions(&pin("en_tq"), questions).unwrap();
    let cells = |found: Vec<&HelpRef>| -> Vec<String> {
        found.into_iter().map(HelpRef::tsv_text).collect()
    };

    // tC4 files `1:1a` under chapter 1 (`refPart`), but no verse of it carries `1a`
    // (`helpsFocus.js` `keyCarries`).
    let chapter_1 = ["1:1a", "1:0", "1:1:1"];
    let found: Vec<&Note> = helps
        .as_written_notes(BookId::Psalms, 1)
        .map(|f| f.item)
        .collect();
    assert_eq!(
        cells(found.iter().map(|n| &n.reference).collect()),
        chapter_1
    );
    assert_eq!(
        cells(
            helps
                .as_written_word_links(BookId::Psalms, 1)
                .map(|f| &f.item.reference)
                .collect()
        ),
        chapter_1
    );
    assert_eq!(
        cells(
            helps
                .as_written_questions(BookId::Psalms, 1)
                .map(|f| &f.item.reference)
                .collect()
        ),
        chapter_1
    );
    assert!(
        helps
            .notes_for("PSA 1:1".parse().unwrap())
            .all(|f| !matches!(locus(&f.item.reference), BibleLocus::AsWritten(_)))
    );
    // ` 3:1` names no chapter tC4 can number; `front:front` and `intro` none at all.
    assert_eq!(
        cells(
            helps
                .as_written_notes(BookId::Psalms, 3)
                .map(|f| &f.item.reference)
                .collect()
        ),
        [
            "3:front ",
            "3:Front",
            "3:1,front",
            "3:2-1",
            "3:",
            "3:1-2:4",
            "3:1\u{2013}3",
            "3:\u{663}"
        ]
    );
    // tC4 files `0:front` under chapter 0, which no chapter of a book is.
    assert_eq!(
        cells(
            helps
                .as_written_notes(BookId::Psalms, 0)
                .map(|f| &f.item.reference)
                .collect()
        ),
        ["0:front", "0:intro"]
    );
    // The front of chapter 3 is still only `3:front`, `03:front` and `3:front` and a CR.
    assert_eq!(
        cells(
            helps
                .front_notes(BookId::Psalms, 3)
                .map(|f| &f.item.reference)
                .collect()
        ),
        ["3:front", "3:front", "3:front"]
    );
}

#[test]
fn as_written_holds_only_what_no_other_place_holds() {
    for known in [
        "1:1",
        "3:front",
        "03:front",
        "front:intro",
        "2:intro",
        "1:3,5-6",
    ] {
        assert_eq!(WrittenRef::new(known), None, "{known:?}");
    }
    for cell_breaking in ["1:1\t", "1:\n1"] {
        assert_eq!(WrittenRef::new(cell_breaking), None, "{cell_breaking:?}");
    }
    let odd = WrittenRef::new("1:1a").unwrap();
    assert_eq!(odd.as_str(), "1:1a");
    assert_eq!(odd.chapter(), Some(1));
    assert_eq!(WrittenRef::new("front:front").unwrap().chapter(), None);
}

/// The legacy nine-column table names a place with two cells. A pair in a form this crate
/// gives no place to is kept as written and writes back as the same two cells.
#[test]
fn the_legacy_table_keeps_a_chapter_and_verse_as_written() {
    let legacy = fixture("helps/tn-tsv9/en_tn_57-TIT.tsv");
    let tit = TsvScope::Book(BookId::Titus);
    for (chapter, verse) in [("one", "1"), ("1", "x"), ("1:2", "3"), ("1", "")] {
        let text = legacy
            .split('\n')
            .enumerate()
            .map(|(i, line)| {
                if i == 4 {
                    let mut cells: Vec<&str> = line.split('\t').collect();
                    cells[1] = chapter;
                    cells[2] = verse;
                    cells.join("\t")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = parse_notes(&text, tit).unwrap();
        let written = format!("{chapter}:{verse}");
        assert_eq!(
            locus(&parsed.notes[3].reference),
            &BibleLocus::AsWritten(WrittenRef::new(&written).unwrap()),
            "{written:?}"
        );
        assert_eq!(
            write_notes(&parsed.notes, tit, NotesLayout::LegacyNine).unwrap(),
            text,
            "{written:?}"
        );
    }
}

/// tC4's `tsvRows` drops the lines JavaScript's `trim()` leaves empty, a lone U+FEFF among
/// them, before it takes the first line left as the header. Such a table is read, not
/// refused.
#[test]
fn the_lines_tc4_drops_as_blank_are_skipped_before_the_header_too() {
    let vectors = vectors();
    let blank = &vectors["blankLines"];
    let links = parse_word_links(blank["table"].as_str().unwrap(), PSA).unwrap();
    let ours: Vec<Value> = links
        .iter()
        .map(|l| tc4_row(&l.id, &l.reference, l.occurrence))
        .collect();
    assert_eq!(&ours, blank["rows"].as_array().unwrap());

    // The notes reader finds the legacy table's header past them too.
    let legacy = fixture("helps/tn-tsv9/en_tn_57-TIT.tsv");
    let tit = TsvScope::Book(BookId::Titus);
    assert_eq!(
        parse_notes(&format!("\n\u{feff}\n{legacy}"), tit).unwrap(),
        parse_notes(&legacy, tit).unwrap()
    );
}
