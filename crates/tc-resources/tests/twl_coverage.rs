//! tC4 `test/twlPin.test.ts` (#644, D96): the words links of a `_twl` release cover the
//! books it holds, although its export carries the tW articles too. A book no `en_twl`
//! release holds resolves to no rung for tW, and that coverage is what travels with the
//! project's pins.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use md5::{Digest, Md5};
use serde_json::{Value, json};
use tc_core::BookId;
use tc_resources::resolve::{
    PreflightState, Resolution, Resolved, backfill_coverage, preflight_tool_book, resolve_slot,
    resolve_tool,
};
use tc_resources::{
    BookCoverage, Collection, GatewayLanguage, HelpsChoice, LanguageSet, Missing, PartStatus,
    ProjectPins, ResourcePin, ResourceStore, SetSlot, ToolId, install_dcs_zip, load_book, suite,
};

use common::{fixture, fixture_pin, zip};

const RESOURCES: &str = "tc4/test-fixtures/resources";

fn store() -> (tempfile::TempDir, ResourceStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    (dir, store)
}

/// The tW article the `en_twl` exports carry. Its bytes are en_tw v87's, which the
/// en_twl v86 and v91 metadata list with the same checksum.
fn god_article() -> Vec<u8> {
    fixture(&format!("{RESOURCES}/en_tw@v87/payload-kt-god.md"))
}

/// The en_twl v86 export slice as Door43 serves it: its own metadata, the Titus TSV and
/// one of the 954 articles the export carries.
fn twl86_export() -> Vec<u8> {
    let metadata = fixture(&format!("{RESOURCES}/en_twl@v86/metadata.json"));
    let tit = fixture(&format!("{RESOURCES}/en_twl@v86/TIT.tsv"));
    let god = god_article();
    zip(&[
        ("en_twl/metadata.json", &metadata),
        ("en_twl/ingredients/TIT.tsv", &tit),
        ("en_twl/ingredients/payload/kt/god.md", &god),
    ])
}

fn install_twl86(store: &ResourceStore) -> ResourcePin {
    let pin = fixture_pin("en_twl@v86", "unfoldingWord/en_twl", "v86")
        .with_flavor("parascriptural/x-bcvarticles");
    install_dcs_zip(store, &pin, &twl86_export(), 1 << 30).unwrap();
    pin
}

/// The book TSVs the en_twl v91 export lists (tC4 `booksOf`).
fn listed_books(metadata: &Value) -> Vec<String> {
    metadata["ingredients"]
        .as_object()
        .unwrap()
        .keys()
        .filter_map(|k| {
            let code = k.strip_prefix("ingredients/")?.strip_suffix(".tsv")?;
            (code.len() == 3
                && code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
            .then(|| code.to_owned())
        })
        .collect()
}

/// The en_twl v91 export in its real shape: the export's own metadata (revision, the 59
/// book TSVs and 954 articles it lists), a stand-in body for each book TSV with its
/// listed checksum and size rewritten to match, and the real god.md article. Only the
/// TSV rows are not the release's; this fixture vendors the metadata alone.
fn twl91_export() -> Vec<u8> {
    let mut metadata: Value =
        serde_json::from_slice(&fixture(&format!("{RESOURCES}/en_twl@v91/metadata.json"))).unwrap();
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for code in listed_books(&metadata) {
        let path = format!("ingredients/{code}.tsv");
        let body = format!(
            "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n1:1\tab{}\tkeyterm\tx\t1\trc://*/tw/dict/bible/kt/god\n",
            code.to_lowercase()
        )
        .into_bytes();
        metadata["ingredients"][&path] = json!({
            "checksum": { "md5": Md5::digest(&body).iter().map(|b| format!("{b:02x}")).collect::<String>() },
            "mimeType": "text/tab-separated-values",
            "size": body.len(),
        });
        entries.push((format!("en_twl/{path}"), body));
    }
    entries.push((
        "en_twl/ingredients/payload/kt/god.md".to_owned(),
        god_article(),
    ));
    entries.push((
        "en_twl/metadata.json".to_owned(),
        serde_json::to_vec(&metadata).unwrap(),
    ));
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    zip(&refs)
}

fn english(twl: ResourcePin) -> LanguageSet {
    LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
    .with(SetSlot::TranslationWordsLinks, twl)
    .with(
        SetSlot::TranslationWords,
        suite::english_helps()
            .pin(SetSlot::TranslationWords)
            .unwrap()
            .clone(),
    )
}

#[test]
fn a_links_export_that_carries_articles_covers_only_its_book_files() {
    let (_dir, store) = store();
    let v86 = install_twl86(&store);
    let installed = store.dcs(&v86).unwrap().unwrap();
    assert!(installed.has_file("ingredients/payload/kt/god.md"));
    let titus = BookCoverage::books([BookId::Titus]);
    assert_eq!(installed.books(), titus.as_ref());
    assert_eq!(installed.dcs_pin().unwrap().books, titus);
}

#[test]
fn a_book_the_primary_links_lack_falls_back_and_backfill_records_the_real_books() {
    let (_dir, store) = store();
    let v86 = install_twl86(&store);
    // The English rung records Psalms and Titus; en_twl v86 holds Titus alone.
    let v91 = suite::english_twl()
        .clone()
        .with_books(BookCoverage::books([BookId::Psalms, BookId::Titus]).unwrap());
    let pins = ProjectPins {
        primary: Some(english(v86.clone())),
        fallback: Some(english(v91.clone())),
        ..ProjectPins::default()
    };

    let psa = resolve_slot(
        &pins,
        SetSlot::TranslationWordsLinks,
        Collection::Book(BookId::Psalms),
        &store,
    )
    .and_then(Resolved::usable)
    .unwrap();
    assert_eq!(psa.rung, tc_resources::Rung::Fallback);
    assert!(psa.pin.same_identity(&v91));
    let tit = resolve_slot(
        &pins,
        SetSlot::TranslationWordsLinks,
        Collection::Book(BookId::Titus),
        &store,
    )
    .and_then(Resolved::usable)
    .unwrap();
    assert_eq!(tit.rung, tc_resources::Rung::Primary);

    let installed = |p: &ResourcePin| store.has_dcs(p);
    let pre = preflight_tool_book(
        Some(&pins),
        ToolId::TranslationWords,
        BookId::Psalms,
        &store,
        &installed,
        false,
    );
    assert_ne!(pre.state, PreflightState::Ready);
    assert_eq!(pre.needs.as_ref(), Some(&v91));

    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms);
    assert_eq!(
        loaded.parts[&tc_helps::HelpsPart::WordLinks],
        PartStatus::Missing(Missing::NotInstalled(v91.clone()))
    );

    let (next, changed) = backfill_coverage(&pins, &store);
    let recorded = next
        .primary
        .as_ref()
        .unwrap()
        .pin(SetSlot::TranslationWordsLinks)
        .unwrap()
        .books
        .clone();
    assert_eq!(recorded, BookCoverage::books([BookId::Titus]));
    assert_eq!(changed.len(), 1);
}

#[test]
fn the_v91_export_lists_59_book_tsvs() {
    let metadata: Value =
        serde_json::from_slice(&fixture(&format!("{RESOURCES}/en_twl@v91/metadata.json"))).unwrap();
    assert_eq!(listed_books(&metadata).len(), 59);
}

#[test]
fn unreleased_books_resolve_to_no_rung_for_tw() {
    let (_dir, store) = store();
    let twl = suite::english_twl();
    let installed = install_dcs_zip(&store, twl, &twl91_export(), 1 << 30).unwrap();
    assert!(installed.has_file("ingredients/payload/kt/god.md"));
    match installed.books() {
        Some(BookCoverage::Books(books)) => assert_eq!(books.len(), 59),
        other => panic!("en_twl v91 must record its 59 books, not {other:?}"),
    }
    let pins = suite::project_pins();
    for code in ["1CH", "2CH", "ECC", "EZK", "ISA", "JER", "NUM"] {
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
    let tit: Resolution = resolve_tool(
        &pins,
        ToolId::TranslationWords,
        Collection::Book(BookId::Titus),
        &store,
    )
    .and_then(Resolved::usable)
    .unwrap();
    assert!(tit.pin.same_identity(twl));

    // The coverage the project records is the 59 books, never the whole collection.
    let (next, _) = backfill_coverage(&pins, &store);
    for set in [next.primary.as_ref(), next.fallback.as_ref()] {
        let books = set
            .unwrap()
            .pin(SetSlot::TranslationWordsLinks)
            .unwrap()
            .books
            .clone();
        assert!(
            matches!(&books, Some(BookCoverage::Books(list)) if list.len() == 59),
            "{books:?}"
        );
    }
}
