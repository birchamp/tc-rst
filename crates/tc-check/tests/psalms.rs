//! The check lists tC4 derives from the v91 helps the app ships (`bundle/en/`), against
//! `fixtures/check/psalms-vectors.json`: what tC4's own `deriveForProject` makes of them
//! (`crates/tc-check/tools/gen-psalms-vectors.cjs` runs it). Psalms is compared check by
//! check, in order, for a whole-book project, partial scopes and an `org` project; every
//! other book of en_tn and en_twl is compared by count and digest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::io::Read;

use common::{eng, fixture, identity, reason};
use serde_json::{Value, json};
use tc_burrito::ScopeRange;
use tc_check::derive::{BookSpec, CheckList, SkipReason, notes_for_book, word_links_for_book};
use tc_core::BookId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{HelpItemId, WordArticleId};
use tc_versification::StandardScheme;

/// The files of a burrito export in `bundle/en/` whose path `keep` accepts, by path.
fn export(zip_name: &str, keep: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let path = common::fixture_path(&format!("../bundle/en/{zip_name}"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        if keep(&name) {
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            out.push((name, text));
        }
    }
    out.sort();
    out
}

/// The book tables of an export, by book.
fn tables(zip_name: &str) -> Vec<(BookId, String)> {
    export(zip_name, |name| {
        name.split('/').nth(1) == Some("ingredients") && name.ends_with(".tsv")
    })
    .into_iter()
    .filter_map(|(name, text)| {
        let code = name.rsplit('/').next()?.strip_suffix(".tsv")?;
        Some((code.parse().ok()?, text))
    })
    .collect()
}

/// The book table of an export, read alone.
fn table(zip_name: &str, book: BookId) -> String {
    let repo = zip_name.split("_v").next().unwrap();
    let path = format!("{repo}/ingredients/{}.tsv", book.code());
    export(zip_name, |name| name == path).remove(0).1
}

/// The articles of the en_tw v91 pin, the gate tC4 applies to tW checks (#643).
fn tw_articles() -> BTreeSet<WordArticleId> {
    let meta = export("en_tw_v91.sb.zip", |name| name == "en_tw/metadata.json");
    parse_burrito_metadata(&meta[0].1).unwrap().word_articles()
}

fn vectors() -> Value {
    serde_json::from_str(&fixture("check/psalms-vectors.json")).unwrap()
}

/// A book's helps, read once, for the tool of a case of the vectors.
enum Rows {
    Notes(Vec<tc_helps::Note>),
    Links(Vec<tc_helps::WordLink>),
}

impl Rows {
    fn read(tool: &str, book: BookId, text: &str) -> Self {
        let scope = TsvScope::Book(book);
        match tool {
            "translationNotes" => Self::Notes(parse_notes(text, scope).unwrap().notes),
            "translationWords" => Self::Links(parse_word_links(text, scope).unwrap()),
            other => panic!("{other}"),
        }
    }

    fn psalms(tool: &str) -> Self {
        let zip = match tool {
            "translationNotes" => "en_tn_v91.sb.zip",
            _ => "en_twl_v91.sb.zip",
        };
        Self::read(tool, BookId::Psalms, &table(zip, BookId::Psalms))
    }

    fn derive(&self, spec: &BookSpec<'_>, articles: &BTreeSet<WordArticleId>) -> CheckList {
        match self {
            Self::Notes(notes) => notes_for_book(notes, spec),
            Self::Links(links) => word_links_for_book(links, Some(articles), spec),
        }
    }
}

#[test]
fn psalms_check_lists_match_tc4_check_for_check() {
    let vectors = vectors();
    let articles = tw_articles();
    let notes = Rows::psalms("translationNotes");
    let words = Rows::psalms("translationWords");
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
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
        let rows = if tool == "translationNotes" {
            &notes
        } else {
            &words
        };
        let list = rows.derive(&spec, &articles);
        let expected = case["items"].as_array().unwrap();
        let full = expected
            .first()
            .is_some_and(|i| i.as_array().unwrap().len() == 6);
        let items: Vec<Value> = list
            .items
            .iter()
            .map(|item| {
                let mut out = identity(item);
                if full {
                    out.push(json!(item.group_id));
                    out.push(json!(item.category.name()));
                }
                Value::Array(out)
            })
            .collect();
        assert_eq!(items.len(), expected.len(), "{label}");
        for (i, (ours, theirs)) in items.iter().zip(expected).enumerate() {
            assert_eq!(ours, theirs, "{label}, check {i}");
        }
        let dropped: Vec<Value> = list
            .dropped
            .iter()
            .map(|d| {
                let mut out = identity(&d.item);
                out.push(json!(reason(&d.reason)));
                Value::Array(out)
            })
            .collect();
        assert_eq!(&Value::Array(dropped), &case["unplaceable"], "{label}");
        assert_eq!(Some(list.mapped), case["mapped"].as_bool(), "{label}");
        // tC4 skips only plain notes and links to articles the pin lacks.
        assert!(
            list.skipped
                .iter()
                .all(|s| matches!(s.reason, SkipReason::PlainNote | SkipReason::ArticleMissing)),
            "{label}: {:?}",
            list.skipped
        );
    }
}

#[test]
fn the_superscriptions_are_checks_on_the_verse_front() {
    let articles = tw_articles();
    let spec = eng(BookId::Psalms);
    let at = |list: &CheckList, verse: &str| -> Vec<String> {
        list.items
            .iter()
            .filter(|i| i.key.target.parts().1 == verse)
            .map(|i| i.key.target.locator())
            .collect()
    };
    let notes = Rows::psalms("translationNotes").derive(&spec, &articles);
    assert_eq!(notes.items.len(), 7969);
    let fronts = at(&notes, "front");
    assert_eq!(fronts.len(), 110);
    assert_eq!(fronts[0], "4:front");
    // Psalm 3's superscription note has no SupportReference: it is shown, not checked.
    assert!(
        notes
            .skipped
            .iter()
            .any(|s| s.source.tsv_text() == "3:front" && s.reason == SkipReason::PlainNote)
    );
    let words = Rows::psalms("translationWords").derive(&spec, &articles);
    assert_eq!(words.items.len(), 7683);
    assert_eq!(at(&words, "front").len(), 155);
    assert!(at(&words, "front").contains(&"3:front".to_owned()));
    // A comma list keeps the verse as the row writes it.
    let sbh4 = notes
        .items
        .iter()
        .find(|i| i.key.check_id.as_str() == "sbh4")
        .unwrap();
    assert_eq!(sbh4.key.target.locator(), "5:1,3,8,12");
    assert_eq!(sbh4.context()["reference"]["verse"], "1,3,8,12");
}

fn digest(lines: &[String]) -> String {
    tc_core::Md5::of(lines.join("\n").as_bytes()).to_hex()
}

/// tC4's `String()` of a reference part as the vectors hold it.
fn part(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[test]
fn every_book_s_check_list_matches_tc4() {
    let vectors = vectors();
    let articles = tw_articles();
    let mut compared = 0;
    for (tool, zip) in [
        ("translationNotes", "en_tn_v91.sb.zip"),
        ("translationWords", "en_twl_v91.sb.zip"),
    ] {
        for (book, text) in tables(zip) {
            let expected = &vectors["books"][tool][book.code()];
            let list = Rows::read(tool, book, &text).derive(&eng(book), &articles);
            let lines: Vec<String> = list
                .items
                .iter()
                .map(|i| {
                    let (chapter, verse) = i.key.target.parts();
                    format!(
                        "{}|{chapter}|{verse}|{}|{}",
                        i.key.check_id, i.key.occurrence, i.group_id
                    )
                })
                .collect();
            let label = format!("{tool} {}", book.code());
            assert_eq!(
                Some(lines.len() as u64),
                expected["checks"].as_u64(),
                "{label}"
            );
            assert_eq!(
                Some(digest(&lines).as_str()),
                expected["md5"].as_str(),
                "{label}"
            );
            // tC4 also derives the rows on every occurrence (Occurrence -1); a check key
            // holds no -1 (STORAGE.md §2.2), so tc-check reports each of them instead.
            let every: Vec<Value> = list
                .skipped
                .iter()
                .filter(|s| s.reason == SkipReason::EveryOccurrence)
                .map(|s| {
                    let HelpItemId::Row(id) = &s.id else {
                        panic!("{label}: {:?}", s.id)
                    };
                    let text = s.source.tsv_text();
                    let (chapter, verse) = text.split_once(':').unwrap();
                    json!([id.as_str(), chapter, verse])
                })
                .collect();
            let tc4_every: Vec<Value> = expected["every"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| json!([part(&e[0]), part(&e[1]), part(&e[2])]))
                .collect();
            assert_eq!(every, tc4_every, "{label}");
            assert!(
                list.skipped.iter().all(|s| matches!(
                    s.reason,
                    SkipReason::PlainNote
                        | SkipReason::ArticleMissing
                        | SkipReason::EveryOccurrence
                )),
                "{label}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 118);
}
