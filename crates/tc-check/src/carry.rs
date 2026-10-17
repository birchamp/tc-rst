//! Carrying decisions over when the resource behind a check list changes: a release
//! upgrade (J12), a gateway-language change (J13) or a re-pin from the guided fix
//! (tC4 `mergeAndReattach`, `reattachAcrossResource` and `carryOverDecisions`, D17 and
//! D36; STORAGE.md §5.8).
//!
//! The resource is the primary key: the list derived from the new resource is the work.
//! Each stored decision is placed on the new list in tC4's two passes. First by its
//! identity and quote: the same check id, place, occurrence and quote text. Then,
//! across languages, by check id when the original-language quote still says the same
//! thing, else by place, quote and occurrence, with the group id breaking a tie. A
//! decision neither pass can place stays where it is: its resource no longer matches,
//! so it computes as invalidated, and it is kept, so pinning the old resource again
//! makes it valid again. The plan never deletes or rewrites a decision's judgement.
//!
//! This returns a plan; the caller writes it. For each placed decision the plan names
//! the new check, so the caller can record the new resource and the new check's
//! context. A decision moved to another key is a `MovedTo` at the old key and the whole
//! decision at the new one (STORAGE.md §5.8).

use std::collections::HashMap;

use tc_journal::json::Json;
use tc_journal::record::ResourceRef;
use tc_usfm::words::words;

use crate::item::CheckItem;
use crate::key::{CheckKey, CheckTarget};
use crate::status::{CheckState, DecisionRecord, TargetText};

/// A stored decision with the two fields of its tC4 context the re-attach reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDecision {
    pub key: CheckKey,
    /// The context's `quoteString`.
    pub quote: String,
    /// The context's `groupId`.
    pub group_id: String,
    pub decision: DecisionRecord,
}

impl StoredDecision {
    /// The decision stored under `key`, with the quote and group id its context records
    /// (empty when the context lacks them, as a tC3 context can).
    pub fn of(key: CheckKey, decision: DecisionRecord) -> Self {
        let text = |name: &str| {
            decision
                .context
                .get(name)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Self {
            quote: text("quoteString"),
            group_id: text("groupId"),
            key,
            decision,
        }
    }
}

/// How a decision was placed on the new list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// Same check id, place, occurrence and quote text.
    Identity,
    /// Same check id, and the quote names the same original words.
    CheckId,
    /// Same place, quote and occurrence (a check id the new resource does not share),
    /// with the group id breaking a tie.
    Place,
}

/// What happens to one stored decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The new list has the decision's check under the same key. The decision stays;
    /// its resource becomes the new one and its context the new check's.
    Stays { item: usize, by: Match },
    /// The decision's check is on the new list under another key: the old key records
    /// `MovedTo { to }` and the new key receives the decision.
    Moves {
        item: usize,
        to: CheckKey,
        by: Match,
        /// Another decision is placed on the same check, or the decision already at the
        /// new key finds no check and stays there. Both are kept: writing them makes a
        /// conflict the person resolves. (When the decision at the new key moves away
        /// in the same plan, the move is not contested: write the arrival after that
        /// decision's `MovedTo`.)
        contested: bool,
    },
    /// No check on the new list is this decision's. Nothing is written; it computes as
    /// invalidated and is kept.
    Unplaced,
}

/// One stored decision and what the change does to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: CheckKey,
    /// The decision as stored, which a move writes whole at its new key.
    pub decision: DecisionRecord,
    pub outcome: Outcome,
}

impl Entry {
    /// The key of the check the decision is placed on: its own key when it stays, the
    /// new key when it moves, `None` when it is not placed.
    pub fn placed_at(&self) -> Option<&CheckKey> {
        match &self.outcome {
            Outcome::Stays { .. } => Some(&self.key),
            Outcome::Moves { to, .. } => Some(to),
            Outcome::Unplaced => None,
        }
    }
}

/// What a resource change does to one (tool, collection)'s decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The resource the new list comes from.
    pub resource: ResourceRef,
    /// One entry per stored decision, in the order given.
    pub entries: Vec<Entry>,
    /// Checks on the new list that no decision was placed on: work the new resource
    /// brings.
    pub undecided: usize,
}

/// Two original-language quotes name the same words: equal word by word through tC4's
/// tokenizer, `&`-separated part by part (tC4 `sameOrigQuote`), so punctuation and
/// spacing differences do not matter and a longer or shorter quote does.
pub fn same_quote(a: &str, b: &str) -> bool {
    let norm = |q: &str| {
        q.split('&')
            .map(|part| {
                words(part.trim())
                    .iter()
                    .map(|w| w.text)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(" & ")
    };
    norm(a) == norm(b)
}

/// What a cross-language match compares (tC4 `crossKey`): the place, the quote text and
/// the occurrence.
fn place_of(target: &CheckTarget, quote: &str, occurrence: u32) -> (CheckTarget, String, u32) {
    (target.clone(), quote.to_owned(), occurrence)
}

/// Plans carrying `stored` over to `items`, the list derived from `resource`.
///
/// tC4 runs the same two passes when a check session opens, against the list of the
/// resource the decisions were made on, so a decision whose key the list does not hold
/// (an import, an older list) shows on the check it re-attaches to. [`crate::session`]
/// does that.
pub fn plan(items: &[CheckItem], resource: &ResourceRef, stored: &[StoredDecision]) -> Plan {
    let (outcomes, undecided) = placements(items, stored);
    Plan {
        resource: resource.clone(),
        entries: stored
            .iter()
            .zip(outcomes)
            .map(|(d, outcome)| Entry {
                key: d.key.clone(),
                decision: d.decision.clone(),
                outcome,
            })
            .collect(),
        undecided,
    }
}

/// Where each stored decision goes on `items`, and how many checks receive none.
pub(crate) fn placements(items: &[CheckItem], stored: &[StoredDecision]) -> (Vec<Outcome>, usize) {
    let mut placed_item: Vec<bool> = vec![false; items.len()];
    let mut outcomes: Vec<Option<Outcome>> = vec![None; stored.len()];

    // The exact identity goes first, so a decision on an unchanged check never competes
    // with a cross-language match.
    let by_identity: HashMap<(&CheckKey, &str), usize> = stored
        .iter()
        .enumerate()
        .map(|(s, d)| ((&d.key, d.quote.as_str()), s))
        .collect();
    for (i, item) in items.iter().enumerate() {
        if let Some(&s) = by_identity.get(&(&item.key, item.quote.as_str()))
            && outcomes[s].is_none()
        {
            outcomes[s] = Some(Outcome::Stays {
                item: i,
                by: Match::Identity,
            });
            placed_item[i] = true;
        }
    }

    // Only the checks the first pass left open are offered, so a cross-language match
    // never takes a check from its own decision (tC4 #448).
    let open: Vec<usize> = (0..items.len()).filter(|&i| !placed_item[i]).collect();
    let waiting: Vec<usize> = (0..stored.len())
        .filter(|&s| outcomes[s].is_none())
        .collect();
    let attached = across(stored, &waiting, items, &open);

    // tC4 kept one decision per check here and dropped the others from the file; every
    // decision that attaches is placed, and two on one check are a conflict.
    let mut arrivals: HashMap<usize, usize> = HashMap::new();
    for &(_, i, _) in &attached {
        *arrivals.entry(i).or_default() += 1;
    }
    for &(s, i, by) in &attached {
        placed_item[i] = true;
        let to = items[i].key.clone();
        outcomes[s] = Some(if to == stored[s].key {
            Outcome::Stays { item: i, by }
        } else {
            Outcome::Moves {
                item: i,
                to,
                by,
                contested: false,
            }
        });
    }
    // A move is contested when another decision arrives at the same check, or when the
    // decision already at the new key stays there unplaced.
    let unplaced_at: Vec<&CheckKey> = stored
        .iter()
        .zip(&outcomes)
        .filter(|(_, o)| o.is_none())
        .map(|(d, _)| &d.key)
        .collect();
    for outcome in outcomes.iter_mut().flatten() {
        if let Outcome::Moves {
            item,
            to,
            contested,
            ..
        } = outcome
        {
            *contested = arrivals.get(item).is_some_and(|&n| n > 1) || unplaced_at.contains(&&*to);
        }
    }

    (
        outcomes
            .into_iter()
            .map(|o| o.unwrap_or(Outcome::Unplaced))
            .collect(),
        placed_item.iter().filter(|p| !**p).count(),
    )
}

/// tC4's cross-language re-attach on its own (`reattachAcrossResource`): for each stored
/// decision, the check of `items` it attaches to and how, or `None` when no single check
/// matches. [`plan`] runs it on what its first pass leaves.
pub fn reattach(stored: &[StoredDecision], items: &[CheckItem]) -> Vec<Option<(usize, Match)>> {
    let waiting: Vec<usize> = (0..stored.len()).collect();
    let open: Vec<usize> = (0..items.len()).collect();
    let mut out = vec![None; stored.len()];
    for (s, i, by) in across(stored, &waiting, items, &open) {
        out[s] = Some((i, by));
    }
    out
}

/// The second pass over the decisions `waiting` and the checks `open`: by check id when
/// the quote names the same words, else by place, quote and occurrence with the group id
/// breaking a tie. Several decisions can attach to one check.
fn across(
    stored: &[StoredDecision],
    waiting: &[usize],
    items: &[CheckItem],
    open: &[usize],
) -> Vec<(usize, usize, Match)> {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for &i in open {
        // A later check with the same id replaces an earlier one, as in tC4's Map.
        by_id.insert(items[i].key.check_id.as_str(), i);
    }
    let mut attached = Vec::new();
    for &s in waiting {
        let d = &stored[s];
        if let Some(&i) = by_id.get(d.key.check_id.as_str())
            && same_quote(items[i].quote.as_str(), &d.quote)
        {
            attached.push((s, i, Match::CheckId));
            continue;
        }
        let wanted = place_of(&d.key.target, &d.quote, d.key.occurrence);
        let mut candidates: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&i| {
                place_of(
                    &items[i].key.target,
                    items[i].quote.as_str(),
                    items[i].key.occurrence,
                ) == wanted
            })
            .collect();
        if candidates.len() > 1 {
            candidates.retain(|&i| items[i].group_id == d.group_id);
        }
        if let [i] = candidates[..] {
            attached.push((s, i, Match::Place));
        }
    }
    attached
}

/// The numbers the confirmation dialogs show for one (tool, collection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Decisions placed on the new list.
    pub carried: usize,
    /// Of those, the ones that count as decided after the change (tC4 #580: the
    /// figure the dialog shows, so it equals what the check screen will say).
    pub decided_after: usize,
    /// Decisions that held against the current resource and would no longer: the
    /// checks to check again.
    pub invalidated: usize,
    /// Decisions that already did not hold and still find no check.
    pub still_unplaced: usize,
    /// Decisions whose move meets another decision; the person will choose.
    pub contested: usize,
    /// Checks on the new list with no decision.
    pub undecided: usize,
}

impl Plan {
    /// The dialog's numbers. `current` is the resource the (tool, collection) resolves
    /// to before the change; `text` gives the text of a check's target by the no-text rule
    /// (§7.7), so `decided_after` counts what the check screen will show.
    pub fn summary<'t>(
        &self,
        current: Option<&ResourceRef>,
        text: impl Fn(&CheckTarget) -> Option<&'t TargetText>,
    ) -> Summary {
        let mut out = Summary {
            undecided: self.undecided,
            ..Summary::default()
        };
        for entry in &self.entries {
            let Some(at) = entry.placed_at() else {
                if current.is_some_and(|c| c.same_resource(&entry.decision.resource)) {
                    out.invalidated += 1;
                } else {
                    out.still_unplaced += 1;
                }
                continue;
            };
            out.carried += 1;
            if matches!(
                entry.outcome,
                Outcome::Moves {
                    contested: true,
                    ..
                }
            ) {
                out.contested += 1;
            }
            let after = DecisionRecord {
                resource: self.resource.clone(),
                ..entry.decision.clone()
            };
            let state = CheckState::of(Some(&after), Some(&self.resource), text(&at.target));
            if state.is_decided() {
                out.decided_after += 1;
            }
        }
        out
    }
}
