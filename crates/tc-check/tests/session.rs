//! Opening a check session: the sample project's Titus tN decisions on the es-419_tn v66
//! list, compared with what tC4's own session merge (`mergeAndReattach` in
//! `completedCheckSession`, run on the same files) shows.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{eng, file_resource, notes, projected, sample, stored_decisions, target_text};
use serde_json::Value;
use tc_check::derive::notes_for_book;
use tc_check::rail::{Filter, Progress, counts};
use tc_check::session::open;
use tc_check::{CheckTarget, Dot, TargetText};
use tc_core::BookId;

#[test]
fn the_sample_decisions_show_where_tc4_shows_them() {
    let items = notes_for_book(
        &notes("es-419_tn@v66/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    )
    .items;
    let usfm = sample("TIT.usfm");
    let file: Value = serde_json::from_str(&sample("checking/translationNotes/TIT.json")).unwrap();
    let stored = stored_decisions(&file, BookId::Titus, &usfm);
    let resource = file_resource(&file);
    let texts: Vec<(CheckTarget, Option<TargetText>)> = items
        .iter()
        .map(|i| i.key.target.clone())
        .chain(stored.iter().map(|s| s.key.target.clone()))
        .map(|t| {
            let text = target_text(&usfm, &t);
            (t, text)
        })
        .collect();
    let lookup = |t: &CheckTarget| {
        texts
            .iter()
            .find(|(target, _)| target == t)
            .and_then(|(_, text)| text.as_ref())
    };
    let decisions = projected(&stored, Some(&resource), lookup);
    let session = open(items, &decisions, Some(&resource), lookup);

    // tC4: swi9's decision (1:1, κατὰ πίστιν) shows on rtc9, and gr8c's (1:4, χάρις καὶ
    // εἰρήνη) on h93t, still invalidated; progress 1 of 112.
    let shown: Vec<(String, String)> = (0..session.items.len())
        .filter_map(|i| {
            session.shown(i).map(|s| {
                (
                    session.items[i].key.check_id.to_string(),
                    stored[s].key.check_id.to_string(),
                )
            })
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("rtc9".to_owned(), "swi9".to_owned()),
            ("h93t".to_owned(), "gr8c".to_owned())
        ]
    );
    assert_eq!(
        session.progress(),
        Progress {
            decided: 1,
            total: 112
        }
    );
    assert!(session.off_list.is_empty() && session.contested().next().is_none());
    let at = |id: &str| {
        session
            .items
            .iter()
            .position(|i| i.key.check_id.as_str() == id)
            .unwrap()
    };
    assert_eq!(session.states[at("rtc9")].dot(), Dot::Todo);
    assert!(session.states[at("rtc9")].commented);
    assert_eq!(session.states[at("h93t")].dot(), Dot::Invalid);
    let chips: Vec<usize> = counts(&session.states).iter().map(|(_, n)| *n).collect();
    assert_eq!(chips, [112, 111, 1, 0, 1]);
    assert_eq!(Filter::ORDER.len(), chips.len());
}

#[test]
fn a_decision_no_check_carries_is_listed_not_lost() {
    let items = notes_for_book(
        &notes("en_tn@v86/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    )
    .items;
    let usfm = sample("TIT.usfm");
    let file: Value = serde_json::from_str(&sample("checking/translationNotes/TIT.json")).unwrap();
    let stored = stored_decisions(&file, BookId::Titus, &usfm);
    let resource = file_resource(&file);
    let decisions = projected(&stored, Some(&resource), |_| None);
    let session = open(items, &decisions, Some(&resource), |_| None);
    // On the English list, κατὰ πίστιν alone is no check of 1:1 (en rtc9 quotes a longer
    // span) and en swi9 is another check, so swi9's decision is kept and listed.
    let off: Vec<String> = session
        .off_list
        .iter()
        .map(|&s| stored[s].key.check_id.to_string())
        .collect();
    assert_eq!(off, ["swi9"]);
    let placed: usize = session.placed.iter().map(Vec::len).sum();
    assert_eq!(placed + session.off_list.len(), stored.len());
}
