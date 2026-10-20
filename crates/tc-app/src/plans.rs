//! Planning what a change of pins does to the project's check decisions (APP-DESIGN §4.8,
//! STORAGE §5.8; tC4 `planResourcesChange`, `carryOverDecisions`, `consequencesOfGatewayChange`).
//!
//! The change is any move of the pins: a release upgrade of a language set, a new checking
//! language, a re-pin from the guided fix. It is planned off the store's actor from one
//! snapshot, in four steps, each costing what it touches and no more:
//!
//! 1. the (tool, book) units whose list the change moves ([`tc_check::change::unit_moves`]),
//!    found from the pins alone, so a book the change does not reach is never read;
//! 2. those units' decisions, counted by [`tc_check::change::consequences`] into the units the
//!    change affects, which it may block;
//! 3. each affected unit's list derived from the new pins, and [`tc_check::carry::plan`] run
//!    over its decisions, which gives the figures the dialog shows;
//! 4. the carry-over as planned changes, each naming the heads its record had in the snapshot,
//!    for the store's one `ApplyUpgrade` batch.
//!
//! A decision no list places is not written: its resource no longer matches, so it computes
//! as invalidated and is kept, and pinning the old resource again makes it valid. A decision
//! that would move onto a check another decision holds or moves onto is not moved either: the
//! store's `ApplyUpgrade` takes one change per record and refuses an arrival over a record
//! it had not seen, so such a decision stays where it is, kept and listed among the checks
//! to check again, never replaced. (STORAGE §5.8 would write both as a conflict; a request to
//! the storage team asks for it.)

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tc_api::check::{CheckUnit, Tool};
use tc_api::resources::{BlockReason, BlockedUnit, Carry, PlanRow};
use tc_check::carry::{self, Outcome, StoredDecision};
use tc_check::change::{self, Unit};
use tc_check::derive::{BookSpec, CheckList, notes_for_book, notes_for_stories};
use tc_check::derive::{word_links_for_book, word_links_for_stories};
use tc_check::status::TargetText;
use tc_check::{
    Answer, CheckItem, CheckKey, CheckTarget, Collection, Coverage, DecisionRecord, Ladder,
    ResourceRef, SetSlot, SlotPin, ToolId, Unrecorded,
};
use tc_core::BookId;
use tc_helps::WordArticleId;
use tc_journal::command::PlannedChange;
use tc_journal::keys::PinGroup;
use tc_journal::record::{CheckValue, Record, Scope as BookScope};
use tc_journal::{Cause, ChangeId, RecordKey};
use tc_versification::Scheme;

use crate::counters::CountingSnapshot;
use crate::numbering::Numbering;

/// The two tools a change of pins moves, in the order tC4 lists a book's rows (`TOOL_SLOT`).
pub const TOOLS: [ToolId; 2] = [ToolId::TranslationWords, ToolId::TranslationNotes];

/// A resource the new pins resolve a unit's list to, and the translation words that filter a
/// Words list of a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSource {
    pub list: ResourceRef,
    /// The articles of the translation words pin of the rung that answered the list.
    pub articles: Option<ResourceRef>,
}

/// What a plan cannot read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// A resource the new pins name could not be read. Said as it is: a count made over a
    /// resource that could not be read would understate what the change costs.
    #[error("{0}")]
    Unreadable(String),
}

/// What a plan reads outside the fold: the installed resources.
pub trait PlanSources {
    /// The books an installed copy of a resource holds, for a pin that records none.
    fn coverage(&self, resource: &ResourceRef) -> Option<Coverage>;
    /// The articles an installed translation words resource carries; `None` when it cannot be
    /// told, which filters nothing.
    fn articles(&self, resource: &ResourceRef) -> Option<BTreeSet<WordArticleId>>;
    /// The check list of `tool` over `collection` from `source`. A resource that says nothing
    /// about the book is an empty list, a designed state; one that cannot be read is an error.
    fn list(
        &self,
        tool: ToolId,
        collection: Collection,
        source: &ListSource,
        spec: &UnitSpec<'_>,
    ) -> Result<CheckList, PlanError>;
}

/// What a list is derived for besides its resource: the project's numbering and the book's scope.
#[derive(Debug, Clone, Copy)]
pub struct UnitSpec<'a> {
    pub frame: &'a Scheme,
    pub scope: &'a [tc_burrito::ScopeRange],
}

/// What a plan is for, and what it found.
#[derive(Debug, Default)]
pub struct DecisionPlan {
    /// The carry-over, one change per record, in key order.
    pub changes: Vec<PlannedChange>,
    pub rows: Vec<PlanRow>,
    pub blocked: Vec<BlockedUnit>,
    /// Decisions made against what the change leaves.
    pub at_risk: usize,
}

/// A unit the plan reads: a tool over one book or the stories.
#[derive(Debug, Clone)]
struct Candidate {
    tool: ToolId,
    collection: Collection,
    decisions: Vec<StoredDecision>,
}

pub fn api_tool(tool: ToolId) -> Option<Tool> {
    match tool {
        ToolId::TranslationNotes => Some(Tool::TranslationNotes),
        ToolId::TranslationWords => Some(Tool::TranslationWords),
        ToolId::TranslationQuestions => None,
    }
}

pub fn core_tool(tool: Tool) -> ToolId {
    match tool {
        Tool::TranslationNotes => ToolId::TranslationNotes,
        Tool::TranslationWords => ToolId::TranslationWords,
    }
}

pub fn api_unit(collection: Collection) -> CheckUnit {
    match collection {
        Collection::Book(book) => CheckUnit::Book(book),
        Collection::Stories => CheckUnit::Stories,
    }
}

pub fn core_unit(unit: CheckUnit) -> Collection {
    match unit {
        CheckUnit::Book(book) => Collection::Book(book),
        CheckUnit::Stories => Collection::Stories,
    }
}

/// What the project holds that a plan reads of the fold.
pub struct ProjectUnits {
    /// The collections of the project: its present books, or the stories.
    pub collections: Vec<Collection>,
    pub numbering: Numbering,
}

/// The collections of a project, from the snapshot: each book that projects as present (a
/// removed book is never touched, so an upgrade never brings it back), or the stories.
pub fn collections(read: &CountingSnapshot, stories: bool) -> Vec<Collection> {
    if stories {
        return vec![Collection::Stories];
    }
    read.projected_books()
        .into_iter()
        .map(Collection::Book)
        .collect()
}

/// Plans carrying the project's decisions from `current` pins over to `next`.
pub fn plan_decisions(
    read: &CountingSnapshot,
    sources: &dyn PlanSources,
    current: &dyn Ladder,
    next: &dyn Ladder,
    project: &ProjectUnits,
    cancelled: &dyn Fn() -> bool,
    on_unit: &dyn Fn(Collection),
) -> Result<DecisionPlan, PlanError> {
    let local = |r: &ResourceRef| sources.coverage(r);
    let articles = |r: &ResourceRef| sources.articles(r);
    // Step 1 and 2: the units the change moves, and their decisions.
    let mut candidates: Vec<Candidate> = Vec::new();
    for &collection in &project.collections {
        for tool in TOOLS {
            if cancelled() {
                return Ok(DecisionPlan::default());
            }
            if !change::unit_moves(current, next, tool, collection, local) {
                continue;
            }
            // A book that holds no decision of the tool costs a range read, not a projection.
            if let Collection::Book(book) = collection
                && read.decisions_of(tool, book).is_empty()
            {
                continue;
            }
            let decisions: Vec<StoredDecision> = read
                .decisions(tool, collection)
                .into_iter()
                .map(|d| StoredDecision::of(d.key, d.record))
                .collect();
            if !decisions.is_empty() {
                candidates.push(Candidate {
                    tool,
                    collection,
                    decisions,
                });
            }
        }
    }
    let units: Vec<Unit<'_>> = candidates
        .iter()
        .map(|c| Unit {
            tool: c.tool,
            collection: c.collection,
            decisions: &c.decisions,
        })
        .collect();
    let consequences = change::consequences(current, next, &units, local, articles);
    let mut out = DecisionPlan {
        at_risk: consequences.at_risk(),
        ..DecisionPlan::default()
    };
    // Step 3 and 4: each affected unit.
    let mut writes: BTreeMap<RecordKey, PlannedChange> = BTreeMap::new();
    for affected in &consequences.affected {
        if cancelled() {
            return Ok(DecisionPlan::default());
        }
        let Some(tool) = api_tool(affected.tool) else {
            continue;
        };
        let unit = api_unit(affected.collection);
        // A unit the change cannot be applied to is listed with the decisions it holds and no
        // carry-over (tC4 `gateway.affectedRow`).
        let block = |out: &mut DecisionPlan, reason| {
            out.rows.push(PlanRow {
                tool,
                unit,
                decisions: count(affected.decisions),
                against: repo_name(&affected.checked_against),
                carry: None,
            });
            out.blocked.push(BlockedUnit { tool, unit, reason });
        };
        let eng = tc_versification::StandardScheme::Eng.scheme();
        let frame = match (project.numbering, affected.collection) {
            // The stories carry no verse numbering.
            (_, Collection::Stories) | (Numbering::Same, _) => Some(eng),
            (Numbering::Other(scheme), _) => Some(scheme),
            (Numbering::Unknown, _) => None,
        };
        let Some(frame) = frame else {
            block(&mut out, BlockReason::VersificationUnknown);
            continue;
        };
        if affected.blocked {
            block(&mut out, BlockReason::NoResource);
            continue;
        }
        let Some(candidate) = candidates
            .iter()
            .find(|c| c.tool == affected.tool && c.collection == affected.collection)
        else {
            continue;
        };
        let Some(answer) = next.answer(
            affected.tool,
            affected.collection,
            Unrecorded::Installed(&local),
        ) else {
            block(&mut out, BlockReason::NoResource);
            continue;
        };
        let Answer::Pin { rung, pin } = answer else {
            block(&mut out, BlockReason::NoResource);
            continue;
        };
        let list_source = ListSource {
            list: pin.resource.clone(),
            articles: match (affected.tool, affected.collection) {
                (ToolId::TranslationWords, Collection::Book(_)) => {
                    match next.held(rung, SetSlot::TranslationWords) {
                        Some(SlotPin::Usable(held)) => Some(held.resource),
                        _ => None,
                    }
                }
                _ => None,
            },
        };
        let scope = book_scope(read, affected.collection);
        let spec = UnitSpec {
            frame,
            scope: &scope,
        };
        on_unit(affected.collection);
        let list = sources.list(affected.tool, affected.collection, &list_source, &spec)?;
        let mut carried = carry::plan(&list.items, &pin.resource, &candidate.decisions);
        keep_in_place(&mut carried, &|key| read.in_conflict(key));
        let texts = read.check_texts(affected.collection);
        let targets: BTreeMap<CheckTarget, Arc<TargetText>> = carried
            .entries
            .iter()
            .filter_map(carry::Entry::placed_at)
            .filter_map(|key| {
                texts
                    .text(&key.target)
                    .map(|text| (key.target.clone(), text))
            })
            .collect();
        let summary = carried.summary(Some(&affected.checked_against), |t| {
            targets.get(t).map(|text| &**text)
        });
        out.rows.push(PlanRow {
            tool,
            unit,
            decisions: count(affected.decisions),
            against: repo_name(&affected.checked_against),
            carry: Some(Carry {
                carried: count(summary.decided_after),
                check_again: count(summary.invalidated),
            }),
        });
        let heads = |key: &RecordKey| read.heads(key);
        for change in carry_changes(&heads, &carried, &list.items, rung.into()) {
            writes.insert(change.record.key(), change);
        }
    }
    out.changes = writes.into_values().collect();
    Ok(out)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `en_tn`, from a resource's `git.door43.org/unfoldingWord/en_tn`.
pub fn repo_name(resource: &ResourceRef) -> String {
    resource
        .repo_path
        .rsplit('/')
        .next()
        .unwrap_or(&resource.repo_path)
        .to_owned()
}

/// The ranges of the book's scope; none is the whole book.
fn book_scope(read: &CountingSnapshot, collection: Collection) -> Vec<tc_burrito::ScopeRange> {
    let Collection::Book(book) = collection else {
        return Vec::new();
    };
    match read.projected(&RecordKey::BookPresence(book)) {
        Some(Record::BookPresence {
            value:
                tc_journal::record::BookPresence::Present {
                    scope: BookScope::Ranges(ranges),
                    ..
                },
            ..
        }) => ranges.as_slice().to_vec(),
        _ => Vec::new(),
    }
}

/// Leaves where they are, as decisions no list places, those the plan cannot write without
/// replacing or hiding one: a move that meets another decision, and anything over a record in
/// conflict, whose own key or target is one. A change over a conflict's heads would end it and
/// show only one version. A move onto the key of a decision that is left in place is left too,
/// so the pass runs until nothing more is held.
fn keep_in_place(plan: &mut carry::Plan, in_conflict: &dyn Fn(&RecordKey) -> bool) {
    let mut held: BTreeSet<CheckKey> = BTreeSet::new();
    for entry in &plan.entries {
        let target = match &entry.outcome {
            Outcome::Stays { .. } => None,
            Outcome::Moves { to, .. } => Some(to),
            Outcome::Unplaced => continue,
        };
        for key in std::iter::once(&entry.key).chain(target) {
            if in_conflict(&RecordKey::Check(key.clone())) {
                held.insert(key.clone());
            }
        }
    }
    loop {
        let mut more = false;
        for entry in &mut plan.entries {
            let hold = match &entry.outcome {
                Outcome::Stays { .. } => held.contains(&entry.key),
                Outcome::Moves { to, contested, .. } => {
                    *contested || held.contains(&entry.key) || held.contains(to)
                }
                Outcome::Unplaced => false,
            };
            if hold {
                entry.outcome = Outcome::Unplaced;
                more |= held.insert(entry.key.clone());
            }
        }
        if !more {
            return;
        }
    }
}

/// The changes one unit's carry-over writes, one per record (STORAGE §5.8 step 2).
fn carry_changes(
    heads: &dyn Fn(&RecordKey) -> Vec<ChangeId>,
    plan: &carry::Plan,
    items: &[CheckItem],
    group: PinGroup,
) -> Vec<PlannedChange> {
    let mut out: BTreeMap<RecordKey, PlannedChange> = BTreeMap::new();
    let cause = Cause::CarryOver { group };
    let write = |key: &CheckKey, value: CheckValue| {
        let record = RecordKey::Check(key.clone());
        PlannedChange {
            parents: heads(&record),
            record: Record::Check {
                key: key.clone(),
                value,
            },
            cause: cause.clone(),
        }
    };
    let decided = |entry: &carry::Entry, item: usize| DecisionRecord {
        resource: plan.resource.clone(),
        context: merged_context(&entry.decision, &items[item]),
        ..entry.decision.clone()
    };
    // The departures first, so an arrival at the key a decision leaves takes that record's
    // place: the decision that moved away lives at its new key.
    for entry in &plan.entries {
        match &entry.outcome {
            Outcome::Stays { item, .. } => {
                let after = decided(entry, *item);
                if after != entry.decision {
                    out.insert(
                        RecordKey::Check(entry.key.clone()),
                        write(&entry.key, CheckValue::Decision(after)),
                    );
                }
            }
            Outcome::Moves {
                to,
                contested: false,
                ..
            } => {
                out.insert(
                    RecordKey::Check(entry.key.clone()),
                    write(&entry.key, CheckValue::MovedTo { to: to.clone() }),
                );
            }
            Outcome::Moves {
                contested: true, ..
            }
            | Outcome::Unplaced => {}
        }
    }
    for entry in &plan.entries {
        if let Outcome::Moves {
            item,
            to,
            contested: false,
            ..
        } = &entry.outcome
        {
            out.insert(
                RecordKey::Check(to.clone()),
                write(to, CheckValue::Decision(decided(entry, *item))),
            );
        }
    }
    out.into_values().collect()
}

/// The new check's context over the decision's own: the new resource decides every field it
/// names, and a field only the old context holds is kept.
fn merged_context(decision: &DecisionRecord, item: &CheckItem) -> tc_journal::json::JsonMap {
    let mut context = decision.context.clone();
    for (name, value) in item.journal_context().iter() {
        context.insert(name.clone(), value.clone());
    }
    context
}

/// The list of a unit's checks, by derive, for the notes or the words links.
pub fn derive_list(
    tool: ToolId,
    collection: Collection,
    notes: &[tc_helps::Note],
    links: &[tc_helps::WordLink],
    articles: Option<&BTreeSet<WordArticleId>>,
    spec: &UnitSpec<'_>,
) -> CheckList {
    match (tool, collection) {
        (ToolId::TranslationNotes, Collection::Book(book)) => {
            notes_for_book(notes, &book_spec(book, spec))
        }
        (ToolId::TranslationWords, Collection::Book(book)) => {
            word_links_for_book(links, articles, &book_spec(book, spec))
        }
        (ToolId::TranslationNotes, Collection::Stories) => notes_for_stories(notes, None),
        (ToolId::TranslationWords, Collection::Stories) => word_links_for_stories(links, None),
        (ToolId::TranslationQuestions, _) => CheckList {
            items: Vec::new(),
            dropped: Vec::new(),
            skipped: Vec::new(),
            mapped: false,
        },
    }
}

fn book_spec<'a>(book: BookId, spec: &'a UnitSpec<'a>) -> BookSpec<'a> {
    BookSpec {
        book,
        resource_frame: tc_versification::StandardScheme::Eng.scheme(),
        project_frame: Some(spec.frame),
        scope: spec.scope,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    //! How a carry-over becomes changes: one change per record, never a replacement of a
    //! record the plan did not read, never a loss of what a decision holds.

    use tc_check::carry::Entry;
    use tc_check::{Judgement, UserStatus};
    use tc_helps::tsv::{TsvScope, parse_notes};
    use tc_journal::Json;
    use tc_journal::decision::verse_text_md5_v1;

    use super::*;

    const TIT_V86: &str =
        include_str!("../../../fixtures/tc4/test-fixtures/resources/en_tn@v86/TIT.tsv");

    fn items() -> Vec<CheckItem> {
        let notes = parse_notes(TIT_V86, TsvScope::Book(BookId::Titus))
            .unwrap()
            .notes;
        let eng = tc_versification::StandardScheme::Eng.scheme();
        notes_for_book(
            &notes,
            &BookSpec {
                book: BookId::Titus,
                resource_frame: eng,
                project_frame: Some(eng),
                scope: &[],
            },
        )
        .items
    }

    fn resource(sha: char) -> ResourceRef {
        ResourceRef {
            repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
            sha: tc_journal::GitOid::parse(&sha.to_string().repeat(40)).unwrap(),
        }
    }

    fn decision(item: &CheckItem, resource: ResourceRef) -> DecisionRecord {
        let mut context = item.journal_context();
        context.insert("tcExtra".into(), Json::from("kept"));
        DecisionRecord {
            context,
            category: Some("translate".into()),
            judgement: Judgement {
                selections: None,
                nothing_to_select: true,
                status: Some(UserStatus::Valid),
                verse_text_md5: verse_text_md5_v1("Paul"),
            },
            comment: Some("a translator's comment".into()),
            bookmarked: true,
            resource,
            imported_invalid: false,
            dismissed: false,
            extra: tc_journal::json::JsonMap::default(),
        }
    }

    fn heads_of(key: &RecordKey) -> Vec<ChangeId> {
        // A head per record, told apart by the key's text.
        let mut bytes = [0u8; 32];
        for (i, b) in format!("{key:?}").bytes().enumerate() {
            bytes[i % 32] ^= b;
        }
        vec![ChangeId::from_bytes(bytes)]
    }

    fn changes_of(plan: &carry::Plan, items: &[CheckItem]) -> BTreeMap<RecordKey, PlannedChange> {
        carry_changes(&heads_of, plan, items, PinGroup::Primary)
            .into_iter()
            .map(|c| (c.record.key(), c))
            .collect()
    }

    fn decision_of(change: &PlannedChange) -> &DecisionRecord {
        match &change.record {
            Record::Check {
                value: CheckValue::Decision(d),
                ..
            } => d,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_decision_that_stays_takes_the_new_resource_and_keeps_every_word_the_person_wrote() {
        let items = items();
        let old = resource('1');
        let new = resource('2');
        let was = decision(&items[0], old);
        let plan = carry::Plan {
            resource: new.clone(),
            entries: vec![Entry {
                key: items[0].key.clone(),
                decision: was.clone(),
                outcome: Outcome::Stays {
                    item: 0,
                    by: carry::Match::Identity,
                },
            }],
            undecided: 0,
        };
        let written = changes_of(&plan, &items);
        let change = &written[&RecordKey::Check(items[0].key.clone())];
        let now = decision_of(change);
        assert_eq!(now.resource, new);
        assert_eq!(now.judgement, was.judgement);
        assert_eq!(now.comment, was.comment);
        assert!(now.bookmarked);
        assert_eq!(
            now.context.get("tcExtra"),
            Some(&Json::from("kept")),
            "a context field only the old record holds stays"
        );
        assert_eq!(
            change.parents,
            heads_of(&RecordKey::Check(items[0].key.clone()))
        );
        assert_eq!(
            change.cause,
            Cause::CarryOver {
                group: PinGroup::Primary
            }
        );
    }

    #[test]
    fn a_decision_already_on_the_new_resource_is_not_written_again() {
        let items = items();
        let new = resource('2');
        let plan = carry::Plan {
            resource: new.clone(),
            entries: vec![Entry {
                key: items[0].key.clone(),
                decision: DecisionRecord {
                    context: items[0].journal_context(),
                    ..decision(&items[0], new)
                },
                outcome: Outcome::Stays {
                    item: 0,
                    by: carry::Match::Identity,
                },
            }],
            undecided: 0,
        };
        assert!(changes_of(&plan, &items).is_empty());
    }

    #[test]
    fn a_decision_no_list_places_is_not_touched() {
        let items = items();
        let plan = carry::Plan {
            resource: resource('2'),
            entries: vec![Entry {
                key: items[0].key.clone(),
                decision: decision(&items[0], resource('1')),
                outcome: Outcome::Unplaced,
            }],
            undecided: 3,
        };
        assert!(changes_of(&plan, &items).is_empty());
    }

    #[test]
    fn a_move_writes_the_old_key_as_moved_and_the_decision_at_the_new_key() {
        let items = items();
        let was = decision(&items[0], resource('1'));
        let plan = carry::Plan {
            resource: resource('2'),
            entries: vec![Entry {
                key: items[0].key.clone(),
                decision: was.clone(),
                outcome: Outcome::Moves {
                    item: 1,
                    to: items[1].key.clone(),
                    by: carry::Match::CheckId,
                    contested: false,
                },
            }],
            undecided: 0,
        };
        let written = changes_of(&plan, &items);
        assert_eq!(written.len(), 2);
        let old = &written[&RecordKey::Check(items[0].key.clone())];
        assert!(matches!(
            &old.record,
            Record::Check { value: CheckValue::MovedTo { to }, .. } if *to == items[1].key
        ));
        let arrived = &written[&RecordKey::Check(items[1].key.clone())];
        assert_eq!(decision_of(arrived).judgement, was.judgement);
        assert_eq!(decision_of(arrived).comment, was.comment);
        assert_eq!(decision_of(arrived).resource, resource('2'));
    }

    #[test]
    fn two_decisions_that_would_swap_places_each_land_once() {
        let items = items();
        let (a, b) = (decision(&items[0], resource('1')), {
            let mut b = decision(&items[1], resource('1'));
            b.comment = Some("the other".into());
            b
        });
        let moves = |from: usize, to: usize, decision: &DecisionRecord| Entry {
            key: items[from].key.clone(),
            decision: decision.clone(),
            outcome: Outcome::Moves {
                item: to,
                to: items[to].key.clone(),
                by: carry::Match::Place,
                contested: false,
            },
        };
        let plan = carry::Plan {
            resource: resource('2'),
            entries: vec![moves(0, 1, &a), moves(1, 0, &b)],
            undecided: 0,
        };
        let written = changes_of(&plan, &items);
        assert_eq!(written.len(), 2, "one change for each record");
        assert_eq!(
            decision_of(&written[&RecordKey::Check(items[1].key.clone())]).comment,
            a.comment
        );
        assert_eq!(
            decision_of(&written[&RecordKey::Check(items[0].key.clone())]).comment,
            b.comment
        );
    }

    #[test]
    fn a_move_onto_a_check_another_decision_holds_stays_where_it_is() {
        let items = items();
        let plan = carry::Plan {
            resource: resource('2'),
            entries: vec![
                Entry {
                    key: items[0].key.clone(),
                    decision: decision(&items[0], resource('1')),
                    outcome: Outcome::Moves {
                        item: 1,
                        to: items[1].key.clone(),
                        by: carry::Match::Place,
                        contested: true,
                    },
                },
                Entry {
                    key: items[1].key.clone(),
                    decision: decision(&items[1], resource('1')),
                    outcome: Outcome::Unplaced,
                },
            ],
            undecided: 0,
        };
        assert!(
            changes_of(&plan, &items).is_empty(),
            "neither decision is replaced, hidden or merged"
        );
    }

    /// A random project of decisions on the first verse's notes, each with the quote of some
    /// note of it and maybe a check id the list lacks, so the real `carry::plan` finds stays,
    /// moves, swaps, chains, contested moves and unplaced decisions; some records, and some
    /// keys a decision moves onto, are in conflict.
    mod no_loss {
        use proptest::prelude::*;
        use tc_check::carry::StoredDecision;

        use super::*;

        fn conflicted_heads(
            conflicted: BTreeSet<CheckKey>,
        ) -> impl Fn(&RecordKey) -> Vec<ChangeId> {
            move |key| {
                let mut heads = heads_of(key);
                if matches!(key, RecordKey::Check(k) if conflicted.contains(k)) {
                    heads.push(ChangeId::from_bytes([7; 32]));
                }
                heads
            }
        }

        proptest! {
            #[test]
            fn a_carry_over_never_closes_a_conflict_nor_drops_a_judgement_or_comment(
                specs in prop::collection::vec((0..4usize, 0..4usize, any::<bool>(), any::<bool>()), 1..8),
                note_conflicts in 0u8..16,
            ) {
                let items = items();
                let mut keys = BTreeSet::new();
                let mut conflicted = BTreeSet::new();
                let mut stored = Vec::new();
                for (n, (at, quoting, unknown_id, in_conflict)) in specs.into_iter().enumerate() {
                    let mut key = items[at].key.clone();
                    if unknown_id {
                        key.check_id = tc_check::CheckId::parse(&format!("z{n}z")).unwrap();
                    }
                    if !keys.insert(key.clone()) {
                        continue;
                    }
                    let mut was = decision(&items[at], resource('1'));
                    was.context.insert("quoteString".into(), Json::from(items[quoting].quote.as_str()));
                    was.comment = Some(format!("comment {n}"));
                    was.judgement.status = [UserStatus::Valid, UserStatus::Invalid, UserStatus::Todo]
                        .get(n % 3)
                        .copied();
                    if in_conflict {
                        conflicted.insert(key.clone());
                    }
                    stored.push(StoredDecision::of(key, was));
                }
                for (i, item) in items.iter().take(4).enumerate() {
                    if note_conflicts >> i & 1 == 1 {
                        conflicted.insert(item.key.clone());
                    }
                }
                let heads = conflicted_heads(conflicted.clone());
                let mut plan = carry::plan(&items, &resource('2'), &stored);
                keep_in_place(&mut plan, &|key| heads(key).len() > 1);
                let written = carry_changes(&heads, &plan, &items, PinGroup::Primary);

                let mut now: BTreeMap<CheckKey, Option<DecisionRecord>> = stored
                    .iter()
                    .map(|s| (s.key.clone(), Some(s.decision.clone())))
                    .collect();
                for change in &written {
                    let Record::Check { key, value } = &change.record else {
                        panic!("{change:?}")
                    };
                    prop_assert!(!conflicted.contains(key), "{key:?} is in conflict and was written");
                    now.insert(key.clone(), match value {
                        CheckValue::Decision(d) => Some(d.clone()),
                        _ => None,
                    });
                }
                for was in &stored {
                    let kept = now.values().flatten().any(|d| {
                        d.comment == was.decision.comment && d.judgement == was.decision.judgement
                    });
                    prop_assert!(kept, "{:?} is lost", was.decision.comment);
                }
            }
        }
    }
}
