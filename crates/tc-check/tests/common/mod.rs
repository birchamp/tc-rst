// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(dead_code, clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::Value;
use tc_check::carry::StoredDecision;
use tc_check::derive::{BookSpec, CheckList};
use tc_check::{
    CheckId, CheckKey, CheckTarget, Coverage, Decision, Judgement, Pin, RefPart, RepoPath,
    ResourceRef, Selection, TextMd5, ToolId, UserStatus,
};
use tc_core::{BookId, VerseSpan};
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{CommitSha, Note, WordLink};
use tc_usfm::Book;
use tc_versification::StandardScheme;

pub fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

pub fn fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A Door43 resource fixture from tC4: `resource("en_tn@v86/TIT.tsv")`.
pub fn resource(rel: &str) -> String {
    fixture(&format!("tc4/test-fixtures/resources/{rel}"))
}

pub fn notes(rel: &str, book: BookId) -> Vec<Note> {
    parse_notes(&resource(rel), TsvScope::Book(book))
        .unwrap()
        .notes
}

pub fn obs_notes(rel: &str) -> Vec<Note> {
    parse_notes(&resource(rel), TsvScope::Obs).unwrap().notes
}

pub fn word_links(rel: &str, book: BookId) -> Vec<WordLink> {
    parse_word_links(&resource(rel), TsvScope::Book(book)).unwrap()
}

pub fn obs_word_links(rel: &str) -> Vec<WordLink> {
    parse_word_links(&resource(rel), TsvScope::Obs).unwrap()
}

/// A whole-book list in `eng`, the frame of the unfoldingWord helps.
pub fn eng(book: BookId) -> BookSpec<'static> {
    BookSpec {
        book,
        resource_frame: StandardScheme::Eng.scheme(),
        project_frame: Some(StandardScheme::Eng.scheme()),
        scope: &[],
    }
}

pub fn checks_of(list: &CheckList) -> Vec<String> {
    list.items
        .iter()
        .map(|i| i.key.check_id.to_string())
        .collect()
}

/// tC4's test helper: a deterministic fake commit per (repo, version) string.
pub fn sha40(s: &str) -> CommitSha {
    let mut h: u32 = 5381;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(33) ^ u32::from(c);
    }
    CommitSha::parse(&format!("{h:08x}").repeat(5)).unwrap()
}

pub fn resource_ref(repo: &str, sha: CommitSha) -> ResourceRef {
    ResourceRef {
        repo_path: RepoPath::parse(&format!("git.door43.org/{repo}")).unwrap(),
        sha,
    }
}

/// A pin as tC4's tests write one: `pin("unfoldingWord/en_tn", "v86", &["TIT"])`. An
/// empty book list records no coverage.
pub fn pin(repo: &str, version: &str, books: &[&str]) -> Pin {
    Pin {
        resource: resource_ref(repo, sha40(&format!("{repo}@{version}"))),
        version: Some(version.to_owned()),
        coverage: Coverage::from_codes(books.iter().copied()),
    }
}

// ---- the sample project ------------------------------------------------------------

pub fn sample(rel: &str) -> String {
    fixture(&format!("tc4/sample-burrito/ingredients/{rel}"))
}

/// The plain text of a verse of a book.
pub fn verse_text(usfm: &str, chapter: u16, verse: u16) -> String {
    Book::parse(usfm)
        .verse(chapter, VerseSpan::single(verse))
        .unwrap()
        .plain_text()
}

/// A tC4 decision as an import would read it, stamped with the md5 of `text`.
pub fn decision_of(json: &Value, resource: &ResourceRef, text: &str) -> Decision {
    let selections = json["selections"].as_array().map(|list| {
        list.iter()
            .map(|s| Selection {
                text: s["text"].as_str().unwrap().into(),
                occurrence: u32::try_from(s["occurrence"].as_u64().unwrap()).unwrap(),
                occurrences: u32::try_from(s["occurrences"].as_u64().unwrap()).unwrap(),
            })
            .collect()
    });
    Decision {
        judgement: Judgement {
            selections,
            nothing_to_select: json["nothingToSelect"] == true,
            status: match json["status"].as_str() {
                Some("valid") => Some(UserStatus::Valid),
                Some("invalid") => Some(UserStatus::Invalid),
                Some("todo") => Some(UserStatus::Todo),
                _ => None,
            },
            verse_text_md5: TextMd5::of(text),
        },
        comment: json["comments"].as_str().map(str::to_owned),
        bookmarked: json["reminders"] == true,
        resource: resource.clone(),
        imported_invalid: json["invalidated"] == true,
    }
}

/// The resource a tC4 decisions file records.
pub fn file_resource(file: &Value) -> ResourceRef {
    Pin::from_json(&file["resource"]).unwrap().resource
}

/// The decisions of a tC4 Bible decisions file, keyed as the journal keys them, each
/// stamped with the md5 of its verse in `usfm`.
pub fn stored_decisions(file: &Value, book: BookId, usfm: &str) -> Vec<StoredDecision> {
    let tool = ToolId::parse(file["tool"].as_str().unwrap()).unwrap();
    file["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|json| {
            let context = &json["contextId"];
            let part = |v: &Value| match v {
                Value::Number(n) => RefPart::number(u16::try_from(n.as_u64().unwrap()).unwrap()),
                other => RefPart::new(other.as_str().unwrap()).unwrap(),
            };
            let chapter = part(&context["reference"]["chapter"]);
            let verse = part(&context["reference"]["verse"]);
            let text = verse_text(
                usfm,
                chapter.as_str().parse().unwrap(),
                verse.as_str().parse().unwrap(),
            );
            StoredDecision {
                key: CheckKey {
                    tool,
                    check_id: CheckId::parse(context["checkId"].as_str().unwrap()).unwrap(),
                    target: CheckTarget::Verse {
                        book,
                        chapter,
                        verse,
                    },
                    occurrence: u32::try_from(context["occurrence"].as_u64().unwrap()).unwrap(),
                },
                quote: context["quoteString"].as_str().unwrap().to_owned(),
                group_id: context["groupId"].as_str().unwrap().to_owned(),
                decision: decision_of(json, &file_resource(file), &text),
            }
        })
        .collect()
}
