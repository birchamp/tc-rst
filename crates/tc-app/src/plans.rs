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
use tc_journal::{Cause, RecordKey};
use tc_versification::Scheme;

use crate::counters::CountingSnapshot;
use crate::numbering::Numbering;

/// The two tools a change of pins moves.
pub const TOOLS: [ToolId; 2] = [ToolId::TranslationNotes, ToolId::TranslationWords];

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
        let blocked = |reason| BlockedUnit { tool, unit, reason };
        let eng = tc_versification::StandardScheme::Eng.scheme();
        let frame = match (project.numbering, affected.collection) {
            // The stories carry no verse numbering.
            (_, Collection::Stories) | (Numbering::Same, _) => Some(eng),
            (Numbering::Other(scheme), _) => Some(scheme),
            (Numbering::Unknown, _) => None,
        };
        let Some(frame) = frame else {
            out.blocked.push(blocked(BlockReason::VersificationUnknown));
            continue;
        };
        if affected.blocked {
            out.blocked.push(blocked(BlockReason::NoResource));
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
            out.blocked.push(blocked(BlockReason::NoResource));
            continue;
        };
        let Answer::Pin { rung, pin } = answer else {
            out.blocked.push(blocked(BlockReason::NoResource));
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
        let carried = carry::plan(&list.items, &pin.resource, &candidate.decisions);
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
        for change in carry_changes(read, &carried, &list.items, rung.into()) {
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

/// The changes one unit's carry-over writes, one per record (STORAGE §5.8 step 2).
fn carry_changes(
    read: &CountingSnapshot,
    plan: &carry::Plan,
    items: &[CheckItem],
    group: PinGroup,
) -> Vec<PlannedChange> {
    let mut out: BTreeMap<RecordKey, PlannedChange> = BTreeMap::new();
    let cause = Cause::CarryOver { group };
    let write = |key: &CheckKey, value: CheckValue| {
        let record = RecordKey::Check(key.clone());
        PlannedChange {
            parents: read.heads(&record),
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
