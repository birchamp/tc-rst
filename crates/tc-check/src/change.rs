//! What a change of pins would cost, counted before the person confirms it (tC4
//! `consequencesOfGatewayChange` and `uncoveredByChange`, D23a and D30.2). This is the one
//! counter of it: a gateway change, a release upgrade and a re-pin all count here.
//!
//! A decision is at risk when it holds against the resource its (tool, collection)
//! resolves to now and the change moves that resolution elsewhere. A book the new
//! primary does not cover keeps resolving to the fallback it was checked against, so it
//! is not affected. A decision that already does not hold against the current
//! resolution (one a carry-over could not place, which keeps its old resource) is not at
//! risk again: tC4 rewrote such a decision's resource in the file, so it never counted
//! it twice. Here it is counted apart, as revived when the change resolves its unit back
//! to the resource it was made against and as detached otherwise. A change that would
//! leave an affected unit with no usable resource is blocked, because its decisions
//! would then match no rung.
//!
//! A Words list of a book is also filtered by the articles of the translation words that
//! the language set answering it pins (tC4 `withArticle`, #643): a link to an article that
//! resource lacks is no check. So a decision on a book's Words list is also at risk when
//! the change keeps the list's resource but moves that translation words pin to one
//! without the decision's article: its check leaves the list (tC4 #661). The article sets
//! come from the caller's `articles`, read only when that pin moves. tC4 4.0 counts only
//! the list's resource, so its dialog says nothing changes while the check leaves the
//! list unannounced; the rebuild counts it (a declared difference).
//!
//! Both sides resolve with the journal's ladder ([`Ladder::answer`]) and compare resources
//! as the fold does ([`ResourceRef::same_resource`]).

use std::collections::BTreeSet;

use tc_helps::{Slug, WordArticleId, WordCategory};
use tc_journal::pins::{Answer, Coverage, Ladder, SetSlot, Unrecorded};
use tc_journal::record::ResourceRef;

use crate::carry::{StoredDecision, Summary};
use crate::key::{Collection, ToolId};

/// The stored decisions of one (tool, collection).
#[derive(Debug, Clone, Copy)]
pub struct Unit<'a> {
    pub tool: ToolId,
    pub collection: Collection,
    pub decisions: &'a [StoredDecision],
}

/// A (tool, collection) whose decisions the change moves away from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    pub tool: ToolId,
    pub collection: Collection,
    /// The resource it resolves to before the change, which its decisions at risk were
    /// made against.
    pub checked_against: ResourceRef,
    /// How many of its decisions hold against `checked_against` and would no longer: the
    /// list moves to another resource, or their check leaves it.
    pub decisions: usize,
    /// Of `decisions`, those whose check leaves the list because the translation words
    /// that filter it after the change lack the decision's article (tC4 #661). Each is
    /// kept as it is and listed off the list, and shows again as it was when a later
    /// release carries the article again; the carry-over plan counts it in
    /// [`Summary::invalidated`] because no check on the new list is its own.
    pub articles_left: usize,
    /// True when no rung answers it with a usable pin after the change: the change is
    /// refused while any affected unit is blocked.
    pub blocked: bool,
}

/// The cost of a change of pins.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consequences {
    /// Units with decisions, none of which the change puts at risk.
    pub unaffected: usize,
    pub affected: Vec<Affected>,
    /// Decisions that do not hold now and will hold after the change, because it
    /// resolves their unit back to the resource they were made against.
    pub revived: usize,
    /// Decisions that hold neither now nor after the change: made against a resource
    /// their unit resolves to on neither side. They compute as invalidated (resource
    /// changed) either way and are kept.
    pub detached: usize,
}

impl Consequences {
    /// Decisions that hold now against a resource the change moves away from. Some
    /// carry over by meaning; the rest come back as checks to do.
    pub fn at_risk(&self) -> usize {
        self.affected.iter().map(|a| a.decisions).sum()
    }

    /// True when nothing that holds now is disturbed.
    pub fn harmless(&self) -> bool {
        self.affected.is_empty()
    }

    /// True when the change must be refused.
    pub fn blocked(&self) -> bool {
        self.affected.iter().any(|a| a.blocked)
    }
}

/// Counts what changing the pins from `current` to `next` would cost. `local` gives the
/// books an installed copy of a pin holds, for pins that record none. `articles` gives the
/// articles an installed translation words resource carries (what
/// `tc_helps::metadata::BurritoMetadata::word_articles` reads), `None` when they are not
/// known here, which filters nothing, as `derive::word_links_for_book` does with `None`. It
/// is called only for a Words unit of a book whose translation words pin the change moves,
/// at most once per side, so a caller may read it from disk.
pub fn consequences(
    current: &dyn Ladder,
    next: &dyn Ladder,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
    articles: impl Fn(&ResourceRef) -> Option<BTreeSet<WordArticleId>>,
) -> Consequences {
    let mut out = Consequences::default();
    for unit in units {
        if unit.decisions.is_empty() {
            continue;
        }
        let answer = |pins: &dyn Ladder| {
            pins.answer(unit.tool, unit.collection, Unrecorded::Installed(&local))
        };
        let (answer_before, answer_after) = (answer(current), answer(next));
        let before = answer_before.as_ref().and_then(Answer::resource);
        let after = answer_after.as_ref().and_then(Answer::resource);
        let moves_away = moved(before, after);
        let tw_before = filtering_words(current, unit, answer_before.as_ref());
        let tw_after = filtering_words(next, unit, answer_after.as_ref());
        // The article sets are read only when the pin that filters the list moves.
        let gates = moved(tw_before.as_ref(), tw_after.as_ref()).then(|| {
            (
                tw_before.as_ref().and_then(&articles),
                tw_after.as_ref().and_then(&articles),
            )
        });
        let holds = |d: &StoredDecision, on: Option<&ResourceRef>| {
            on.is_some_and(|r| r.same_resource(&d.decision.resource))
        };
        let (mut holding, mut revived, mut at_risk, mut articles_left) = (0, 0, 0, 0);
        for d in unit.decisions {
            if holds(d, before) {
                holding += 1;
                let leaves = gates.as_ref().is_some_and(|(gate_before, gate_after)| {
                    let keys = article_keys(d);
                    on_list(gate_before.as_ref(), &keys) && !on_list(gate_after.as_ref(), &keys)
                });
                if moves_away || leaves {
                    at_risk += 1;
                }
                if leaves {
                    articles_left += 1;
                }
            } else if holds(d, after) {
                revived += 1;
            }
        }
        out.revived += revived;
        out.detached += unit.decisions.len() - holding - revived;
        match before {
            Some(checked_against) if at_risk > 0 => out.affected.push(Affected {
                tool: unit.tool,
                collection: unit.collection,
                checked_against: checked_against.clone(),
                decisions: at_risk,
                articles_left,
                blocked: after.is_none(),
            }),
            _ => out.unaffected += 1,
        }
    }
    out
}

/// True when two sides' resources differ, one being absent included.
fn moved(a: Option<&ResourceRef>, b: Option<&ResourceRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => !a.same_resource(b),
        (a, b) => a.is_some() != b.is_some(),
    }
}

/// The translation words resource whose articles filter `unit`'s list on one side: for a
/// Words list of a book, the pin of the rung whose links answer it, never the other rung's
/// (the resolving set's articles, as the tool's preflight reads them). `None` when there
/// is no such list (a list with no usable links pin has nothing to filter), or the rung
/// holds no usable translation words pin, so no filter is known. Stories lists take every
/// link.
fn filtering_words(
    pins: &dyn Ladder,
    unit: &Unit<'_>,
    answer: Option<&Answer>,
) -> Option<ResourceRef> {
    let answer = answer.filter(|a| a.resource().is_some())?;
    match (unit.tool, unit.collection) {
        (ToolId::TranslationWords, Collection::Book(_)) => pins
            .pin(answer.rung(), SetSlot::TranslationWords)
            .map(|p| p.resource),
        _ => None,
    }
}

/// The article a Words decision is on: its recorded category (`DecisionRecord::category`,
/// which `status::decision_edit` writes from the check and tC4's files carry) and its group
/// id, the article's slug (tC4 `withArticle`: `${category}/${groupId}`). With no recorded
/// category it is the slug in each of the three categories; with a group id that is no
/// slug, none, and the decision then never counts as leaving.
fn article_keys(d: &StoredDecision) -> Vec<WordArticleId> {
    let Ok(slug) = Slug::parse(&d.group_id) else {
        return Vec::new();
    };
    let categories = match d.decision.category.as_deref().and_then(WordCategory::parse) {
        Some(category) => vec![category],
        None => vec![
            WordCategory::KeyTerm,
            WordCategory::Name,
            WordCategory::Other,
        ],
    };
    categories
        .into_iter()
        .map(|category| WordArticleId::Dcs {
            category,
            slug: slug.clone(),
        })
        .collect()
}

/// Whether a check on one of `keys` is on a list filtered by `articles` (`None`: no filter).
fn on_list(articles: Option<&BTreeSet<WordArticleId>>, keys: &[WordArticleId]) -> bool {
    articles.is_none_or(|set| keys.is_empty() || keys.iter().any(|k| set.contains(k)))
}

impl Summary {
    /// The totals of several units' summaries: the whole upgrade's numbers.
    pub fn total(summaries: impl IntoIterator<Item = Summary>) -> Summary {
        summaries
            .into_iter()
            .fold(Summary::default(), |a, b| Summary {
                carried: a.carried + b.carried,
                decided_after: a.decided_after + b.decided_after,
                invalidated: a.invalidated + b.invalidated,
                still_unplaced: a.still_unplaced + b.still_unplaced,
                contested: a.contested + b.contested,
                undecided: a.undecided + b.undecided,
            })
    }
}
