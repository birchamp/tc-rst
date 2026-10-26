//! Check lists from the real unfoldingWord helps vendored by tC4, with the numbers tC4's
//! own suites assert (`derive-full-strength`, `deriveForProject`, `deriveObs`,
//! `twArticleGate`, `s0c-derive-parity`, `obsCheckSession`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;

use common::{checks_of, eng, notes, obs_notes, obs_word_links, resource, word_links};
use tc_burrito::ScopeRange;
use tc_check::derive::{
    BookSpec, DropReason, SkipReason, notes_for_book, notes_for_stories, word_links_for_book,
    word_links_for_stories,
};
use tc_check::{CheckTarget, ToolId};
use tc_core::BookId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_versification::{StandardScheme, Unplaceable};

fn categories(list: &tc_check::derive::CheckList) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for item in &list.items {
        *out.entry(item.category.name()).or_default() += 1;
    }
    out
}

fn locator(target: &CheckTarget) -> String {
    target.locator()
}

#[test]
fn en_tn_titus_derives_157_checks_with_the_tc3_categories() {
    let list = notes_for_book(
        &notes("en_tn@v86/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    );
    assert_eq!(list.items.len(), 157);
    assert!(!list.mapped);
    assert!(list.dropped.is_empty());
    let plain = list
        .skipped
        .iter()
        .filter(|s| s.reason == SkipReason::PlainNote)
        .count();
    assert_eq!((plain, list.skipped.len()), (49, 49));
    assert_eq!(
        categories(&list),
        BTreeMap::from([
            ("culture", 26),
            ("discourse", 1),
            ("figures", 53),
            ("grammar", 71),
            ("other", 6)
        ])
    );
    let groups: std::collections::BTreeSet<&str> =
        list.items.iter().map(|i| i.group_id.as_str()).collect();
    assert_eq!(groups.len(), 28);
    let discontinuous = list
        .items
        .iter()
        .filter(|i| i.quote.as_str().contains('&'))
        .count();
    assert_eq!(discontinuous, 12);
    assert!(
        list.items
            .iter()
            .all(|i| i.key.tool == ToolId::TranslationNotes)
    );
}

#[test]
fn a_tn_check_records_tc4_s_context() {
    let list = notes_for_book(
        &notes("en_tn@v86/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    );
    let item = &list.items[0];
    let context = serde_json::Value::Object(item.context());
    let keys: Vec<&str> = context
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "checkId",
            "occurrenceNote",
            "reference",
            "tool",
            "groupId",
            "quote",
            "quoteString",
            "occurrence"
        ]
    );
    assert_eq!(context["reference"]["bookId"], "tit");
    assert!(context["reference"]["chapter"].is_number());
    assert_eq!(context["tool"], "translationNotes");
    assert!(context["quote"].is_array());
    // The note keeps the TSV's escapes, as tC4 stores the raw cell.
    let row = resource("en_tn@v86/TIT.tsv");
    let raw_note = row
        .lines()
        .find(|l| l.split('\t').nth(1) == Some(item.key.check_id.as_str()))
        .unwrap()
        .split('\t')
        .nth(6)
        .unwrap();
    assert_eq!(context["occurrenceNote"], raw_note);
}

#[test]
fn en_twl_titus_derives_188_checks_from_rc_links() {
    let list = word_links_for_book(
        &word_links("en_twl@v86/TIT.tsv", BookId::Titus),
        None,
        &eng(BookId::Titus),
    );
    assert_eq!(list.items.len(), 188);
    assert_eq!(
        categories(&list),
        BTreeMap::from([("kt", 111), ("names", 6), ("other", 71)])
    );
    let context = serde_json::Value::Object(list.items[0].context());
    assert!(context["quote"].is_string());
    assert!(context.get("occurrenceNote").is_none());
}

#[test]
fn the_combined_export_form_of_links_reads_the_same() {
    let list = word_links_for_book(
        &word_links("en_tw@v87/TIT.tsv", BookId::Titus),
        None,
        &eng(BookId::Titus),
    );
    assert_eq!(
        categories(&list),
        BTreeMap::from([("kt", 110), ("names", 6), ("other", 66)])
    );
    let god = list.items.iter().find(|i| i.group_id == "god").unwrap();
    assert_eq!(god.category.name(), "kt");
}

#[test]
fn es_419_titus_derives_112_checks() {
    let list = notes_for_book(
        &notes("es-419_tn@v66/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    );
    assert_eq!(list.items.len(), 112);
}

#[test]
fn a_link_to_an_article_the_set_lacks_is_not_a_check() {
    let meta = parse_burrito_metadata(&resource("en_tw@v91/metadata.json")).unwrap();
    let articles = meta.word_articles();
    assert_eq!(articles.len(), 953);
    let links = word_links("en_tw@v91/2SA.tsv", BookId::SecondSamuel);
    let spec = eng(BookId::SecondSamuel);
    let ungated = word_links_for_book(&links, None, &spec);
    assert_eq!(ungated.items.len(), 3316);
    let gated = word_links_for_book(&links, Some(&articles), &spec);
    assert_eq!(gated.items.len(), 3315);
    let hidden: Vec<String> = gated
        .skipped
        .iter()
        .filter(|s| s.reason == SkipReason::ArticleMissing)
        .map(|s| format!("{:?}", s.id))
        .collect();
    assert_eq!(hidden.len(), 1);
    assert!(hidden[0].contains("vwrw"));
    assert!(!checks_of(&gated).contains(&"vwrw".to_owned()));

    let titus = word_links("en_tw@v91/TIT.tsv", BookId::Titus);
    let all = word_links_for_book(&titus, None, &eng(BookId::Titus));
    assert_eq!(all.items.len(), 182);
    assert_eq!(
        word_links_for_book(&titus, Some(&articles), &eng(BookId::Titus)).items,
        all.items
    );
    let none = word_links_for_book(&titus, Some(&Default::default()), &eng(BookId::Titus));
    assert!(none.items.is_empty());
}

#[test]
fn a_span_verse_stays_text_in_the_identity() {
    let tsv = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n2:9-10\ts1p2\tkeyterm\tיְהוָה\t1\trc://*/tw/dict/bible/kt/yahweh\n";
    let links =
        tc_helps::tsv::parse_word_links(tsv, tc_helps::tsv::TsvScope::Book(BookId::Jonah)).unwrap();
    let list = word_links_for_book(&links, None, &eng(BookId::Jonah));
    let item = &list.items[0];
    assert_eq!(locator(&item.key.target), "2:9-10");
    assert_eq!(item.context()["reference"]["verse"], "9-10");
}

fn jonah(
    project: Option<&'static tc_versification::Scheme>,
    scope: &'static [ScopeRange],
) -> tc_check::derive::CheckList {
    let spec = BookSpec {
        book: BookId::Jonah,
        resource_frame: StandardScheme::Eng.scheme(),
        project_frame: project,
        scope,
    };
    notes_for_book(&notes("en_tn@v86/JON.tsv", BookId::Jonah), &spec)
}

#[test]
fn a_cross_frame_project_maps_before_anything_else() {
    let before = jonah(Some(StandardScheme::Eng.scheme()), &[]);
    let after = jonah(Some(StandardScheme::Rsc.scheme()), &[]);
    assert!(after.mapped);
    let refs = |l: &tc_check::derive::CheckList| -> Vec<String> {
        l.items.iter().map(|i| locator(&i.key.target)).collect()
    };
    assert!(refs(&before).contains(&"1:17".to_owned()));
    assert!(!refs(&after).contains(&"1:17".to_owned()));
    let seventeen: Vec<_> = before
        .items
        .iter()
        .filter(|i| locator(&i.key.target) == "1:17")
        .collect();
    assert!(!seventeen.is_empty());
    for item in seventeen {
        let moved = after
            .items
            .iter()
            .find(|i| i.key.check_id == item.key.check_id)
            .unwrap();
        assert_eq!(locator(&moved.key.target), "2:1");
        // Mapping touches only the place.
        assert_eq!(moved.quote, item.quote);
        assert_eq!(moved.key.occurrence, item.key.occurrence);
        assert_eq!(moved.group_id, item.group_id);
        assert_eq!(moved.context()["reference"]["bookId"], "jon");
    }
}

#[test]
fn scope_filters_the_mapped_reference() {
    static TWO_ONE: std::sync::LazyLock<Vec<ScopeRange>> =
        std::sync::LazyLock::new(|| vec!["2:1".parse().unwrap()]);
    let list = jonah(Some(StandardScheme::Rsc.scheme()), &TWO_ONE);
    assert!(!list.items.is_empty());
    assert!(list.items.iter().all(|i| locator(&i.key.target) == "2:1"));
    let source_seventeen: Vec<_> = jonah(Some(StandardScheme::Eng.scheme()), &[])
        .items
        .into_iter()
        .filter(|i| locator(&i.key.target) == "1:17")
        .collect();
    for item in source_seventeen {
        assert!(
            list.items
                .iter()
                .any(|i| i.key.check_id == item.key.check_id)
        );
    }
}

#[test]
fn an_unplaceable_check_is_reported_only_when_it_concerns_the_scope() {
    static ONE_ONE: std::sync::LazyLock<Vec<ScopeRange>> =
        std::sync::LazyLock::new(|| vec!["1:1".parse().unwrap()]);
    static ONE_SEVENTEEN: std::sync::LazyLock<Vec<ScopeRange>> =
        std::sync::LazyLock::new(|| vec!["1:17".parse().unwrap()]);
    let vul = Some(StandardScheme::Vul.scheme());
    let narrow = jonah(vul, &ONE_ONE);
    assert!(narrow.items.iter().all(|i| locator(&i.key.target) == "1:1"));
    assert!(narrow.dropped.is_empty());

    let landing = jonah(vul, &ONE_SEVENTEEN);
    assert!(landing.items.is_empty());
    assert!(!landing.dropped.is_empty());
    assert!(
        landing
            .dropped
            .iter()
            .all(|d| locator(&d.item.key.target) == "1:17")
    );

    let whole = jonah(vul, &[]);
    let all = jonah(Some(StandardScheme::Eng.scheme()), &[]).items.len();
    assert_eq!(whole.items.len() + whole.dropped.len(), all);
    for d in &whole.dropped {
        assert!(matches!(d.reason, DropReason::Unplaceable(_)));
        assert!(
            !whole
                .items
                .iter()
                .any(|i| i.key.check_id == d.item.key.check_id)
        );
    }
    assert!(
        whole
            .dropped
            .iter()
            .any(|d| d.reason == DropReason::Unplaceable(Unplaceable::PastChapterEnd))
    );
}

#[test]
fn an_unknown_project_frame_places_nothing() {
    let list = jonah(None, &[]);
    assert!(list.items.is_empty());
    assert_eq!(
        list.dropped.len(),
        jonah(Some(StandardScheme::Eng.scheme()), &[]).items.len()
    );
    assert!(
        list.dropped
            .iter()
            .all(|d| d.reason == DropReason::UnknownFrame)
    );
    // With a partial scope, an unknown frame gives no verse to attribute a check to the
    // scope by, so nothing is reported and the scope is not shown as all dropped (tC4
    // `deriveForProject`).
    static CHAPTER_ONE: std::sync::LazyLock<Vec<ScopeRange>> =
        std::sync::LazyLock::new(|| vec!["1".parse().unwrap()]);
    let partial = jonah(None, &CHAPTER_ONE);
    assert!(partial.items.is_empty() && partial.dropped.is_empty());
}

#[test]
fn every_obs_note_is_a_check_keyed_by_story_and_frame() {
    let notes = obs_notes("en_obs-tn@v13/OBS.tsv");
    let list = notes_for_stories(&notes, None);
    assert_eq!(list.items.len(), 2325);
    assert!(list.skipped.is_empty());
    assert!(
        list.items
            .iter()
            .all(|i| matches!(i.key.target, CheckTarget::Frame { .. }))
    );
    assert!(list.items.iter().all(|i| i.key.occurrence == 1));
    let stories: std::collections::BTreeSet<String> =
        list.items.iter().map(|i| i.key.target.parts().0).collect();
    assert_eq!(stories.len(), 50);
    let titles: Vec<_> = list
        .items
        .iter()
        .filter(|i| i.key.target.parts().1 == "0")
        .collect();
    assert_eq!(titles.len(), 58);
    let creation = titles
        .iter()
        .find(|i| i.key.target.parts().0 == "1")
        .unwrap();
    assert_eq!(creation.key.check_id.as_str(), "i6lj");
    assert_eq!(creation.quote.as_str(), "The Creation");
    assert_eq!(creation.group_id, "");
    assert_eq!(
        serde_json::Value::Object(creation.context())["reference"],
        serde_json::json!({"story": 1, "frame": 0})
    );

    let one = notes_for_stories(&notes, Some(1));
    assert_eq!(one.items.len(), 91);
    assert!(notes_for_stories(&notes, Some(51)).items.is_empty());
}

#[test]
fn every_obs_link_is_a_check() {
    let list = word_links_for_stories(&obs_word_links("en_obs-twl@v3/OBS.tsv"), None);
    assert_eq!(list.items.len(), 2381);
    assert!(list.items.iter().all(|i| i.key.target.parts().1 != "0"));
    assert_eq!(
        word_links_for_stories(&obs_word_links("en_obs-twl@v3/OBS.tsv"), Some(1))
            .items
            .len(),
        35
    );
}

#[test]
fn aquifer_notes_name_no_occurrence_so_none_becomes_a_check_and_each_is_reported() {
    let articles = tc_helps::aquifer::parse_aquifer_articles(&common::fixture(
        "aquifer/UWTranslationNotes/eng/json/56.content.json",
    ))
    .unwrap()
    .items;
    let notes = tc_helps::aquifer::notes_from_aquifer(&articles).items;
    let list = notes_for_book(&notes, &eng(BookId::Titus));
    assert!(list.items.is_empty());
    assert_eq!(list.skipped.len(), notes.len());
    let unrecorded = list
        .skipped
        .iter()
        .filter(|s| s.reason == SkipReason::OccurrenceUnrecorded)
        .count();
    assert_eq!(unrecorded, 157, "the 157 checks of en_tn Titus");
}

#[test]
fn rows_whose_identity_cannot_be_stored_are_reported() {
    let tsv = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n\
               1:1\taaaa\t\trc://*/ta/man/translate/figs-metaphor\tδοῦλος\t-1\tnote\n\
               1:1\tbbbb\t\trc://*/ta/man/translate/figs-metaphor\tδοῦλος\t\tnote\n\
               1:2\tcccc\t\trc://*/ta/man/translate/figs-metaphor\tἐλπίδι\t1\tnote\n\
               1:2\tcccc\t\trc://*/ta/man/translate/figs-idiom\tἐλπίδι\t1\tnote two\n";
    let parsed =
        tc_helps::tsv::parse_notes(tsv, tc_helps::tsv::TsvScope::Book(BookId::Titus)).unwrap();
    let list = notes_for_book(&parsed.notes, &eng(BookId::Titus));
    assert_eq!(checks_of(&list), ["cccc"]);
    let reasons: Vec<&SkipReason> = list.skipped.iter().map(|s| &s.reason).collect();
    assert!(matches!(
        reasons[..],
        [
            SkipReason::EveryOccurrence,
            SkipReason::OccurrenceUnrecorded,
            SkipReason::DuplicateKey(_)
        ]
    ));
}
