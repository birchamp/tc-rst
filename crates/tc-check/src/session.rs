//! A check session as the Check screen opens it: the check list, the decisions placed on
//! each check and the one shown, and each check's computed state (tC4
//! `completedCheckSession`).
//!
//! The decisions are the fold's projection of the (tool, collection)
//! ([`tc_journal::fold::FoldState::decisions`]): each stored decision with the state the
//! fold computed for it, and whether the fold lists it under Detached (§7.7). They are
//! placed on the list with the carry-over's two passes ([`crate::carry`]), as tC4 places
//! them when a session opens. A decision placed on the check it is stored under shows the
//! fold's state, so the rail and the decision files cannot disagree; one shown on a check
//! under another key is assessed against that check's text with the same function
//! ([`tc_journal::decision::assess`]). Placing writes nothing: a decision shown under
//! another key is still stored under its own key until the person decides the check again
//! or the caller records the move. Every stored decision is either placed on one check or
//! listed off the list; tC4 kept only one of two decisions that land on one check, and
//! here both stay reachable.

use std::collections::BTreeMap;

use tc_journal::fold::ProjectedDecision;
use tc_journal::record::ResourceRef;

use crate::carry::{Outcome, Stored, placements};
use crate::item::CheckItem;
use crate::key::CheckTarget;
use crate::rail::{Progress, progress};
use crate::status::{CheckState, TargetText};

/// The widest verse span ([`tc_journal::keys::RefPart::span`]) the per-verse index lists
/// every verse of; a wider one (no real resource has one) is listed under its first and
/// last verse only, so a malformed cell cannot make an index of thousands of verses.
const WIDEST_SPAN: u16 = 200;

/// An open check session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub items: Vec<CheckItem>,
    /// For each check, every stored decision placed on it, in the order the decisions
    /// were given: indices into the decisions the session was opened with. A check
    /// holding two or more is contested ([`Session::contested`]).
    pub placed: Vec<Vec<usize>>,
    /// For each check, the computed state of the decision shown on it.
    pub states: Vec<CheckState>,
    /// Decisions no check on the list carries. The screen lists them as checks from a
    /// previous resource, kept until the person dismisses them.
    pub off_list: Vec<usize>,
    /// Decisions the fold lists under Detached (§7.7): made against another resource than
    /// the (tool, collection) resolves to, or on a target the text no longer has. One can
    /// also be placed on a check, where it shows as invalidated.
    pub detached: Vec<usize>,
    /// Decided checks over the checks on the list, kept as the states change.
    progress: Progress,
    /// The checks of each verse (chapter, verse), or of each frame (story, frame): a check
    /// on a span such as `9-10` is under both verses, a front or intro target under 0.
    by_verse: BTreeMap<(u16, u16), Vec<usize>>,
}

impl Session {
    /// Decided checks over the checks on the list. It is kept up to date by
    /// [`Session::reassess`] and [`Session::set_state`], not counted again.
    pub fn progress(&self) -> Progress {
        self.progress
    }

    /// The checks on verse `verse` of chapter `chapter` (frame `verse` of story
    /// `chapter`): indices into the list, in list order. A front or intro target is
    /// verse 0 of its chapter, and chapter 0 for a book's front matter.
    pub fn checks_at(&self, chapter: u16, verse: u16) -> &[usize] {
        self.by_verse
            .get(&(chapter, verse))
            .map_or(&[], Vec::as_slice)
    }

    /// Computes again the state of the checks on one verse after its text changed, and no
    /// other: tC4 re-assesses the whole list. `decisions` are the ones the session was
    /// opened with, `resolution` the resource the (tool, collection) resolves to and
    /// `text` the text of a check's target by the no-text rule of §7.7, now with the
    /// edit. The checks on the verse are returned, the rows a screen draws again, and the
    /// progress follows them. Where a check's decision is shown is not decided by the
    /// text, so nothing is placed again.
    pub fn reassess<'t>(
        &mut self,
        chapter: u16,
        verse: u16,
        decisions: &[ProjectedDecision],
        resolution: Option<&ResourceRef>,
        text: impl Fn(&CheckTarget) -> Option<&'t TargetText>,
    ) -> Vec<usize> {
        let rows = self.checks_at(chapter, verse).to_vec();
        for &i in &rows {
            let item = &self.items[i];
            let state = match self.placed[i].last() {
                None => CheckState::undecided(),
                Some(&s) => CheckState::of(
                    Some(&decisions[s].record),
                    resolution,
                    text(&item.key.target),
                ),
            };
            self.progress.update(&self.states[i], &state);
            self.states[i] = state;
        }
        rows
    }

    /// Replaces the state of check `item`: a decision recorded on it. The progress
    /// follows in the time of two reads, whatever the length of the list.
    pub fn set_state(&mut self, item: usize, state: CheckState) {
        if let Some(slot) = self.states.get_mut(item) {
            self.progress.update(slot, &state);
            *slot = state;
        }
    }

    /// The decision shown on check `item`: the later of the decisions placed on it, as in
    /// tC4.
    pub fn shown(&self, item: usize) -> Option<usize> {
        self.placed.get(item)?.last().copied()
    }

    /// The checks more than one decision is placed on. The screen shows the later
    /// decision and offers the others from [`Session::placed`], so the person chooses.
    pub fn contested(&self) -> impl Iterator<Item = usize> + '_ {
        self.placed
            .iter()
            .enumerate()
            .filter(|(_, placed)| placed.len() > 1)
            .map(|(item, _)| item)
    }
}

/// Opens a session on `items` with the fold's `decisions` of the (tool, collection).
/// `resolution` is the resource the (tool, collection) resolves to now and `text` the text
/// of a check's target by the no-text rule of §7.7, the fold's
/// (`tc_journal::fold::FoldState::check_texts`): they assess a decision shown on a check
/// under another key than its own.
pub fn open<'t>(
    items: Vec<CheckItem>,
    decisions: &[ProjectedDecision],
    resolution: Option<&ResourceRef>,
    text: impl Fn(&CheckTarget) -> Option<&'t TargetText>,
) -> Session {
    let stored: Vec<Stored<'_>> = decisions
        .iter()
        .map(|d| Stored::of(&d.key, &d.record))
        .collect();
    let (outcomes, _) = placements(&items, &stored);
    let mut placed: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    let mut off_list = Vec::new();
    for (s, outcome) in outcomes.iter().enumerate() {
        match outcome {
            Outcome::Stays { item, .. } | Outcome::Moves { item, .. } => placed[*item].push(s),
            Outcome::Unplaced => off_list.push(s),
        }
    }
    let states: Vec<CheckState> = items
        .iter()
        .zip(&placed)
        .map(|(item, on)| match on.last() {
            None => CheckState::undecided(),
            Some(&s) if decisions[s].key == item.key => {
                CheckState::assessed(&decisions[s].record, decisions[s].assessment)
            }
            Some(&s) => CheckState::of(
                Some(&decisions[s].record),
                resolution,
                text(&item.key.target),
            ),
        })
        .collect();
    let detached = decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.detached)
        .map(|(s, _)| s)
        .collect();
    let progress = progress(&states);
    let by_verse = verse_index(&items);
    Session {
        items,
        placed,
        states,
        off_list,
        detached,
        progress,
        by_verse,
    }
}

/// The verses of each check's target, as numbers: a span lists each verse it covers.
fn verse_index(items: &[CheckItem]) -> BTreeMap<(u16, u16), Vec<usize>> {
    let mut index: BTreeMap<(u16, u16), Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        let (chapter, first, last) = match &item.key.target {
            CheckTarget::Verse { chapter, verse, .. } => {
                let (first, last) = verse.span().unwrap_or((0, 0));
                (chapter.number().unwrap_or(0), first, last)
            }
            CheckTarget::Frame { story, frame } => (u16::from(story.get()), frame.0, frame.0),
        };
        let verses: Vec<u16> = if last < first {
            vec![first]
        } else if last - first > WIDEST_SPAN {
            vec![first, last]
        } else {
            (first..=last).collect()
        };
        for verse in verses {
            index.entry((chapter, verse)).or_default().push(i);
        }
    }
    index
}
