//! Carry-over plans and the counts the upgrade and gateway-change dialogs show, from
//! tC4's suites (`carryOver`, `derive-full-strength`, `twArticleGate`, `gatewayChange`)
//! and properties that no decision is ever dropped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{eng, notes, pin, resource, word_links};
use proptest::prelude::*;
use tc_check::carry::{Match, Outcome, StoredDecision, Summary, plan, same_quote};
use tc_check::change::{Unit, consequences};
use tc_check::derive::{notes_for_book, word_links_for_book};
use tc_check::resolve::LanguageSet;
use tc_check::session::open;
use tc_check::status::{CheckState, Edit, Triage, apply, assess};
use tc_check::{
    CheckItem, Collection, Decision, Pins, ResourceRef, Selection, Slot, TextMd5, ToolId,
};
use tc_core::BookId;
use tc_helps::tsv::{TsvScope, parse_notes};

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
    }];
    StoredDecision {
        key: item.key.clone(),
        quote: item.quote.as_str().to_owned(),
        group_id: item.group_id.clone(),
        decision: apply(None, Edit::Triage(Triage::Valid(words)), "ho", against),
    }
}

fn spanish() -> ResourceRef {
    pin("Es-419_gl/es-419_tn", "v66", &[]).resource
}

fn english() -> ResourceRef {
    pin("unfoldingWord/en_tn", "v89", &[]).resource
}

fn no_text(_: &tc_check::CheckTarget) -> Option<&'static str> {
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
    stored[1].decision.judgement.verse_text_md5 = TextMd5::of("ho hum");
    let verse = |t: &tc_check::CheckTarget| -> Option<&'static str> {
        Some(if t.locator() == "1:2" { "hum" } else { "ho" })
    };
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
    let verse = |t: &tc_check::CheckTarget| -> Option<&'static str> {
        Some(if t.locator() == "1:2" { "ho" } else { "hum" })
    };
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
    let session = open(en.clone(), &stored, Some(&english()), no_text);
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
                chapter: tc_check::RefPart::number(1),
                verse: tc_check::RefPart::number(1),
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
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource.clone();
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource.clone();
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
    );
    assert!(harmless.harmless() && harmless.at_risk() == 0);

    let tit = stored_against(BookId::Titus, &es_tn, 30);
    let jon = stored_against(BookId::Jonah, &es_tn, 17);
    let c = consequences(
        &pins(es(), en()),
        &pins(fr(), en()),
        &[unit(BookId::Titus, &tit), unit(BookId::Jonah, &jon)],
        none,
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
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource.clone();
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource.clone();
    let mut titus_decisions = stored_against(BookId::Titus, &en_tn, 12);
    titus_decisions.extend(stored_against(BookId::Titus, &es_tn, 1));
    let units = [Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Titus),
        decisions: &titus_decisions,
    }];
    let none = |_: &ResourceRef| None;

    // A change that leaves Titus on en_tn is harmless.
    let c = consequences(&pins(en()), &pins(en()), &units, none);
    assert!(c.harmless(), "{c:?}");
    assert_eq!((c.unaffected, c.revived, c.detached), (1, 0, 1));

    // A change that moves Titus puts at risk only what holds now.
    let c = consequences(&pins(en()), &pins(fr()), &units, none);
    assert_eq!(c.at_risk(), 12);
    assert_eq!(c.affected[0].checked_against, en_tn);
    assert_eq!((c.revived, c.detached), (0, 1));

    // Pinning Spanish again moves the twelve and brings the left-behind one back.
    let c = consequences(&pins(en()), &pins(es()), &units, none);
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
    let es_tn = es().get(Slot::TranslationNotes).unwrap().resource.clone();
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource.clone();
    let mut ruth = stored_against(BookId::Ruth, &es_tn, 3);
    ruth.extend(stored_against(BookId::Ruth, &en_tn, 2));
    let units = [Unit {
        tool: ToolId::TranslationNotes,
        collection: Collection::Book(BookId::Ruth),
        decisions: &ruth,
    }];
    let c = consequences(&pins(es()), &pins(fr()), &units, |_| None);
    assert_eq!((c.at_risk(), c.revived, c.detached), (3, 2, 0));
    assert!(!c.blocked());
    // What the count says is what the decisions compute after the change.
    let after = tc_check::resolve::resolve(
        &pins(fr()),
        ToolId::TranslationNotes,
        Collection::Book(BookId::Ruth),
    )
    .unwrap()
    .pin
    .resource;
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
    let en_tn = en().get(Slot::TranslationNotes).unwrap().resource.clone();
    let tag_only = LanguageSet::from_json(&serde_json::json!({
        "translationNotes": {
            "repoPath": "git.door43.org/Xenizo/fr_tn",
            "version": "v2.0",
            "books": ["TIT"]
        }
    }))
    .unwrap();
    assert_eq!(tag_only.unusable().count(), 1);
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
    );
    assert_eq!((c.at_risk(), c.blocked()), (2, true));
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
        decision: Decision {
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
        let session = open(new.clone(), &stored, Some(&english()), no_text);
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
                let state = CheckState::of(Some(&after.decision), Some(&english()), Some("ho"));
                prop_assert!(state.invalidation().is_none());
                // Before the change is written, the old decision computes as invalid
                // against the new resolution, and is kept.
                let before = CheckState::of(Some(&d.decision), Some(&english()), Some("ho"));
                prop_assert!(before.invalidation().is_some_and(|i| i.resource_changed));
            }
        }
    }
}
