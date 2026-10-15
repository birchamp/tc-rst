//! The v91 helps the app ships (`bundle/en/`): every table of en_tn, en_twl and en_tq
//! reads, with the rows tC4's reader returns, and the Psalms superscription rows
//! (`N:front`) read as chapter fronts.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use common::{export_tables, fixture};
use serde_json::Value;
use tc_core::{BookId, Reference};
use tc_helps::metadata::{DcsPin, HelpsSource};
use tc_helps::tsv::{
    NotesLayout, TsvScope, WordLinkStyle, parse_notes, parse_questions, parse_word_links,
    write_notes, write_word_links,
};
use tc_helps::{BibleLocus, BibleRef, CheckReadiness, CommitSha, HelpRef, Helps, Note};

const PSA: TsvScope = TsvScope::Book(BookId::Psalms);

/// What tC4's TSV reader returns for each table: `rows`, and the lines it skips
/// (`fixtures/helps/v91-tsv-rows.json`, made by `crates/tc-check/tools/gen-psalms-vectors.cjs`).
fn tc4_rows() -> Value {
    serde_json::from_str(&fixture("helps/v91-tsv-rows.json")).unwrap()
}

/// The lines tC4's `tsvRows` drops: those whose `trim()` is empty, counted from 1 at the
/// header.
fn blank_lines(text: &str) -> Vec<u64> {
    text.split('\n')
        .enumerate()
        .filter(|(_, line)| line.trim().is_empty())
        .map(|(i, _)| i as u64 + 1)
        .collect()
}

#[test]
fn every_v91_table_reads_and_keeps_every_row_tc4_reads() {
    let expected = tc4_rows();
    for (repo, zip) in [
        ("en_tn", "en_tn_v91.sb.zip"),
        ("en_twl", "en_twl_v91.sb.zip"),
        ("en_tq", "en_tq_v91.sb.zip"),
    ] {
        let tables = export_tables(zip);
        assert_eq!(tables.len(), 59, "{zip}");
        let mut total = 0;
        for (code, text) in &tables {
            let book: BookId = code.parse().unwrap();
            let scope = TsvScope::Book(book);
            let read = match repo {
                "en_tn" => parse_notes(text, scope).map(|n| n.notes.len()),
                "en_twl" => parse_word_links(text, scope).map(|l| l.len()),
                _ => parse_questions(text, scope).map(|q| q.len()),
            };
            let read = read.unwrap_or_else(|e| panic!("{repo} {code}: {e}"));
            let tc4 = &expected["rows"][repo][code];
            assert_eq!(Some(read as u64), tc4["rows"].as_u64(), "{repo} {code}");
            // The only lines either reader skips: the empty line after each file's final
            // newline. Every other line is a row, in both readers.
            let skipped = blank_lines(text);
            let last = text.split('\n').count() as u64;
            assert_eq!(skipped, vec![last], "{repo} {code}");
            let tc4_skipped: Vec<u64> = tc4["skipped"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap())
                .collect();
            assert_eq!(skipped, tc4_skipped, "{repo} {code}");
            total += read;
        }
        let expected_total = match repo {
            "en_tn" => 87_573,
            "en_twl" => 85_764,
            _ => 12_139,
        };
        assert_eq!(total, expected_total, "{repo}");
    }
}

fn front_chapters<'a>(refs: impl Iterator<Item = &'a HelpRef>) -> BTreeMap<u16, usize> {
    let mut out = BTreeMap::new();
    for r in refs {
        if let HelpRef::Bible(BibleRef {
            locus: BibleLocus::ChapterFront(c),
            ..
        }) = r
        {
            *out.entry(*c).or_default() += 1;
        }
    }
    out
}

#[test]
fn psalms_superscription_rows_read_as_chapter_fronts_and_write_back_unchanged() {
    let text = fixture("scale/psa/en_tn/tn_PSA.tsv");
    let notes = parse_notes(&text, PSA).unwrap().notes;
    assert_eq!(notes.len(), 8212);
    let fronts = front_chapters(notes.iter().map(|n| &n.reference));
    assert_eq!(fronts.values().sum::<usize>(), 113);
    assert_eq!(fronts.keys().next(), Some(&3));
    let py7k = notes
        .iter()
        .find(|n| n.reference.tsv_text() == "3:front")
        .unwrap();
    assert_eq!(py7k.reference.to_string(), "PSA 3:front");
    assert!(py7k.read_from.is_none());
    // tC4 checks a superscription note with a SupportReference like any other note.
    let jhz1 = notes
        .iter()
        .find(|n| n.reference.tsv_text() == "4:front")
        .unwrap();
    assert!(jhz1.support_reference.is_some());
    assert_eq!(jhz1.check_readiness(), CheckReadiness::Ready);
    assert_eq!(write_notes(&notes, PSA, NotesLayout::Seven).unwrap(), text);

    for (text, style) in [
        (fixture("scale/psa/en_twl/twl_PSA.tsv"), WordLinkStyle::Rc),
        (
            export_tables("en_twl_v91.sb.zip")
                .into_iter()
                .find(|(code, _)| code == "PSA")
                .unwrap()
                .1,
            WordLinkStyle::ExportPath,
        ),
    ] {
        let links = parse_word_links(&text, PSA).unwrap();
        assert_eq!(links.len(), 7683);
        assert_eq!(
            front_chapters(links.iter().map(|l| &l.reference))
                .values()
                .sum::<usize>(),
            155
        );
        assert_eq!(write_word_links(&links, PSA, style).unwrap(), text);
    }
}

#[test]
fn a_chapter_front_is_neither_a_verse_nor_an_introduction() {
    let tsv = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n\
               3:front\tpy7k\t\t\tלְ\u{2060}דָוִ֑ד\t1\tA note\n\
               3:1\tab12\t\t\tא\t1\tB\n\
               3:intro\tcd34\t\t\t\t0\tC\n";
    let notes = parse_notes(tsv, PSA).unwrap().notes;
    let front = &notes[0].reference;
    assert!(!front.is_introduction());
    let HelpRef::Bible(place) = front else {
        panic!("{front}")
    };
    assert_eq!(place.locus, BibleLocus::ChapterFront(3));
    assert!(!place.overlaps(&"PSA 3:1".parse::<Reference>().unwrap()));
    assert!(!place.overlaps(&"PSA 3:1-8".parse::<Reference>().unwrap()));

    let mut helps = Helps::new();
    let source = HelpsSource::Dcs(DcsPin {
        owner: "unfoldingWord".into(),
        repo: "en_tn".into(),
        tag: Some("v91".into()),
        commit: CommitSha::parse("e586762e330f482a60c52aedd1c7b3a2f155df8a").unwrap(),
    });
    helps.add_notes(&source, notes.clone()).unwrap();
    let ids = |found: Vec<&Note>| -> Vec<String> {
        found.iter().map(|n| format!("{:?}", n.id)).collect()
    };
    let on_front: Vec<&Note> = helps
        .front_notes(BookId::Psalms, 3)
        .map(|f| f.item)
        .collect();
    assert_eq!(ids(on_front.clone()), ids(vec![&notes[0]]));
    assert!(helps.front_notes(BookId::Psalms, 4).next().is_none());
    let on_verse: Vec<&Note> = helps
        .notes_for("PSA 3:1".parse().unwrap())
        .map(|f| f.item)
        .collect();
    assert_eq!(ids(on_verse), ids(vec![&notes[1]]));
    assert_eq!(
        helps
            .introductions(BookId::Psalms, Some(3))
            .map(|f| f.item.reference.tsv_text())
            .collect::<Vec<_>>(),
        ["3:intro"]
    );
}
