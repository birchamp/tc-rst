//! A decision and its computed state (`docs/STORAGE.md` §5.4, §7.7).
//!
//! A decision is the journal's `DecisionRecord`. It stores what it was made against: the
//! md5 of the target's text and the resource the check list came from. Whether it still
//! holds is computed every time from the current text and the current resolution, by
//! [`tc_journal::decision::assess`], the function the fold computes the decision files
//! with; nothing is written when the text or the pins change, and restoring them makes the
//! decision valid again.
//!
//! Two consequences differ from tC4, by design. tC4 wrote `invalidated` into the file and,
//! when such a decision re-attached, set its status to To do unless the person had chosen
//! Invalid; here a decision whose resource and words hold again is shown as the person
//! left it. And tC4 revalidated selections on every open, so a selection tC4 recorded on
//! a word that is also part of a longer word (`the` in "the theory") read as changed
//! although the verse was not edited; here the selections are revalidated only when the
//! text differs from the text the decision was made against. (Selections recorded here
//! count their text as the validator does, so they hold on any edit that leaves their
//! text alone; see [`crate::selection`].)
//!
//! An edit is the journal's [`DecisionEdit`]: [`decision_edit`] makes it from a button of
//! the Check screen, and [`DecisionEdit::apply`] is the one rule of what it does.

pub use tc_journal::command::{DecisionBasis, DecisionChange, DecisionEdit};
use tc_journal::record::ResourceRef;

use crate::item::CheckItem;

pub use tc_journal::decision::{Assessment, Invalidation, TargetText, assess};
pub use tc_journal::record::{DecisionRecord, Judgement, Selection, UserStatus};

/// The state of one check on the list: its decision, if any, as computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckState {
    pub assessment: Option<Assessment>,
    /// Whether the decision has selections (tC4 `selections !== false`).
    pub has_selections: bool,
    pub nothing_to_select: bool,
    pub bookmarked: bool,
    pub commented: bool,
}

impl CheckState {
    /// A check with no decision.
    pub fn undecided() -> Self {
        Self {
            assessment: None,
            has_selections: false,
            nothing_to_select: false,
            bookmarked: false,
            commented: false,
        }
    }

    /// The state of a check from its decision, assessed against `resolution` and `text`
    /// (see [`assess`]).
    pub fn of(
        decision: Option<&DecisionRecord>,
        resolution: Option<&ResourceRef>,
        text: Option<&TargetText>,
    ) -> Self {
        match decision {
            None => Self::undecided(),
            Some(d) => Self::assessed(d, assess(d, resolution, text)),
        }
    }

    /// The state of a check whose decision was assessed already: by the fold, when the
    /// decision is on the check it is stored under.
    pub fn assessed(decision: &DecisionRecord, assessment: Assessment) -> Self {
        Self {
            assessment: Some(assessment),
            has_selections: decision.judgement.selections.is_some(),
            nothing_to_select: decision.judgement.nothing_to_select,
            bookmarked: decision.bookmarked,
            commented: decision.comment.as_deref().is_some_and(|c| !c.is_empty()),
        }
    }

    pub fn invalidation(&self) -> Option<Invalidation> {
        self.assessment.and_then(|a| a.invalidation)
    }

    /// The status tC4 shows for the check (see [`Assessment::status`]).
    pub fn status(&self) -> Option<UserStatus> {
        self.assessment.and_then(|a| a.status)
    }

    /// tC4's single counting rule (`isDecided`): not invalidated, and either words are
    /// selected, nothing-to-select is set, or the status is Invalid. A bookmark or a
    /// comment is never a decision. As in tC4, a decision whose words are still selected
    /// counts as decided even after the person sets To do.
    pub fn is_decided(&self) -> bool {
        self.invalidation().is_none()
            && (self.has_selections
                || self.nothing_to_select
                || self.status() == Some(UserStatus::Invalid))
    }

    /// The dot the rail shows.
    pub fn dot(&self) -> Dot {
        if self.invalidation().is_some() {
            return Dot::Invalid;
        }
        match self.status() {
            Some(UserStatus::Valid) => Dot::Valid,
            Some(UserStatus::Invalid) => Dot::Invalid,
            Some(UserStatus::Todo) | None => Dot::Todo,
        }
    }
}

/// The status dot of a rail row: green, red, or grey. An invalidated check is red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dot {
    Valid,
    Invalid,
    Todo,
}

/// A triage button of the Check screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Triage {
    /// "Mark valid" with the tapped words; no words is nothing-to-select.
    Valid(Vec<Selection>),
    Invalid,
    Todo,
}

/// A change the person makes to a check from the Check screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Triage(Triage),
    Bookmark(bool),
    /// The check comment; an empty comment removes it.
    Comment(String),
}

impl Edit {
    /// What the button changes, as tC4's buttons change it (tC4 `recordDecision`): Mark
    /// valid sets the words or nothing-to-select, Invalid clears the words, To do sets only
    /// the status.
    pub fn changes(self) -> Vec<DecisionChange> {
        match self {
            Self::Triage(Triage::Valid(words)) if words.is_empty() => vec![
                DecisionChange::Selections(None),
                DecisionChange::NothingToSelect(true),
                DecisionChange::Status(Some(UserStatus::Valid)),
            ],
            Self::Triage(Triage::Valid(words)) => vec![
                DecisionChange::Selections(Some(words)),
                DecisionChange::NothingToSelect(false),
                DecisionChange::Status(Some(UserStatus::Valid)),
            ],
            Self::Triage(Triage::Invalid) => vec![
                DecisionChange::Selections(None),
                DecisionChange::NothingToSelect(false),
                DecisionChange::Status(Some(UserStatus::Invalid)),
            ],
            Self::Triage(Triage::Todo) => vec![DecisionChange::Status(Some(UserStatus::Todo))],
            Self::Bookmark(on) => vec![DecisionChange::Bookmarked(on)],
            Self::Comment(text) => vec![DecisionChange::Comment(Some(text))],
        }
    }
}

impl Edit {
    /// The journal command for this edit (§5.4), with the `basis` a first decision starts
    /// from and `resource`, the pin of the list the check is on.
    pub fn command(self, basis: Option<DecisionBasis>, resource: &ResourceRef) -> DecisionEdit {
        DecisionEdit {
            basis,
            resource: resource.clone(),
            changes: self.changes(),
        }
    }
}

/// The journal command for the person's edit of `item` (§5.4): its changes, the check's
/// context and category for a first decision, and `resource`, the pin of the list the
/// check is on. [`DecisionEdit::apply`] gives the decision after it.
pub fn decision_edit(item: &CheckItem, edit: Edit, resource: &ResourceRef) -> DecisionEdit {
    edit.command(
        Some(DecisionBasis {
            context: item.journal_context(),
            category: Some(item.category.name().into()),
        }),
        resource,
    )
}
