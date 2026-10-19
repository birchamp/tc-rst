//! The owner's tW/TWL ruling on the real exports (`bundle/en/README.md`, tC4 D96 ruling 5):
//! the tW export carries 66 TWL book files that DCS builds from `en_twl` master, and the TWL
//! export carries 954 tW articles. Both copies are ignored. Word-link rows come only from the
//! `translationWordsLinks` pin (`en_twl`, 59 books), articles only from the
//! `translationWords` pin (`en_tw`, the whole collection).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use tc_core::BookId;
use tc_helps::HelpsPart;
use tc_resources::resolve::{Resolved, resolve_slot, resolve_tool};
use tc_resources::{
    Collection, HelpsChoice, Missing, PartStatus, SetSlot, ToolId, install_dcs_zip, load_book,
    suite, with_opened_paths,
};

use common::real::{BOOK_REPOS, export_with, store_with};

/// Whether a folder of the path is called `name` (a separator of either kind).
fn has_component(path: &std::path::Path, name: &str) -> bool {
    path.components().any(|c| c.as_os_str() == name)
}

fn under(paths: &[PathBuf], repo: &str) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| has_component(p, repo))
        .cloned()
        .collect()
}

fn tsv(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "tsv"))
        .cloned()
        .collect()
}

/// The tW export holds a TWL file for every book, Numbers included, built from master: none
/// of it is read, and it does not make the TWL pin cover Numbers.
#[test]
fn num_reports_word_links_missing_and_reads_none_of_the_tw_exports_tsvs() {
    let (_dir, store) = store_with(BOOK_REPOS);
    let pins = suite::project_pins();
    let (loaded, opened) =
        with_opened_paths(|| load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Numbers));
    match &loaded.parts[&HelpsPart::WordLinks] {
        PartStatus::Missing(Missing::NotCovered { pinned }) => {
            assert!(
                pinned.iter().any(|p| p.same_identity(suite::english_twl())),
                "the missing links name the TWL pin: {pinned:?}"
            );
        }
        other => panic!("Numbers has no word links in the 59 books of en_twl: {other:?}"),
    }
    assert!(
        loaded.helps.word_links().is_empty(),
        "no word link is loaded"
    );
    let tw_tsvs = tsv(&under(&opened, "en_tw"));
    assert!(
        tw_tsvs.is_empty(),
        "the tW export's master-built TSVs are never opened: {tw_tsvs:?}"
    );
    assert!(
        tsv(&under(&opened, "en_twl")).is_empty(),
        "no TWL file is opened for Numbers either"
    );
}

#[test]
fn psalms_loads_exactly_7683_rows_from_en_twl() {
    let (_dir, store) = store_with(BOOK_REPOS);
    let pins = suite::project_pins();
    let (loaded, opened) =
        with_opened_paths(|| load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms));
    match &loaded.parts[&HelpsPart::WordLinks] {
        PartStatus::Loaded { source, .. } => {
            assert!(source.to_string().contains("en_twl"), "{source}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(loaded.helps.word_links().len(), 7_683);
    let tsvs = tsv(&opened);
    let from_twl = under(&tsvs, "en_twl");
    assert!(
        from_twl.iter().any(|p| p.ends_with("ingredients/PSA.tsv")),
        "{tsvs:?}"
    );
    assert!(
        under(&tsvs, "en_tw").is_empty(),
        "no TSV is read from the tW export: {tsvs:?}"
    );
}

/// The articles come from `en_tw` only: a copy of the TWL export with one payload article
/// changed still loads the `en_tw` text, and no `payload/` file of the TWL install is opened.
#[test]
fn articles_come_from_en_tw_only() {
    let article = "ingredients/payload/kt/god.md";
    let changed = export_with(
        "en_twl_v91.sb.zip",
        "en_twl",
        &[(
            article,
            b"# Tampered\n\nThis is not the article of en_tw.\n".to_vec(),
        )],
    );
    let pins = suite::project_pins();
    let god = |store: &tc_resources::ResourceStore| {
        let loaded = load_book(store, &pins, &HelpsChoice::Dcs, BookId::Psalms);
        let id = loaded
            .helps
            .word_links()
            .iter()
            .map(|l| l.article.clone())
            .find(|a| {
                let text = format!("{a:?}");
                text.contains("KeyTerm") && text.contains("\"god\"")
            })
            .expect("Psalms links the article of god");
        loaded
            .helps
            .word_article(&id)
            .expect("the article loads")
            .item
            .clone()
    };
    let (_original_dir, original) = store_with(BOOK_REPOS);
    let expected = god(&original);
    // The same store, but with the changed copy of the TWL export installed in place of the
    // real one.
    let (_dir, store) = store_with(&["en_tw", "en_tn", "en_ult", "en_ust", "hbo_uhb"]);
    install_dcs_zip(&store, suite::english_twl(), &changed, 1 << 30).unwrap();
    let ((got, loaded_words), opened) = with_opened_paths(|| {
        let article = god(&store);
        let words = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms)
            .helps
            .word_links()
            .len();
        (article, words)
    });
    assert_eq!(loaded_words, 7_683);
    assert_eq!(got.body, expected.body, "the text is en_tw's");
    assert!(!got.body.contains("Tampered"));
    let payload: Vec<_> = under(&opened, "en_twl")
        .into_iter()
        .filter(|p| has_component(p, "payload"))
        .collect();
    assert!(
        payload.is_empty(),
        "the TWL install's articles are not read: {payload:?}"
    );
}

/// The tW pin covers the whole collection (its articles serve every book), and the TWL pin
/// its 59 books: seven books resolve to no rung for the tW tool.
#[test]
fn the_tw_pin_covers_every_book_and_the_twl_pin_its_59() {
    let (_dir, store) = store_with(&["en_tw", "en_twl"]);
    let pins = suite::project_pins();
    for ordinal in 1..=66u8 {
        let book = BookId::from_ordinal(ordinal).unwrap();
        let resolved = resolve_slot(
            &pins,
            SetSlot::TranslationWords,
            Collection::Book(book),
            &store,
        );
        assert!(
            matches!(resolved, Some(Resolved::Pin(_))),
            "{book:?}: the tW articles serve every book: {resolved:?}"
        );
    }
    let twl = store.dcs(suite::english_twl()).unwrap().unwrap();
    match twl.books() {
        Some(tc_resources::BookCoverage::Books(books)) => assert_eq!(books.len(), 59),
        other => panic!("{other:?}"),
    }
    for code in ["NUM", "1CH", "2CH", "ECC", "ISA", "JER", "EZK"] {
        let book: BookId = code.parse().unwrap();
        assert_eq!(
            resolve_tool(
                &pins,
                ToolId::TranslationWords,
                Collection::Book(book),
                &store
            ),
            None,
            "{code}"
        );
    }
    for code in ["PSA", "GEN", "TIT"] {
        let book: BookId = code.parse().unwrap();
        assert!(
            resolve_tool(
                &pins,
                ToolId::TranslationWords,
                Collection::Book(book),
                &store
            )
            .and_then(Resolved::usable)
            .is_some(),
            "{code}"
        );
    }
}
