//! The TWL export's `./payload/<category>/<slug>.md` links name the same articles as the
//! repository's `rc://*/tw/dict/bible/<category>/<slug>` links (`bundle/en/README.md`: DCS
//! rewrites each `TWLink` in the export), so the checks derived from either are the same, for
//! all 59 books of the release.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::io::Read;

use common::{eng, fixture, fixture_path};
use tc_check::derive::{CheckList, word_links_for_book};
use tc_core::BookId;
use tc_helps::WordArticleId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_word_links};

fn archive(file: &str) -> zip::ZipArchive<std::fs::File> {
    let path = fixture_path("../bundle/en").join(file);
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

/// The articles of the en_tw v91 pin, which gate the tW checks.
fn tw_articles() -> BTreeSet<WordArticleId> {
    let mut text = String::new();
    archive("en_tw_v91.sb.zip")
        .by_name("en_tw/metadata.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    parse_burrito_metadata(&text).unwrap().word_articles()
}

/// `./payload/kt/god.md` written back as `rc://*/tw/dict/bible/kt/god`.
fn as_rc_links(tsv: &str) -> String {
    let mut out = String::with_capacity(tsv.len() + tsv.len() / 8);
    let mut rest = tsv;
    while let Some(at) = rest.find("./payload/") {
        out.push_str(&rest[..at]);
        out.push_str("rc://*/tw/dict/bible/");
        rest = &rest[at + "./payload/".len()..];
        let end = rest.find(".md").expect("a payload link ends in .md");
        out.push_str(&rest[..end]);
        rest = &rest[end + ".md".len()..];
    }
    out.push_str(rest);
    out
}

fn derive(tsv: &str, book: BookId, articles: &BTreeSet<WordArticleId>) -> CheckList {
    let links = parse_word_links(tsv, TsvScope::Book(book)).unwrap();
    word_links_for_book(&links, Some(articles), &eng(book))
}

#[test]
fn payload_links_derive_the_same_checks_as_rc_links_in_all_59_books() {
    let articles = tw_articles();
    let mut export = archive("en_twl_v91.sb.zip");
    let mut books = 0;
    let mut rows = 0;
    for i in 0..export.len() {
        let mut entry = export.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let Some(code) = name
            .strip_prefix("en_twl/ingredients/")
            .and_then(|n| n.strip_suffix(".tsv"))
            .filter(|c| c.len() == 3)
        else {
            continue;
        };
        let book: BookId = code.parse().unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        assert!(
            text.contains("./payload/"),
            "{code}: the export writes payload links"
        );
        let from_export = derive(&text, book, &articles);
        let rc = as_rc_links(&text);
        assert!(!rc.contains("./payload/"));
        let from_rc = derive(&rc, book, &articles);
        assert_eq!(from_export, from_rc, "{code}: payload and rc links differ");
        assert!(
            !from_export.items.is_empty(),
            "{code}: the book has word-link checks"
        );
        rows += from_export.items.len() + from_export.skipped.len() + from_export.dropped.len();
        books += 1;
    }
    assert_eq!(books, 59);
    assert_eq!(
        rows, 85_764,
        "every TWL row of the release is accounted for"
    );
}

/// Psalms: the export's links, the same links written as rc links, and the repository's own
/// file at the v91 commit give one list.
#[test]
fn the_psalms_export_equals_the_repositorys_own_file() {
    let articles = tw_articles();
    let mut text = String::new();
    archive("en_twl_v91.sb.zip")
        .by_name("en_twl/ingredients/PSA.tsv")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let repo = fixture("scale/psa/en_twl/twl_PSA.tsv");
    let from_export = derive(&text, BookId::Psalms, &articles);
    let from_repo = derive(&repo, BookId::Psalms, &articles);
    assert_eq!(from_export.items.len(), 7_683);
    assert_eq!(from_export, from_repo);
}
