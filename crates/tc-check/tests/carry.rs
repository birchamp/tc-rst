//! Carry-over plans and the counts the upgrade and gateway-change dialogs show, from
//! tC4's suites (`carryOver`, `derive-full-strength`, `twArticleGate`, `gatewayChange`)
//! and properties that no decision is ever dropped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::cell::Cell;
use std::collections::BTreeSet;

use common::{LanguageSet, Pins, Set, Slot, eng, notes, pin, projected, resource, t, word_links};
use proptest::prelude::*;
use tc_check::carry::{Match, Outcome, StoredDecision, Summary, plan, same_quote};
use tc_check::change::{Affected, Unit, consequences};
use tc_check::derive::{notes_for_book, word_links_for_book};
use tc_check::fix::{Machine, PreflightState, preflight};
use tc_check::session::open;
use tc_check::status::{CheckState, Edit, Triage, assess, decision_edit};
use tc_check::{
    CheckItem, CheckTarget, Collection, DecisionRecord, Ladder, Pin, ResourceRef, Selection,
    TargetText, ToolId,
};
use tc_core::BookId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_helps::{Slug, WordArticleId, WordCategory};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::json::JsonMap;

const TN_HEADER: &str = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote";

/// A tN table of `[reference, id, support reference, quote, occurrence, note]` rows.
fn tsv(rows: &[[&str; 6]]) -> Vec<tc_helps::Note> {
    let mut text = TN_HEADER.to_owned();
    for r in rows {
        text.push_str(&format!(
            "\n{}\t{}\t\t{}\t{}\t{}\t{}",
            r[0], r[1], r[2], r[3], r[4], r[5]
        ));
    }
    parse_notes(&text, TsvScope::Book(BookId::Titus))
        .unwrap()
        .notes
}

fn titus(rows: &[[&str; 6]]) -> Vec<CheckItem> {
    notes_for_book(&tsv(rows), &eng(BookId::Titus)).items
}

/// A decision as the Check screen records "Mark valid" with one word selected.
fn decided(item: &CheckItem, against: &ResourceRef) -> StoredDecision {
    let words = vec![Selection {
        text: "ho".into(),
        occurrence: 1,
        occurrences: 1,
        extra: JsonMap::new(),
    }];
    // As the Check screen records it, with the check's context, which names its quote and
    // group id.
    let decision = decision_edit(item, Edit::Triage(Triage::Valid(words)), against)
        .apply(None, Some("ho"))
        .unwrap();
    let stored = StoredDecision::of(item.key.clone(), decision);
    assert_eq!(
        (stored.quote.as_str(), stored.group_id.as_str()),
        (item.quote.as_str(), item.group_id.as_str())
    );
    stored
}

fn spanish() -> ResourceRef {
    pin("Es-419_gl/es-419_tn", "v66", &[]).resource
}

fn english() -> ResourceRef {
    pin("unfoldingWord/en_tn", "v89", &[]).resource
}

fn no_text(_: &CheckTarget) -> Option<&'static TargetText> {
    None
}

#[test]
fn a_finished_book_is_not_finished_when_the_new_resource_asks_new_questions() {
    let es = titus(&[
        ["1:1", "abc1", "figs-metaphor", "δοῦλος", "1", "nota uno"],
        [
            "1:2",
            "abc2",
            "figs-abstractnouns",
            "ἐλπίδι",
            "1",
            "nota dos",
        ],
    ]);
    let en = titus(&[
        ["1:1", "abc1", "figs-metaphor", "δοῦλος", "1", "note one"],
        [
            "1:3",
            "xyz9",
            "figs-activepassive",
            "ἐφανέρωσεν",
            "1",
            "note three",
        ],
    ]);
    let stored: Vec<StoredDecision> = es.iter().map(|i| decided(i, &spanish())).collect();
    let p = plan(&en, &english(), &stored);
    let summary = p.summary(Some(&spanish()), no_text);
    assert_eq!(
        (summary.carried, summary.invalidated, summary.undecided),
        (1, 1, 1)
    );
    assert_eq!(
        p.entries[0].outcome,
        Outcome::Stays {
            item: 0,
            by: Match::Identity
        }
    );
    assert_eq!(p.entries[1].outcome, Outcome::Unplaced);
    // The unplaced decision is still in the plan's entries: nothing is dropped.
    assert_eq!(p.entries.len(), stored.len());
}

#[test]
fn a_cross_language_twin_carries_and_is_keyed_to_the_new_resource() {
    let es = titus(&[["1:1", "es01", "figs-metaphor", "δοῦλος", "1", "nota"]]);
    let en = titus(&[["1:1", "en99", "figs-metaphor", "δοῦλος", "1", "note"]]);
    let stored = vec![decided(&es[0], &spanish())];
    let p = plan(&en, &english(), &stored);
    let summary = p.summary(Some(&spanish()), no_text);
    assert_eq!(
        (summary.carried, summary.invalidated, summary.undecided),
        (1, 0, 0)
    );
    match &p.entries[0].outcome {
        Outcome::Moves {
            to, by, contested, ..
        } => {
            assert_eq!(to.check_id.as_str(), "en99");
            assert_eq!(*by, Match::Place);
            assert!(!contested);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_check_id_whose_quote_changed_is_another_check() {
    let es = titus(&[[
        "1:1",
        "rtc9",
        "figs-abstractnouns",
        "κατὰ πίστιν",
        "1",
        "nota",
    ]]);
    let en = titus(&[[
        "1:1",
        "rtc9",
        "figs-abstractnouns",
        "κατὰ πίστιν ἐκλεκτῶν Θεοῦ",
        "1",
        "note",
    ]]);
    let stored = vec![decided(&es[0], &spanish())];
    let p = plan(&en, &english(), &stored);
    assert_eq!(p.entries[0].outcome, Outcome::Unplaced);
    assert_eq!(p.undecided, 1);
    // A quote that differs only in punctuation is the same check.
    let cosmetic = titus(&[[
        "1:1",
        "rtc9",
        "figs-abstractnouns",
        "κατὰ πίστιν,",
        "1",
        "note",
    ]]);
    let p = plan(&cosmetic, &english(), &stored);
    assert_eq!(
        p.entries[0].outcome,
        Outcome::Stays {
            item: 0,
            by: Match::CheckId
        }
    );
}

fn titus_tn(rel: &str) -> Vec<CheckItem> {
    notes_for_book(&notes(rel, BookId::Titus), &eng(BookId::Titus)).items
}

#[test]
fn real_english_decisions_reattach_to_spanish_checks_as_tc4_counts() {
    let en = titus_tn("en_tn@v86/TIT.tsv");
    let es = titus_tn("es-419_tn@v66/TIT.tsv");
    let stored: Vec<StoredDecision> = en.iter().map(|i| decided(i, &english())).collect();
    // tC4's derive-full-strength suite runs the cross-language pass alone.
    let results = tc_check::carry::reattach(&stored, &es);
    let attached: Vec<(usize, usize)> = results
        .iter()
        .enumerate()
        .filter_map(|(s, r)| r.map(|(i, _)| (s, i)))
        .collect();
    let by_id = attached
        .iter()
        .filter(|(s, i)| es[*i].key.check_id == stored[*s].key.check_id)
        .count();
    assert_eq!(
        (by_id, attached.len(), results.len() - attached.len()),
        (76, 89, 68)
    );
    // No decision ever rides a changed quote (B18).
    assert!(
        attached
            .iter()
            .all(|(s, i)| same_quote(es[*i].quote.as_str(), &stored[*s].quote))
    );
}

#[test]
fn real_carry_over_places_what_tc4_places_and_keeps_what_tc4_dropped() {
    let en = titus_tn("en_tn@v86/TIT.tsv");
    let es = titus_tn("es-419_tn@v66/TIT.tsv");
    // tC4 `carryOverDecisions` on the same data, run through tC4's own code:
    // - en to es: 83 carried, 73 invalidated, 29 undecided, 156 decisions kept of 157.
    //   Two decisions attached to one Spanish check and tC4 kept only the later one;
    //   here both are carried and flagged, so the person chooses.
    // - es to en: 83 carried, 28 invalidated, 74 undecided, 111 kept of 112. A decision
    //   moved onto the key of xyz6, whose own decision found no check, and tC4 dropped
    //   that one (#448); here it stays, and the move is flagged.
    for (from, to, expected) in [(&en, &es, (84, 2, 73, 29)), (&es, &en, (83, 1, 29, 74))] {
        let stored: Vec<StoredDecision> = from.iter().map(|i| decided(i, &english())).collect();
        let p = plan(to, &spanish(), &stored);
        let s = p.summary(Some(&english()), no_text);
        assert_eq!(
            (s.carried, s.contested, s.invalidated, s.undecided),
            expected
        );
        assert_eq!(s.carried + s.invalidated, stored.len());
    }
    let stored: Vec<StoredDecision> = es.iter().map(|i| decided(i, &english())).collect();
    let p = plan(&en, &spanish(), &stored);
    let contested: Vec<String> = p
        .entries
        .iter()
        .filter_map(|e| match &e.outcome {
            Outcome::Moves {
                to,
                contested: true,
                ..
            } => Some(to.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(contested, ["translationNotes TIT 2:13 xyz6 #1"]);
    let held = p
        .entries
        .iter()
        .find(|e| e.key.check_id.as_str() == "xyz6")
        .unwrap();
    assert_eq!(held.outcome, Outcome::Unplaced);
}

#[test]
fn a_decision_on_a_link_whose_article_left_is_kept_and_counted_as_to_check() {
    let links = word_links("en_tw@v91/2SA.tsv", BookId::SecondSamuel);
    let spec = eng(BookId::SecondSamuel);
    let meta =
        tc_helps::metadata::parse_burrito_metadata(&resource("en_tw@v91/metadata.json")).unwrap();
    let ungated = word_links_for_book(&links, None, &spec).items;
    let gated = word_links_for_book(&links, Some(&meta.word_articles()), &spec).items;
    let old = pin("unfoldingWord/en_twl", "v90", &[]).resource;
    let new = pin("unfoldingWord/en_twl", "v91", &[]).resource;
    let hidden = ungated
        .iter()
        .find(|i| i.key.check_id.as_str() == "vwrw")
        .unwrap();
    let stored = vec![decided(hidden, &old)];
    let p = plan(&gated, &new, &stored);
    assert_eq!(p.entries[0].outcome, Outcome::Unplaced);
    let summary = p.summary(Some(&old), no_text);
    assert_eq!((summary.carried, summary.invalidated), (0, 1));
    assert_eq!(summary.undecided, 3315);
}

#[test]
fn decided_after_counts_what_the_check_screen_will_show() {
    let items = titus(&[
        ["1:1", "aaaa", "figs-metaphor", "δοῦλος", "1", "n"],
        ["1:2", "bbbb", "figs-metaphor", "ἐλπίδι", "1", "n"],
    ]);
    let mut stored: Vec<StoredDecision> = items.iter().map(|i| decided(i, &spanish())).collect();
    // The verse text of the second check changed and lost the selected word.
    stored[1].decision.judgement.verse_text_md5 = verse_text_md5_v1("ho hum");
    let (ho, hum) = (t("ho"), t("hum"));
    let verse = |target: &CheckTarget| Some(if target.locator() == "1:2" { &hum } else { &ho });
    let p = plan(&items, &english(), &stored);
    let summary = p.summary(Some(&spanish()), verse);
    assert_eq!((summary.carried, summary.decided_after), (2, 1));
    let total = Summary::total([summary, summary]);
    assert_eq!((total.carried, total.decided_after), (4, 2));
}

#[test]
fn decided_after_reads_the_verse_a_decision_moves_to() {
    let es = titus(&[["1:1", "abc1", "figs-metaphor", "δοῦλος", "1", "nota"]]);
    let en = titus(&[["1:2", "abc1", "figs-metaphor", "δοῦλος", "1", "note"]]);
    let stored = vec![decided(&es[0], &spanish())];
    let p = plan(&en, &english(), &stored);
    assert!(matches!(
        p.entries[0].outcome,
        Outcome::Moves {
            by: Match::CheckId,
            ..
        }
    ));
    // The words were selected on "ho", the text of the verse the check moves to.
    let (ho, hum) = (t("ho"), t("hum"));
    let verse = |target: &CheckTarget| Some(if target.locator() == "1:2" { &ho } else { &hum });
    let summary = p.summary(Some(&spanish()), verse);
    assert_eq!((summary.carried, summary.decided_after), (1, 1));
}

#[test]
fn a_decision_already_invalid_is_not_counted_again() {
    let items = titus(&[["1:2", "bbbb", "figs-metaphor", "ἐλπίδι", "1", "n"]]);
    let gone = titus(&[["1:1", "aaaa", "figs-metaphor", "δοῦλος", "1", "n"]]);
    let older = pin("Es-419_gl/es-419_tn", "v60", &[]).resource;
    let stored = vec![decided(&gone[0], &older)];
    let summary = plan(&items, &english(), &stored).summary(Some(&spanish()), no_text);
    assert_eq!((summary.invalidated, summary.still_unplaced), (0, 1));
}

#[test]
fn two_decisions_arriving_at_one_check_are_both_kept_as_a_conflict() {
    // Two Spanish notes on the same words of the same verse with one group id; English
    // has one such check. tC4 kept only the later decision; here both are kept.
    let es = titus(&[
        ["1:1", "es01", "figs-metaphor", "δοῦλος", "1", "a"],
        ["1:1", "es02", "figs-metaphor", "δοῦλος", "2", "b"],
    ]);
    let en = titus(&[["1:1", "en01", "figs-metaphor", "δοῦλος", "1", "note"]]);
    let mut stored: Vec<StoredDecision> = es.iter().map(|i| decided(i, &spanish())).collect();
    // Make the second decision match the same English check by place.
    stored[1].key.occurrence = 1;
    stored[1].key.check_id = tc_check::CheckId::parse("es02").unwrap();
    let p = plan(&en, &english(), &stored);
    for entry in &p.entries {
        assert!(
            matches!(
                entry.outcome,
                Outcome::Moves {
                    contested: true,
                    ..
                }
            ),
            "{entry:?}"
        );
    }
    let summary = p.summary(Some(&spanish()), no_text);
    assert_eq!(summary.contested, 2);

    // Opening the English list shows the later decision and keeps the other reachable.
    let decisions = projected(&stored, Some(&english()), no_text);
    let session = open(en.clone(), &decisions, Some(&english()), no_text);
    assert_eq!(session.placed, [vec![0, 1]]);
    assert_eq!(session.shown(0), Some(1));
    assert_eq!(session.contested().collect::<Vec<_>>(), [0]);
    assert!(session.off_list.is_empty());
}

fn set(lang: &str, org: &str, v: &str, books: &[&str]) -> LanguageSet {
    let mut s = LanguageSet::default();
    s.insert(
        Slot::TranslationNotes,
        pin(&format!("{org}/{lang}_tn"), v, books),
    );
    s.insert(
        Slot::TranslationWordsLinks,
        pin(&format!("{org}/{lang}_twl"), v, books),
    );
    s
}

fn stored_against(book: BookId, against: &ResourceRef, n: usize) -> Vec<StoredDecision> {
    let item = &titus(&[["1:1", "aaaa", "figs-metaphor", "δοῦλος", "1", "n"]])[0];
    (0..n)
        .map(|i| {
            let mut d = decided(item, against);
            d.key.check_id = tc_check::CheckId::parse(&format!("c{i}")).unwrap();
            d.key.target = tc_check::CheckTarget::Verse {
                book,
                chapter: tc_check::RefPart::from_number(1),
                verse: tc_check::RefPart::from_number(1),
            };
            d
        })
        .collect()
}

#[test]
fn a_gateway_change_counts_the_decisions_it_puts_at_risk() {
    // tC4 test/gatewayChange.test.ts.
    let es = || set("es-419", "Es-419_gl", "v66", &["TIT", "JON", "RUT"]);
    let fr = || set("fr", "Xenizo", "v2.0", &["TIT", "JON"]);
    let en = || {
        set(
            "en",
            "unfoldingWord",
            "v89",
            &["TIT", "JON", "HEB", "PSA", "RUT"],
        )
    };
    let pins = |primary: LanguageSet, fallback: LanguageSet| Pins {
        primary: Some(primary),
        fallback: Some(fallback),
    };
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource;
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource;
    let none = |_: &ResourceRef| None;
    let unit = |book, decisions| Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(book),
        decisions,
    };

    let same = stored_against(BookId::Titus, &en_tn, 12);
    let harmless = consequences(
        &pins(en(), en()),
        &pins(en(), en()),
        &[unit(BookId::Titus, &same)],
        none,
        no_articles,
    );
    assert!(harmless.harmless() && harmless.at_risk() == 0);

    let tit = stored_against(BookId::Titus, &es_tn, 30);
    let jon = stored_against(BookId::Jonah, &es_tn, 17);
    let c = consequences(
        &pins(es(), en()),
        &pins(fr(), en()),
        &[unit(BookId::Titus, &tit), unit(BookId::Jonah, &jon)],
        none,
        no_articles,
    );
    assert_eq!(c.affected.len(), 2);
    assert_eq!(c.affected[0].checked_against, es_tn);
    assert_eq!(c.at_risk(), 47);
    assert!(!c.blocked());

    let heb = stored_against(BookId::Hebrews, &en_tn, 25);
    let c = consequences(
        &pins(es(), en()),
        &pins(fr(), en()),
        &[unit(BookId::Hebrews, &heb), unit(BookId::Titus, &[])],
        none,
        no_articles,
    );
    assert!(c.harmless());
    assert_eq!(c.unaffected, 1);

    let fr_tit = || set("fr", "Xenizo", "v2.0", &["TIT"]);
    let rut = stored_against(BookId::Ruth, &es_tn, 3);
    let c = consequences(
        &pins(es(), en()),
        &pins(fr_tit(), fr_tit()),
        &[unit(BookId::Titus, &tit), unit(BookId::Ruth, &rut)],
        none,
        no_articles,
    );
    let blocked: Vec<Collection> = c
        .affected
        .iter()
        .filter(|a| a.blocked)
        .map(|a| a.collection)
        .collect();
    assert_eq!(blocked, [Collection::Book(BookId::Ruth)]);
    assert!(c.blocked());
}

#[test]
fn a_decision_a_carry_over_left_behind_is_not_put_at_risk_again() {
    // After a carry from Spanish to English, Titus holds decisions on en_tn and one
    // es-419_tn decision no English check took. It keeps its old resource.
    let es = || set("es-419", "Es-419_gl", "v66", &["TIT"]);
    let en = || set("en", "unfoldingWord", "v89", &["TIT"]);
    let fr = || set("fr", "Xenizo", "v2.0", &["TIT"]);
    let pins = |primary: LanguageSet| Pins {
        primary: Some(primary),
        fallback: Some(en()),
    };
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource;
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource;
    let mut titus_decisions = stored_against(BookId::Titus, &en_tn, 12);
    titus_decisions.extend(stored_against(BookId::Titus, &es_tn, 1));
    let units = [Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Titus),
        decisions: &titus_decisions,
    }];
    let none = |_: &ResourceRef| None;

    // A change that leaves Titus on en_tn is harmless.
    let c = consequences(&pins(en()), &pins(en()), &units, none, no_articles);
    assert!(c.harmless(), "{c:?}");
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 0, 1));

    // A change that moves Titus puts at risk only what holds now.
    let c = consequences(&pins(en()), &pins(fr()), &units, none, no_articles);
    assert_eq!(c.at_risk(), 12);
    assert_eq!(c.affected[0].checked_against, en_tn);
    assert_eq!((c.revived, c.detached), (0, 1));

    // Pinning Spanish again moves the twelve and brings the left-behind one back.
    let c = consequences(&pins(en()), &pins(es()), &units, none, no_articles);
    assert_eq!((c.at_risk(), c.revived, c.detached), (12, 1, 0));
}

#[test]
fn a_change_back_to_the_resource_a_decision_was_made_against_revives_it() {
    // Ruth resolves to es-419_tn and holds three decisions on it and two on en_tn left
    // from when it resolved to English. Pinning French, which lacks Ruth, puts it back
    // on English.
    let es = || set("es-419", "Es-419_gl", "v66", &["RUT"]);
    let fr = || set("fr", "Xenizo", "v2.0", &["TIT"]);
    let en = || set("en", "unfoldingWord", "v89", &["RUT"]);
    let pins = |primary: LanguageSet| Pins {
        primary: Some(primary),
        fallback: Some(en()),
    };
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource;
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource;
    let mut ruth = stored_against(BookId::Ruth, &es_tn, 3);
    ruth.extend(stored_against(BookId::Ruth, &en_tn, 2));
    let units = [Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Ruth),
        decisions: &ruth,
    }];
    let c = consequences(&pins(es()), &pins(fr()), &units, |_| None, no_articles);
    assert_eq!((c.at_risk(), c.revived, c.detached), (3, 2, 0));
    assert!(!c.blocked());
    // What the count says is what the decisions compute after the change.
    let after = pins(fr())
        .resolution(ToolId::TranslationNotes, Collection::Book(BookId::Ruth))
        .and_then(|a| a.resource().cloned())
        .unwrap();
    let valid_after = ruth
        .iter()
        .filter(|d| {
            assess(&d.decision, Some(&after), None)
                .invalidation
                .is_none()
        })
        .count();
    assert_eq!(valid_after, c.revived);
}

#[test]
fn a_change_onto_an_unusable_pin_is_blocked() {
    // The next primary pins tN for Titus with a tag and no commit: no rung would answer
    // Titus with a usable pin, so its decisions would match none.
    let en = || set("en", "unfoldingWord", "v89", &["TIT"]);
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource;
    let tag_only = LanguageSet::from_json(&serde_json::json!({
        "translationNotes": {
            "repoPath": "git.door43.org/Xenizo/fr_tn",
            "version": "v2.0",
            "books": ["TIT"]
        }
    }))
    .unwrap();
    assert_eq!(tag_only.unusable().len(), 1);
    let titus_decisions = stored_against(BookId::Titus, &en_tn, 2);
    let units = [Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Titus),
        decisions: &titus_decisions,
    }];
    let c = consequences(
        &Pins {
            primary: Some(en()),
            fallback: Some(en()),
        },
        &Pins {
            primary: Some(tag_only),
            fallback: Some(en()),
        },
        &units,
        |_| None,
        no_articles,
    );
    assert_eq!((c.at_risk(), c.blocked()), (2, true));
}

// ---- tW-only pin changes (tC4 #661) ---------------------------------------------------

/// The 2 Samuel Words list of en_twl v91, with the articles of en_tw v91 and of a "v90"
/// that still carried `kt/arcofthecovenant`, the article of the 2SA 6:2 link `vwrw`. No
/// real tW release in the fixtures carries it (en_tw v87 and v91 both lack it), so v90 is
/// built as v91 plus that one article.
struct Ark {
    links: Vec<tc_helps::WordLink>,
    before: BTreeSet<WordArticleId>,
    after: BTreeSet<WordArticleId>,
    twl: Pin,
    tw90: Pin,
    tw91: Pin,
}

impl Ark {
    fn new() -> Self {
        let after = parse_burrito_metadata(&resource("en_tw@v91/metadata.json"))
            .unwrap()
            .word_articles();
        let ark = WordArticleId::Dcs {
            category: WordCategory::KeyTerm,
            slug: Slug::parse("arcofthecovenant").unwrap(),
        };
        assert!(!after.contains(&ark));
        let mut before = after.clone();
        before.insert(ark);
        Self {
            links: word_links("en_tw@v91/2SA.tsv", BookId::SecondSamuel),
            before,
            after,
            twl: pin("unfoldingWord/en_twl", "v91", &["2SA"]),
            tw90: pin("unfoldingWord/en_tw", "v90", &[]),
            tw91: pin("unfoldingWord/en_tw", "v91", &[]),
        }
    }

    /// The Words checks of 2 Samuel, the list filtered by `articles`.
    fn items(&self, articles: &BTreeSet<WordArticleId>) -> Vec<CheckItem> {
        word_links_for_book(&self.links, Some(articles), &eng(BookId::SecondSamuel)).items
    }

    /// What the caller knows of each tW release's articles.
    fn articles(&self, r: &ResourceRef) -> Option<BTreeSet<WordArticleId>> {
        if r.same_resource(&self.tw90.resource) {
            Some(self.before.clone())
        } else if r.same_resource(&self.tw91.resource) {
            Some(self.after.clone())
        } else {
            None
        }
    }

    /// `vwrw`, whose article v91 lacks, and the first check of the list, whose article
    /// stays, both decided against `against`.
    fn decisions(&self, against: &ResourceRef) -> Vec<StoredDecision> {
        let items = self.items(&self.before);
        let vwrw = items
            .iter()
            .find(|i| i.key.check_id.as_str() == "vwrw")
            .unwrap();
        assert_ne!(items[0].key, vwrw.key);
        let stored = vec![decided(vwrw, against), decided(&items[0], against)];
        assert_eq!(
            (
                stored[0].decision.category.as_deref(),
                stored[0].group_id.as_str()
            ),
            (Some("kt"), "arcofthecovenant")
        );
        stored
    }
}

/// No article set is known: no Words list is filtered.
fn no_articles(_: &ResourceRef) -> Option<BTreeSet<WordArticleId>> {
    None
}

/// A language set with these pins.
fn set_of(pins: &[(Slot, &Pin)]) -> LanguageSet {
    let mut s = LanguageSet::default();
    for (slot, pin) in pins {
        s.insert(*slot, (*pin).clone());
    }
    s
}

/// Both rungs hold `set`.
fn both(set: LanguageSet) -> Pins {
    Pins {
        primary: Some(set.clone()),
        fallback: Some(set),
    }
}

fn words(book: BookId, decisions: &[StoredDecision]) -> Unit<'_> {
    Unit {
        tool: ToolId::TranslationWords,
        collection: Collection::Book(book),
        decisions,
    }
}

#[test]
fn a_tw_only_change_that_drops_a_decided_article_is_counted() {
    let ark = Ark::new();
    let (before, after) = (ark.items(&ark.before), ark.items(&ark.after));
    assert_eq!((before.len(), after.len()), (3316, 3315));
    let stored = ark.decisions(&ark.twl.resource);
    let units = [words(BookId::SecondSamuel, &stored)];
    let pins = |tw: &Pin| {
        both(set_of(&[
            (Slot::TranslationWordsLinks, &ark.twl),
            (Slot::TranslationWords, tw),
        ]))
    };
    let articles = |r: &ResourceRef| ark.articles(r);

    // en_tw v90 to v91, en_twl unchanged: vwrw's check leaves the list.
    let c = consequences(
        &pins(&ark.tw90),
        &pins(&ark.tw91),
        &units,
        |_| None,
        articles,
    );
    assert_eq!(
        c.affected,
        [Affected {
            tool: ToolId::TranslationWords,
            collection: Collection::Book(BookId::SecondSamuel),
            checked_against: ark.twl.resource.clone(),
            decisions: 1,
            articles_left: 1,
            blocked: false,
        }]
    );
    assert_eq!((c.unaffected, c.revived, c.detached), (0, 0, 0));
    assert!(!c.harmless());

    // Nothing moves: nothing is counted.
    let same = consequences(
        &pins(&ark.tw90),
        &pins(&ark.tw90),
        &units,
        |_| None,
        articles,
    );
    assert!(same.harmless());
    assert_eq!(same.unaffected, 1);

    // The carry-over on the new list: vwrw finds no check, the other decision stays, and
    // the dialog's two numbers agree.
    let p = plan(&after, &ark.twl.resource, &stored);
    assert_eq!(p.entries[0].outcome, Outcome::Unplaced);
    assert!(matches!(
        p.entries[1].outcome,
        Outcome::Stays {
            by: Match::Identity,
            ..
        }
    ));
    let summary = p.summary(Some(&ark.twl.resource), no_text);
    assert_eq!((summary.carried, summary.invalidated), (1, c.at_risk()));

    // Kept, not lost: nothing rewrites the decision, and the session lists it off the list.
    let decisions = projected(&stored, Some(&ark.twl.resource), no_text);
    let session = open(after, &decisions, Some(&ark.twl.resource), no_text);
    assert_eq!(session.off_list, [0]);
    assert_eq!(session.placed.iter().flatten().collect::<Vec<_>>(), [&1]);
    // If a later release carries the article again, the decision is placed on its check
    // again, as it was.
    let back = open(before, &decisions, Some(&ark.twl.resource), no_text);
    assert!(back.off_list.is_empty());
    assert!(back.placed.iter().any(|on| on.contains(&0)));
}

#[test]
fn an_article_change_is_judged_by_the_set_that_answers_the_list() {
    let ark = Ark::new();
    let articles = |r: &ResourceRef| ark.articles(r);
    let en = |twl: &Pin, tw: &Pin| {
        set_of(&[
            (Slot::TranslationWordsLinks, twl),
            (Slot::TranslationWords, tw),
        ])
    };

    // The new release's articles are not known here: the list would be derived unfiltered.
    let stored = ark.decisions(&ark.twl.resource);
    let units = [words(BookId::SecondSamuel, &stored)];
    let unknown = pin("unfoldingWord/en_tw", "v92", &[]);
    let c = consequences(
        &both(en(&ark.twl, &ark.tw90)),
        &both(en(&ark.twl, &unknown)),
        &units,
        |_| None,
        articles,
    );
    assert!(c.harmless(), "{c:?}");

    // en_twl and en_tw both change: every decision is at risk, as before #661, and the one
    // whose article leaves is counted once.
    let twl90 = pin("unfoldingWord/en_twl", "v90", &["2SA"]);
    let stored90 = ark.decisions(&twl90.resource);
    let c = consequences(
        &both(en(&twl90, &ark.tw90)),
        &both(en(&ark.twl, &ark.tw91)),
        &[words(BookId::SecondSamuel, &stored90)],
        |_| None,
        articles,
    );
    assert_eq!(c.at_risk(), 2);
    assert_eq!(c.affected[0].articles_left, 1);

    // The primary answers 2 Samuel with its own links and words; only the fallback's
    // translation words change, and they filter no list here.
    let es_twl = pin("Es-419_gl/es-419_twl", "v1", &["2SA"]);
    let c = consequences(
        &Pins {
            primary: Some(en(&ark.twl, &ark.tw90)),
            fallback: Some(en(&es_twl, &ark.tw90)),
        },
        &Pins {
            primary: Some(en(&ark.twl, &ark.tw90)),
            fallback: Some(en(&es_twl, &ark.tw91)),
        },
        &units,
        |_| None,
        articles,
    );
    assert!(c.harmless(), "{c:?}");

    // A decision with no recorded category is on the slug in any category.
    let mut uncategorized = stored.clone();
    uncategorized[0].decision.category = None;
    let c = consequences(
        &both(en(&ark.twl, &ark.tw90)),
        &both(en(&ark.twl, &ark.tw91)),
        &[words(BookId::SecondSamuel, &uncategorized)],
        |_| None,
        articles,
    );
    assert_eq!((c.at_risk(), c.affected[0].articles_left), (1, 1));
}

#[test]
fn a_decision_comes_back_only_when_its_check_is_on_the_new_list() {
    let ark = Ark::new();
    let articles = |r: &ResourceRef| ark.articles(r);
    let en = |twl: &Pin, tw: &Pin| {
        both(set_of(&[
            (Slot::TranslationWordsLinks, twl),
            (Slot::TranslationWords, tw),
        ]))
    };
    // vwrw and a check whose article stays, decided against en_twl v90, which 2 Samuel
    // resolves to only after the change.
    let twl90 = pin("unfoldingWord/en_twl", "v90", &["2SA"]);
    let stored = ark.decisions(&twl90.resource);
    let units = [words(BookId::SecondSamuel, &stored)];
    let decisions = projected(&stored, Some(&twl90.resource), no_text);

    // en_twl goes back to v90 and en_tw moves to v91, which lacks vwrw's article: only the
    // other decision comes back, and the session lists vwrw off the list.
    let c = consequences(
        &en(&ark.twl, &ark.tw90),
        &en(&twl90, &ark.tw91),
        &units,
        |_| None,
        articles,
    );
    assert!(c.harmless(), "{c:?}");
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 1, 1));
    let session = open(
        ark.items(&ark.after),
        &decisions,
        Some(&twl90.resource),
        no_text,
    );
    assert_eq!(session.off_list, [0]);
    assert_eq!(session.placed.iter().flatten().collect::<Vec<_>>(), [&1]);

    // The same when en_tw stays at v91: its pin does not move, and the list still decides.
    let c = consequences(
        &en(&ark.twl, &ark.tw91),
        &en(&twl90, &ark.tw91),
        &units,
        |_| None,
        articles,
    );
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 1, 1));

    // On a list that keeps vwrw's article, both come back.
    let c = consequences(
        &en(&ark.twl, &ark.tw90),
        &en(&twl90, &ark.tw90),
        &units,
        |_| None,
        articles,
    );
    assert_eq!((c.revived, c.detached), (2, 0));
    let session = open(
        ark.items(&ark.before),
        &decisions,
        Some(&twl90.resource),
        no_text,
    );
    assert!(session.off_list.is_empty());

    // A change of only en_tw that brings vwrw's article back puts it on its check again.
    let stored = ark.decisions(&ark.twl.resource);
    let c = consequences(
        &en(&ark.twl, &ark.tw91),
        &en(&ark.twl, &ark.tw90),
        &[words(BookId::SecondSamuel, &stored)],
        |_| None,
        articles,
    );
    assert!(c.harmless(), "{c:?}");
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 1, 0));
}

#[test]
fn a_change_onto_an_unusable_translation_words_pin_is_blocked() {
    // en_twl v91 answers 2 Samuel on both sides; en_tw moves from v90 to a tag with no
    // commit, which the tool refuses (UnusablePin), so the decided checks become
    // unreachable.
    let ark = Ark::new();
    let articles = |r: &ResourceRef| ark.articles(r);
    let mut broken = LanguageSet::from_json(&serde_json::json!({
        "translationWords": {
            "repoPath": "git.door43.org/unfoldingWord/en_tw",
            "version": "v92"
        }
    }))
    .unwrap();
    assert_eq!(broken.unusable().len(), 1);
    broken.set_pin(Slot::TranslationWordsLinks, &ark.twl);
    let working = set_of(&[
        (Slot::TranslationWordsLinks, &ark.twl),
        (Slot::TranslationWords, &ark.tw90),
    ]);
    let stored = ark.decisions(&ark.twl.resource);
    let units = [words(BookId::SecondSamuel, &stored)];
    let offline = Machine {
        installed: &[],
        online: false,
    };
    let opens = |pins: &Pins| {
        preflight(
            Some(pins),
            ToolId::TranslationWords,
            Collection::Book(BookId::SecondSamuel),
            &offline,
        )
        .state
    };
    assert_eq!(opens(&both(broken.clone())), PreflightState::UnusablePin);

    let c = consequences(
        &both(working.clone()),
        &both(broken.clone()),
        &units,
        |_| None,
        articles,
    );
    assert_eq!(
        c.affected,
        [Affected {
            tool: ToolId::TranslationWords,
            collection: Collection::Book(BookId::SecondSamuel),
            checked_against: ark.twl.resource.clone(),
            decisions: 2,
            articles_left: 0,
            blocked: true,
        }]
    );
    assert!(c.blocked());

    // A pin that was already unusable does not refuse a change that leaves it alone.
    let mut with_notes = broken.clone();
    with_notes.set_pin(
        Slot::TranslationNotes,
        &pin("unfoldingWord/en_tn", "v91", &["2SA"]),
    );
    let c = consequences(
        &both(broken.clone()),
        &both(with_notes),
        &units,
        |_| None,
        articles,
    );
    assert!(c.harmless() && !c.blocked(), "{c:?}");

    // Repairing it opens the list again: both decisions are back on their checks.
    let c = consequences(&both(broken), &both(working), &units, |_| None, articles);
    assert!(c.harmless(), "{c:?}");
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 2, 0));
}

#[test]
fn article_sets_are_read_once_each_and_only_when_a_check_depends_on_them() {
    let ark = Ark::new();
    let reads = Cell::new(0);
    let articles = |r: &ResourceRef| {
        reads.set(reads.get() + 1);
        ark.articles(r)
    };
    let books = ["TIT", "2SA", "GEN", "EXO"];
    let tn = pin("unfoldingWord/en_tn", "v91", &books);
    let tn92 = pin("unfoldingWord/en_tn", "v92", &books);
    let twl = pin("unfoldingWord/en_twl", "v91", &books);
    let twl90 = pin("unfoldingWord/en_twl", "v90", &books);
    assert!(twl.resource.same_resource(&ark.twl.resource));
    let sets = |tn: &Pin, twl: &Pin, tw: &Pin| {
        both(set_of(&[
            (Slot::TranslationNotes, tn),
            (Slot::TranslationWordsLinks, twl),
            (Slot::TranslationWords, tw),
        ]))
    };
    // Three books' Words lists filtered by the same translation words, and a Notes list.
    let decided_words = ark.decisions(&ark.twl.resource);
    let decided_notes = stored_against(BookId::Titus, &tn.resource, 3);
    let notes_unit = Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Titus),
        decisions: &decided_notes,
    };
    let units = [
        words(BookId::SecondSamuel, &decided_words),
        words(BookId::Genesis, &decided_words),
        words(BookId::Exodus, &decided_words),
        notes_unit,
    ];

    // The translation words pin moves: each side's set is read once for the three Words
    // units, and none for the Notes unit.
    let c = consequences(
        &sets(&tn, &twl, &ark.tw90),
        &sets(&tn, &twl, &ark.tw91),
        &units,
        |_| None,
        articles,
    );
    assert_eq!((c.at_risk(), reads.get()), (3, 2));

    // A change of the links or the notes alone reads no article set.
    reads.set(0);
    let decided_words90 = ark.decisions(&twl90.resource);
    let units90 = [words(BookId::SecondSamuel, &decided_words90), notes_unit];
    let c = consequences(
        &sets(&tn, &twl90, &ark.tw90),
        &sets(&tn92, &twl, &ark.tw90),
        &units90,
        |_| None,
        articles,
    );
    assert_eq!((c.at_risk(), reads.get()), (5, 0));

    // A change back to the links the decisions were made against reads the new list's set,
    // once, though its pin does not move.
    let units90 = [
        words(BookId::SecondSamuel, &decided_words90),
        words(BookId::Genesis, &decided_words90),
    ];
    let c = consequences(
        &sets(&tn, &twl, &ark.tw91),
        &sets(&tn, &twl90, &ark.tw91),
        &units90,
        |_| None,
        articles,
    );
    assert_eq!((c.revived, c.detached, reads.get()), (2, 2, 1));
}

// ---- properties ----------------------------------------------------------------------

/// A small universe so that ids, places, quotes and groups collide often.
fn arb_row() -> impl Strategy<Value = [String; 6]> {
    (
        prop::sample::select(vec!["1:1", "1:2", "2:1"]),
        prop::sample::select(vec!["aaaa", "bbbb", "cccc", "dddd"]),
        prop::sample::select(vec!["figs-metaphor", "figs-idiom"]),
        prop::sample::select(vec!["δοῦλος", "δοῦλος,", "ἐλπίδι", "δοῦλος & Θεοῦ"]),
        prop::sample::select(vec!["1", "2"]),
    )
        .prop_map(|(r, id, g, q, o)| {
            [
                r.into(),
                id.into(),
                g.into(),
                q.into(),
                o.into(),
                "n".into(),
            ]
        })
}

fn items_of(rows: &[[String; 6]]) -> Vec<CheckItem> {
    let rows: Vec<[&str; 6]> = rows
        .iter()
        .map(|r| [&*r[0], &*r[1], &*r[2], &*r[3], &*r[4], &*r[5]])
        .collect();
    titus(&rows)
}

fn decision_of(
    entry_item: &CheckItem,
    d: &StoredDecision,
    resource: &ResourceRef,
) -> StoredDecision {
    StoredDecision {
        key: entry_item.key.clone(),
        quote: entry_item.quote.as_str().to_owned(),
        group_id: entry_item.group_id.clone(),
        decision: DecisionRecord {
            resource: resource.clone(),
            ..d.decision.clone()
        },
    }
}

proptest! {
    #[test]
    fn no_decision_is_lost_and_placements_are_consistent(
        old_rows in prop::collection::vec(arb_row(), 0..8),
        new_rows in prop::collection::vec(arb_row(), 0..8),
    ) {
        let old = items_of(&old_rows);
        let new = items_of(&new_rows);
        let stored: Vec<StoredDecision> = old.iter().map(|i| decided(i, &spanish())).collect();
        let p = plan(&new, &english(), &stored);

        // Every stored decision has exactly one entry, in order.
        prop_assert_eq!(p.entries.len(), stored.len());
        for (entry, d) in p.entries.iter().zip(&stored) {
            prop_assert_eq!(&entry.key, &d.key);
        }
        let mut arrivals = vec![0usize; new.len()];
        for (entry, d) in p.entries.iter().zip(&stored) {
            match &entry.outcome {
                Outcome::Stays { item, .. } => {
                    prop_assert_eq!(&new[*item].key, &d.key);
                    arrivals[*item] += 1;
                }
                Outcome::Moves { item, to, .. } => {
                    prop_assert_eq!(&new[*item].key, to);
                    prop_assert_ne!(to, &d.key);
                    arrivals[*item] += 1;
                }
                Outcome::Unplaced => {}
            }
        }
        // Two decisions on one check are always flagged, never silently merged.
        for (entry, _) in p.entries.iter().zip(&stored) {
            if let Outcome::Moves { item, contested, .. } = entry.outcome
                && arrivals[item] > 1
            {
                prop_assert!(contested);
            }
        }
        prop_assert_eq!(p.undecided, arrivals.iter().filter(|n| **n == 0).count());

        // A session on the new list reaches every stored decision exactly once: placed
        // on one check, or listed off the list.
        let decisions = projected(&stored, Some(&english()), no_text);
        let session = open(new.clone(), &decisions, Some(&english()), no_text);
        let mut reached: Vec<usize> = session.placed.iter().flatten().copied().collect();
        reached.extend(&session.off_list);
        reached.sort_unstable();
        prop_assert_eq!(reached, (0..stored.len()).collect::<Vec<_>>());
        for (item, on) in session.placed.iter().enumerate() {
            prop_assert_eq!(on.len(), arrivals[item]);
            prop_assert_eq!(session.shown(item), on.last().copied());
        }

        // Writing the plan and planning again moves no carried decision a second time.
        let uncontested = p.entries.iter().all(|e| !matches!(e.outcome, Outcome::Moves { contested: true, .. }));
        let unique_arrivals = arrivals.iter().all(|n| *n <= 1);
        if uncontested && unique_arrivals {
            let mut after: Vec<StoredDecision> = Vec::new();
            for (entry, d) in p.entries.iter().zip(&stored) {
                match &entry.outcome {
                    Outcome::Stays { item, .. } | Outcome::Moves { item, .. } => {
                        after.push(decision_of(&new[*item], d, &english()));
                    }
                    Outcome::Unplaced => after.push(d.clone()),
                }
            }
            let again = plan(&new, &english(), &after);
            for (entry, first) in again.entries.iter().zip(&p.entries) {
                if !matches!(first.outcome, Outcome::Unplaced) {
                    let stays_by_identity = matches!(entry.outcome, Outcome::Stays { by: Match::Identity, .. });
                    prop_assert!(stays_by_identity, "{:?}", entry);
                }
            }
        }
    }

    #[test]
    fn a_carried_decision_keeps_its_judgement_and_holds_against_the_new_resource(
        rows in prop::collection::vec(arb_row(), 1..6),
    ) {
        let items = items_of(&rows);
        let stored: Vec<StoredDecision> = items.iter().map(|i| decided(i, &spanish())).collect();
        let p = plan(&items, &english(), &stored);
        for (entry, d) in p.entries.iter().zip(&stored) {
            if let Outcome::Stays { item, .. } = entry.outcome {
                let after = decision_of(&items[item], d, &english());
                prop_assert_eq!(&after.decision.judgement, &d.decision.judgement);
                let ho = t("ho");
                let state = CheckState::of(Some(&after.decision), Some(&english()), Some(&ho));
                prop_assert!(state.invalidation().is_none());
                // Before the change is written, the old decision computes as invalid
                // against the new resolution, and is kept.
                let before = CheckState::of(Some(&d.decision), Some(&english()), Some(&ho));
                prop_assert!(before.invalidation().is_some_and(|i| i.resource_changed));
            }
        }
    }
}
