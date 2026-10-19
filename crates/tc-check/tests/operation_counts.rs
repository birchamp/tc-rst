//! Operation counts for the Check screen's steps (`docs/PERFORMANCE.md` rule 2, rows
//! "Checking"): each step costs what it touches, on the real Psalms tN and TWL lists and
//! on the decisions of the scale corpus, at the size of Psalms and ten times it.
//!
//! The rail's grouping and a carry-over's second pass each had a cost that grew with the
//! product of two sizes (the groups with the rows, the cross-language pass with the checks
//! it scanned for every decision). These tests pin the counts the indexed versions make, so
//! the product cannot come back unseen.

// Helpers build fixed, known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use common::{eng, fixture, tw_articles};
use proptest::prelude::*;
use tc_check::carry::{StoredDecision, plan_counted};
use tc_check::change::{Unit, consequences_counted};
use tc_check::derive::{CheckList, notes_for_book, word_links_for_book};
use tc_check::rail::{Filter, GroupLabel, RailGroup, SortMode, groups_counted, progress};
use tc_check::session::{Session, open};
use tc_check::{
    Answer, CheckItem, CheckState, CheckTarget, Collection, Ladder, LanguageSets, Pin, ResourceRef,
    Rung, SetSlot, SlotPin, TargetText, ToolId, Unrecorded, Work,
};
use tc_core::{BookId, GitOid};
use tc_corpus::{Corpus, Spec};
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind, ProjectedDecision};
use tc_journal::{PinGroup, Record, RecordKey};

const PSALMS: BookId = BookId::Psalms;
/// The verses of Psalms.
const PSALMS_VERSES: usize = 2_461;
const TN: ToolId = ToolId::TranslationNotes;
const TW: ToolId = ToolId::TranslationWords;

/// The Psalms tN and TWL check lists, derived as the Check screen derives them.
fn lists() -> &'static (CheckList, CheckList) {
    static LISTS: OnceLock<(CheckList, CheckList)> = OnceLock::new();
    LISTS.get_or_init(|| {
        let scope = TsvScope::Book(PSALMS);
        let spec = eng(PSALMS);
        let notes = parse_notes(&fixture("scale/psa/en_tn/tn_PSA.tsv"), scope)
            .unwrap()
            .notes;
        let links = parse_word_links(&fixture("scale/psa/en_twl/twl_PSA.tsv"), scope).unwrap();
        (
            notes_for_book(&notes, &spec),
            word_links_for_book(&links, Some(&tw_articles()), &spec),
        )
    })
}

fn list(tool: ToolId) -> &'static CheckList {
    if tool == TN { &lists().0 } else { &lists().1 }
}

fn fold_corpus(spec: &Spec) -> FoldState {
    let corpus = Corpus::generate(spec).expect("the corpus builds");
    let lines = corpus.journal.lines();
    FoldState::fold(
        ProjectKind::Bible,
        lines.into_iter().map(|l| FoldInput::from(l).prepare()),
    )
}

fn psalms() -> &'static FoldState {
    static STATE: OnceLock<FoldState> = OnceLock::new();
    STATE.get_or_init(|| fold_corpus(&Spec::psalms(1)))
}

fn larger() -> &'static FoldState {
    static STATE: OnceLock<FoldState> = OnceLock::new();
    STATE.get_or_init(|| fold_corpus(&Spec::scaled(1, 10)))
}

/// A snapshot of `state` that counts its own work.
fn fork(state: &FoldState) -> FoldState {
    let mut fork = state.clone();
    fork.own_counters();
    fork
}

/// The project's pins as the ladder reads them.
fn ladder(fold: &FoldState) -> LanguageSets {
    let primary = match fold.projected(&RecordKey::Pins(PinGroup::Primary)) {
        Some(Record::Pins { value, .. }) => Some(value),
        _ => None,
    };
    LanguageSets {
        primary,
        fallback: None,
    }
}

fn resolution(sets: &LanguageSets, tool: ToolId, book: BookId) -> Option<ResourceRef> {
    sets.answer(tool, Collection::Book(book), Unrecorded::CoversEveryBook)
        .and_then(|a: Answer| a.resource().cloned())
}

/// The text of each distinct target of `items`, as the fold reads it (§7.7), read once.
fn texts_of(fold: &FoldState, items: &[CheckItem]) -> HashMap<CheckTarget, Arc<TargetText>> {
    let texts = fold.check_texts(Collection::Book(PSALMS));
    let mut out = HashMap::new();
    for item in items {
        if !out.contains_key(&item.key.target)
            && let Some(text) = texts.text(&item.key.target)
        {
            out.insert(item.key.target.clone(), text);
        }
    }
    out
}

/// What opening a session on `tool`'s Psalms list took: the fold's counters, the
/// session, the decisions and how many times the text was asked for.
struct Opened {
    session: Session,
    decisions: Vec<ProjectedDecision>,
    resolution: Option<ResourceRef>,
    read: tc_journal::fold::Counters,
    asked: usize,
}

fn opened(state: &FoldState, tool: ToolId) -> Opened {
    opened_on(state, tool, list(tool).items.clone())
}

fn opened_on(state: &FoldState, tool: ToolId, items: Vec<CheckItem>) -> Opened {
    let fold = fork(state);
    let sets = ladder(&fold);
    let resolution = resolution(&sets, tool, PSALMS);
    let texts = texts_of(&fold, &items);
    let before = fold.counters();
    let decisions = fold.decisions(tool, Collection::Book(PSALMS));
    let read = fold.counters().since(&before);
    let asked = Cell::new(0usize);
    let session = open(items, &decisions, resolution.as_ref(), |t| {
        asked.set(asked.get() + 1);
        texts.get(t).map(|a| &**a)
    });
    Opened {
        session,
        decisions,
        resolution,
        read,
        asked: asked.get(),
    }
}

#[test]
fn opening_a_psalms_session_assesses_each_check_once() {
    for tool in [TN, TW] {
        for (name, state) in [("psalms", psalms()), ("larger", larger())] {
            let o = opened(state, tool);
            let checks = o.session.items.len();
            let decisions = o.decisions.len();
            eprintln!(
                "{tool:?} on {name}: {checks} checks, {decisions} decisions, {} asked, read {:?}",
                o.asked, o.read
            );
            assert!(decisions > 0, "the corpus decides some Psalms checks");
            // A decision on the check it is stored under shows the state the fold computed;
            // the text is asked for only where a decision is shown under another key, and
            // then once per check.
            assert!(
                o.asked <= checks,
                "{name}: {} asks for {checks} checks",
                o.asked
            );
            // Every stored decision is placed on one check or listed off the list.
            let placed: usize = o.session.placed.iter().map(Vec::len).sum();
            assert_eq!(placed + o.session.off_list.len(), decisions, "{name}");
            assert_eq!(o.session.states.len(), checks);
            // Reading the decisions costs the tool's decisions and the book's verses, once
            // each: not the other records of the project, and not the other tool's.
            let bound = (decisions + PSALMS_VERSES + 150 + 8) as u64;
            assert!(o.read.records_projected <= bound, "{name}: {:?}", o.read);
            assert!(o.read.values_decoded <= bound, "{name}: {:?}", o.read);
        }
        // A list in which some checks sit on another verse than their decision's: the
        // decision is shown on the check it moves to, assessed against that check's text,
        // which is asked for once for the check and not for the project.
        for (name, state) in [("psalms", psalms()), ("larger", larger())] {
            let moved = shifted(&list(tool).items);
            let o = opened_on(state, tool, moved);
            let shown_elsewhere = (0..o.session.items.len())
                .filter(|&i| {
                    o.session
                        .shown(i)
                        .is_some_and(|s| o.decisions[s].key != o.session.items[i].key)
                })
                .count();
            assert!(
                shown_elsewhere > 0,
                "{tool:?} on {name}: some decision moved"
            );
            assert_eq!(
                o.asked, shown_elsewhere,
                "{tool:?} on {name}: the text is asked for once for each decision shown under another key"
            );
        }
        let small = opened(psalms(), tool);
        let big = opened(larger(), tool);
        let per = |o: &Opened| o.read.records_listed as f64 / o.decisions.len().max(1) as f64;
        assert!(
            per(&big) <= per(&small) * 1.5 + 1.0,
            "{tool:?}: listing the decisions visits more records per decision on the larger \
             corpus ({:?} against {:?})",
            big.read,
            small.read
        );
    }
}

#[test]
fn reassessing_a_verse_touches_only_its_checks() {
    for tool in [TN, TW] {
        for (name, state) in [("psalms", psalms()), ("larger", larger())] {
            let mut o = opened(state, tool);
            // The first verse with a decided check on it.
            let (chapter, verse) = (1..=150u16)
                .flat_map(|c| (1..=176u16).map(move |v| (c, v)))
                .find(|&(c, v)| {
                    o.session
                        .checks_at(c, v)
                        .iter()
                        .any(|&i| o.session.shown(i).is_some())
                })
                .expect("a Psalms verse with a decision");
            let rows: Vec<usize> = o.session.checks_at(chapter, verse).to_vec();
            let with_decision = rows
                .iter()
                .filter(|&&i| o.session.shown(i).is_some())
                .count();
            // The verse is rewritten: its text no longer says what the decisions were made on.
            let edited = TargetText::new("A different verse altogether".to_owned());
            let before: Vec<CheckState> = o.session.states.clone();
            let asked = Cell::new(0usize);
            let touched =
                o.session
                    .reassess(chapter, verse, &o.decisions, o.resolution.as_ref(), |_| {
                        asked.set(asked.get() + 1);
                        Some(&edited)
                    });
            assert_eq!(touched, rows, "{tool:?} on {name}: the rows redrawn");
            assert_eq!(
                asked.get(),
                with_decision,
                "{tool:?} on {name}: the text is read once for each check with a decision"
            );
            // No other check's state moved.
            for (i, (was, now)) in before.iter().zip(&o.session.states).enumerate() {
                if !rows.contains(&i) {
                    assert_eq!(
                        was, now,
                        "{tool:?} on {name}: check {i} is not on the verse"
                    );
                }
            }
            // The edit invalidated something the decisions selected on, or at least was
            // assessed: the states of the verse's checks are what assess says of the edit.
            for &i in &rows {
                let expect = match o.session.shown(i) {
                    None => CheckState::undecided(),
                    Some(s) => CheckState::of(
                        Some(&o.decisions[s].record),
                        o.resolution.as_ref(),
                        Some(&edited),
                    ),
                };
                assert_eq!(o.session.states[i], expect, "{tool:?} on {name}: check {i}");
            }
            // The progress followed the rows without being counted again.
            assert_eq!(o.session.progress(), progress(&o.session.states));
            // A verse no check is on has no rows and reads no text.
            assert!(o.session.checks_at(151, 1).is_empty());
            let none = o
                .session
                .reassess(151, 1, &o.decisions, o.resolution.as_ref(), |_| {
                    panic!("no check is on Psalm 151")
                });
            assert!(none.is_empty());
        }
    }
}

fn state_with(
    selected: bool,
    nothing: bool,
    status: Option<tc_check::UserStatus>,
    bad: bool,
) -> CheckState {
    let mut state = CheckState::undecided();
    state.has_selections = selected;
    state.nothing_to_select = nothing;
    state.assessment = Some(tc_check::Assessment {
        invalidation: bad.then_some(tc_check::Invalidation {
            resource_changed: true,
            selections_lost: false,
            imported: false,
        }),
        verse_edits: false,
        status,
        user_invalid: false,
    });
    state
}

fn any_state() -> impl Strategy<Value = CheckState> {
    (
        any::<bool>(),
        any::<bool>(),
        prop::sample::select(vec![
            None,
            Some(tc_check::UserStatus::Valid),
            Some(tc_check::UserStatus::Invalid),
            Some(tc_check::UserStatus::Todo),
        ]),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(selected, nothing, status, bad, none)| {
            if none {
                CheckState::undecided()
            } else {
                state_with(selected, nothing, status, bad)
            }
        })
}

proptest! {
    #[test]
    fn progress_after_a_decision_costs_one_row(
        start in prop::collection::vec(any_state(), 1..40),
        steps in prop::collection::vec((any::<prop::sample::Index>(), any_state()), 0..60),
    ) {
        let mut states = start;
        let mut running = progress(&states);
        for (at, after) in steps {
            let i = at.index(states.len());
            running.update(&states[i], &after);
            states[i] = after;
            prop_assert_eq!(running, progress(&states));
        }
        prop_assert_eq!(running.total, states.len());
    }
}

/// The rail's groups as tC4 computes them and as this crate computed them before the sort
/// keys were cached and the groups bucketed: a stable sort on the chapter and verse parts
/// read as numbers, then a scan of the rows for each group.
fn reference_groups(
    items: &[CheckItem],
    states: &[CheckState],
    filter: Filter,
    mode: SortMode,
) -> Vec<RailGroup> {
    let number = |part: &str| {
        part.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .unwrap_or(-1.0)
    };
    let mut rows: Vec<usize> = (0..items.len())
        .filter(|&i| states.get(i).is_some_and(|s| filter.matches(s)))
        .collect();
    rows.sort_by(|&a, &b| {
        let (ac, av) = items[a].key.target.parts();
        let (bc, bv) = items[b].key.target.parts();
        number(&ac)
            .partial_cmp(&number(&bc))
            .filter(|o| o.is_ne())
            .or_else(|| number(&av).partial_cmp(&number(&bv)))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    match mode {
        SortMode::ByVerse => vec![RailGroup {
            label: GroupLabel::Unit,
            rows,
        }],
        SortMode::ByWord => {
            let mut terms: Vec<&str> = Vec::new();
            for &i in &rows {
                if !terms.contains(&items[i].group_id.as_str()) {
                    terms.push(&items[i].group_id);
                }
            }
            terms.sort_by(|a, b| tc_check::rail::locale_cmp(a, b));
            terms
                .into_iter()
                .map(|term| RailGroup {
                    label: GroupLabel::Term(term.to_owned()),
                    rows: rows
                        .iter()
                        .copied()
                        .filter(|&i| items[i].group_id == term)
                        .collect(),
                })
                .collect()
        }
        SortMode::ByCategory => {
            let mut categories = Vec::new();
            for &i in &rows {
                if !categories.contains(&items[i].category) {
                    categories.push(items[i].category);
                }
            }
            categories
                .into_iter()
                .map(|category| RailGroup {
                    label: GroupLabel::Category(category),
                    rows: rows
                        .iter()
                        .copied()
                        .filter(|&i| items[i].category == category)
                        .collect(),
                })
                .collect()
        }
    }
}

#[test]
fn rail_groups_visit_each_row_once() {
    for tool in [TN, TW] {
        let o = opened(psalms(), tool);
        let (items, states) = (&o.session.items, &o.session.states);
        let n = items.len();
        for filter in Filter::ORDER {
            for &mode in SortMode::modes(tool).iter().chain(&[SortMode::ByCategory]) {
                let mut work = Work::default();
                let groups = groups_counted(items, states, filter, mode, &mut work);
                let shown: usize = groups.iter().map(|g| g.rows.len()).sum();
                let label = format!("{tool:?} {filter:?} {mode:?}");
                // The same groups as the straightforward form makes of them.
                assert_eq!(
                    groups,
                    reference_groups(items, states, filter, mode),
                    "{label}"
                );
                // Each check is tested once, its key read once if shown, its row grouped once.
                assert_eq!(work.rows_tested, n, "{label}");
                assert_eq!(work.keys_read, shown, "{label}");
                let bucketed = if mode == SortMode::ByVerse { 0 } else { shown };
                assert_eq!(work.bucketed, bucketed, "{label}");
                // The sorts compare cached keys: n log n for the rows and for the groups.
                let log = |n: usize| (n.max(2) as f64).log2().ceil() as usize + 1;
                let bound = shown * log(shown) + groups.len() * log(groups.len());
                assert!(
                    work.comparisons <= bound,
                    "{label}: {} comparisons for {shown} rows in {} groups",
                    work.comparisons,
                    groups.len()
                );
            }
        }
    }
}

/// A ladder for the decisions of `fold`, and the one after a new release of tN and TWL.
fn ladders(fold: &FoldState) -> (LanguageSets, LanguageSets) {
    let current = ladder(fold);
    let mut next = current.clone();
    let primary = next
        .primary
        .as_mut()
        .expect("the corpus pins a primary set");
    for slot in [SetSlot::TranslationNotes, SetSlot::TranslationWordsLinks] {
        let SlotPin::Usable(mut pin) = current.held(Rung::Primary, slot).expect("a pin") else {
            panic!("a usable pin");
        };
        pin.resource = ResourceRef {
            repo_path: pin.resource.repo_path.clone(),
            sha: GitOid::parse(&"ab".repeat(20)).unwrap(),
        };
        pin.version = Some("v92".to_owned());
        let pin: Pin = pin;
        primary.set_pin(slot, &pin);
    }
    (current, next)
}

#[test]
fn counting_consequences_visits_each_decision_once() {
    for (name, state) in [("psalms", psalms()), ("larger", larger())] {
        let fold = fork(state);
        let (current, next) = ladders(&fold);
        let books: Vec<BookId> = (1..=66u8)
            .filter_map(BookId::from_ordinal)
            .filter(|b| {
                !fold.decisions_of(TN, *b).is_empty() || !fold.decisions_of(TW, *b).is_empty()
            })
            .collect();
        assert!(!books.is_empty());
        let mut stored: Vec<(ToolId, BookId, Vec<StoredDecision>)> = Vec::new();
        for &book in &books {
            for tool in [TN, TW] {
                let decisions = fold.decisions(tool, Collection::Book(book));
                stored.push((
                    tool,
                    book,
                    decisions
                        .into_iter()
                        .map(|d| StoredDecision::of(d.key, d.record))
                        .collect(),
                ));
            }
        }
        let units: Vec<Unit<'_>> = stored
            .iter()
            .map(|(tool, book, decisions)| Unit {
                tool: *tool,
                collection: Collection::Book(*book),
                decisions,
            })
            .collect();
        let total: usize = stored.iter().map(|(_, _, d)| d.len()).sum();
        let with_decisions = stored.iter().filter(|(_, _, d)| !d.is_empty()).count();
        let mut work = Work::default();
        let local_calls = Cell::new(0usize);
        let out = consequences_counted(
            &current,
            &next,
            &units,
            |_| {
                local_calls.set(local_calls.get() + 1);
                None
            },
            &mut work,
        );
        eprintln!(
            "{name}: {} units, {with_decisions} with decisions, {total} decisions, at risk {}, \
             revived {}, detached {}, work {work:?}",
            units.len(),
            out.at_risk(),
            out.revived,
            out.detached
        );
        assert_eq!(work.decisions_visited, total, "{name}: each decision once");
        assert_eq!(
            work.ladder_answers,
            2 * with_decisions,
            "{name}: two answers a unit"
        );
        assert!(
            out.at_risk() > 0,
            "{name}: a new release puts decisions at risk"
        );
        assert_eq!(
            out.at_risk() + out.revived + out.detached,
            total,
            "{name}: every decision is counted once, somewhere"
        );
        assert!(local_calls.get() <= 2 * with_decisions);
    }
}

/// `stored` decisions of `tool` on Psalms.
fn stored_of(state: &FoldState, tool: ToolId) -> Vec<StoredDecision> {
    state
        .decisions(tool, Collection::Book(PSALMS))
        .into_iter()
        .map(|d| StoredDecision::of(d.key, d.record))
        .collect()
}

/// The list after a release: every 50th check dropped and every 50th from the 25th has a
/// quote that says something else (the 2% and 2% of the plan).
fn released(items: &[CheckItem]) -> Vec<CheckItem> {
    items
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 50 != 0)
        .map(|(i, item)| {
            let mut item = item.clone();
            if i % 50 == 25 {
                item.quote = tc_helps::Quote::new(format!("{} ἄλλος", item.quote.as_str()));
            }
            item
        })
        .collect()
}

/// The list with every 50th check one verse on, which its own decision does not follow.
fn shifted(items: &[CheckItem]) -> Vec<CheckItem> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut item = item.clone();
            if i % 50 == 10
                && let CheckTarget::Verse { verse, .. } = &mut item.key.target
                && let Some(n) = verse.number()
            {
                *verse = tc_check::RefPart::from_number(n + 1);
            }
            item
        })
        .collect()
}

fn new_resource() -> ResourceRef {
    ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
        sha: GitOid::parse(&"cd".repeat(20)).unwrap(),
    }
}

#[test]
fn a_books_carry_over_costs_that_book() {
    for tool in [TN, TW] {
        let items = &list(tool).items;
        let stored = stored_of(larger(), tool);
        assert!(!stored.is_empty());
        // Three lists: the same one, the release with a few checks gone and a few quotes
        // changed, and the worst one for a pass that scans: no decision keeps its check
        // id, so every decision goes to the place lookup.
        let renamed: Vec<StoredDecision> = stored
            .iter()
            .map(|d| {
                let mut d = d.clone();
                d.key.check_id = tc_check::CheckId::parse("zzzz").unwrap();
                d
            })
            .collect();
        let after = released(items);
        for (what, new_items, decisions) in [
            ("unchanged", items.clone(), stored.clone()),
            ("released", after, stored.clone()),
            ("no id kept", items.clone(), renamed),
        ] {
            let (n, m) = (new_items.len(), decisions.len());
            let mut work = Work::default();
            let plan = plan_counted(&new_items, &new_resource(), &decisions, &mut work);
            eprintln!("{tool:?} {what}: {n} checks, {m} decisions, work {work:?}");
            assert_eq!(plan.entries.len(), m);
            // Linear: every counter is a constant times the checks and decisions, and the
            // comparisons of quotes are at most one for each decision.
            let bound = 4 * (n + m);
            assert!(work.identity_probes <= n + m, "{what}");
            assert!(work.id_probes <= m, "{what}");
            assert!(work.quote_comparisons <= m, "{what}");
            assert!(work.place_probes <= m, "{what}");
            assert!(work.place_candidates <= bound, "{what}: {work:?}");
            assert!(work.contest_probes <= m, "{what}");
        }
    }
}
