//! Rows whose `Reference` tc-helps keeps as written (`1:1a`, `0:front`, `3:front `, `1:0`
//! and other forms no shipped table has), against
//! `fixtures/check/reference-forms-vectors.json`: what tC4's own `deriveForProject` makes
//! of a Psalms tN and TWL table with one row per form
//! (`crates/tc-check/tools/gen-psalms-vectors.cjs` runs it). tC4 reads every Reference
//! cell, so such a row must cost no more than itself, never the book's check list.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::{fixture, identity, reason};
use serde_json::Value;
use tc_burrito::ScopeRange;
use tc_check::derive::{BookSpec, SkipReason, notes_for_book, word_links_for_book};
use tc_core::BookId;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{BibleLocus, HelpItemId, HelpRef};
use tc_versification::StandardScheme;

const PSA: TsvScope = TsvScope::Book(BookId::Psalms);

/// The rows whose cell names no verse a check key can hold: tC4 derives them with an
/// `undefined` or empty part (`intro`, an empty cell, `3:`, `:1`).
const NO_KEY: [&str; 4] = ["f009", "f010", "f011", "f012"];

fn vectors() -> Value {
    serde_json::from_str(&fixture("check/reference-forms-vectors.json")).unwrap()
}

/// True when tC4's identity has a chapter and a verse a check key can hold.
fn keyable(identity: &Value) -> bool {
    [&identity[1], &identity[2]]
        .iter()
        .all(|part| !part.is_null() && part.as_str() != Some(""))
}

fn row_id(id: &HelpItemId) -> &str {
    match id {
        HelpItemId::Row(id) => id.as_str(),
        HelpItemId::Aquifer(id) => panic!("{id:?}"),
    }
}

#[test]
fn every_reference_form_derives_the_checks_tc4_derives() {
    let vectors = vectors();
    let notes = parse_notes(vectors["tables"]["translationNotes"].as_str().unwrap(), PSA)
        .unwrap()
        .notes;
    let links =
        parse_word_links(vectors["tables"]["translationWords"].as_str().unwrap(), PSA).unwrap();
    assert_eq!((notes.len(), links.len()), (26, 26));
    // Eight rows are in forms tc-helps gives a place to (`3:01`, `03:front`, `3:front`,
    // `front:intro`, `3:intro`, `3:1`, `3:1-200`, `151:1`); the other 18 are kept as written.
    let as_written = notes
        .iter()
        .filter(|n| matches!(&n.reference, HelpRef::Bible(r) if matches!(r.locus, BibleLocus::AsWritten(_))))
        .count();
    assert_eq!(as_written, 18);

    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let tool = case["tool"].as_str().unwrap();
        let to = match case["to"].as_str().unwrap() {
            "eng" => StandardScheme::Eng,
            "org" => StandardScheme::Org,
            other => panic!("{other}"),
        };
        let scope: Vec<ScopeRange> = case["scope"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().parse().unwrap())
            .collect();
        let spec = BookSpec {
            book: BookId::Psalms,
            resource_frame: StandardScheme::Eng.scheme(),
            project_frame: Some(to.scheme()),
            scope: &scope,
        };
        let label = format!("{tool} eng>{} {:?}", to.name(), case["scope"]);
        let list = match tool {
            "translationNotes" => notes_for_book(&notes, &spec),
            _ => word_links_for_book(&links, None::<&BTreeSet<_>>, &spec),
        };

        let items: Vec<Value> = list
            .items
            .iter()
            .map(|i| Value::from(identity(i)))
            .collect();
        let expected: Vec<Value> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| keyable(i))
            .cloned()
            .collect();
        assert_eq!(items, expected, "{label}");

        let dropped: Vec<Value> = list
            .dropped
            .iter()
            .map(|d| {
                let mut out = identity(&d.item);
                out.push(reason(&d.reason).into());
                Value::from(out)
            })
            .collect();
        let expected: Vec<Value> = case["unplaceable"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| keyable(i))
            .cloned()
            .collect();
        assert_eq!(dropped, expected, "{label}");
        assert_eq!(Some(list.mapped), case["mapped"].as_bool(), "{label}");

        // The rows tC4 derives with no verse a key can hold are reported, one by one,
        // and nothing else is skipped.
        let skipped: Vec<&str> = list
            .skipped
            .iter()
            .map(|s| {
                assert!(
                    matches!(s.reason, SkipReason::InvalidReference(_)),
                    "{label}: {s:?}"
                );
                row_id(&s.id)
            })
            .collect();
        assert_eq!(skipped, NO_KEY, "{label}");
    }
}

/// The rows [`NO_KEY`] names are exactly those tC4 derives with a part a check key cannot
/// hold, so the comparison above leaves out nothing else.
#[test]
fn only_the_rows_with_no_verse_part_are_left_out() {
    let vectors = vectors();
    let whole_eng = vectors["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["tool"] == "translationNotes"
                && c["to"] == "eng"
                && c["scope"] == Value::Array(vec![])
        })
        .unwrap();
    let left_out: Vec<&str> = whole_eng["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| !keyable(i))
        .map(|i| i[0].as_str().unwrap())
        .collect();
    assert_eq!(left_out, NO_KEY);
}
