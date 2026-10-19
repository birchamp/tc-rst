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
//! to the resource it was made against and its check is on the new list, and as detached
//! otherwise. A change that would leave an affected unit with no usable resource is
//! blocked, because its decisions would then match no rung.
//!
//! A Words list of a book is also filtered by the articles of the translation words that
//! the language set answering it pins (tC4 `withArticle`, #643): a link to an article that
//! resource lacks is no check. So a decision on a book's Words list is also at risk when
//! the change keeps the list's resource but moves that translation words pin to one
//! without the decision's article: its check leaves the list (tC4 #661). A decision whose
//! article the move brings back to the list is revived. The article sets come from the
//! caller's `articles`, read only when a decision's place on the list depends on them.
//! tC4 4.0 counts only the list's resource, so its dialog says nothing changes while the
//! check leaves the list unannounced; the rebuild counts it (a declared difference). A
//! rung that answers a Words list of a book with an unusable translation words pin closes
//! the list: the tool refuses to open it ([`crate::fix::preflight`]'s `UnusablePin`), so a
//! change that would put decisions that hold now behind such a pin is blocked, as one onto
//! an unusable links pin is, until the pins are repaired.
//!
//! Both sides resolve with the journal's ladder ([`Ladder::answer`]) and compare resources
//! as the fold does ([`ResourceRef::same_resource`]).

use std::collections::BTreeSet;

use tc_helps::{Slug, WordArticleId, WordCategory};
use tc_journal::pins::{Answer, Coverage, Ladder, SetSlot, SlotPin, Unrecorded};
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
    /// True when, after the change, no rung answers it with a usable pin, or the rung that
    /// answers its Words list holds an unusable translation words pin, so the tool cannot
    /// open the list: the change is refused while any affected unit is blocked.
    pub blocked: bool,
}

/// The cost of a change of pins. A decision is in force when it holds against the resource
/// its unit resolves to and its check is on the list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consequences {
    /// Units with decisions, none of which the change puts at risk.
    pub unaffected: usize,
    pub affected: Vec<Affected>,
    /// Decisions not in force now that will be after the change: it resolves their unit
    /// back to the resource they were made against while the new list carries their check,
    /// or it keeps that resource and brings their check back onto the list (the translation
    /// words that filter it after the change carry their article, or open a list an
    /// unusable pin closed).
    pub revived: usize,
    /// Decisions that do not hold now and will not be in force after the change: made
    /// against a resource their unit resolves to on neither side, or resolved back to by
    /// the change while the new list lacks their check. They are kept.
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
/// is called only for the translation words pin that filters a Words list of a book, only
/// when the change moves that pin or resolves decisions of the list back to their
/// resource, and at most once for each resource in one count, so a caller may read it
/// from disk.
pub fn consequences(
    current: &dyn Ladder,
    next: &dyn Ladder,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
    articles: impl Fn(&ResourceRef) -> Option<BTreeSet<WordArticleId>>,
) -> Consequences {
    let mut out = Consequences::default();
    let mut sets = ArticleSets {
        read: articles,
        read_so_far: Vec::new(),
    };
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
        let filter_before = filter(current, unit, answer_before.as_ref());
        let filter_after = filter(next, unit, answer_after.as_ref());
        let filter_moves = !filter_before.same(&filter_after);
        let closes = filter_after == Filter::Closed;
        let holds = |d: &StoredDecision, on: Option<&ResourceRef>| {
            on.is_some_and(|r| r.same_resource(&d.decision.resource))
        };
        let (mut revived, mut detached, mut at_risk, mut articles_left) = (0, 0, 0, 0);
        for d in unit.decisions {
            if holds(d, before) {
                // With the same filter on both sides, a check is on both lists or on neither.
                let (leaves, returns) = if filter_moves {
                    let on_before = sets.on_list(&filter_before, d);
                    let on_after = sets.on_list(&filter_after, d);
                    (on_before && !on_after, !on_before && on_after)
                } else {
                    (false, false)
                };
                if moves_away || leaves {
                    at_risk += 1;
                }
                if leaves && !closes {
                    articles_left += 1;
                }
                // A decision that holds on both sides keeps its resource, so only its
                // place on the list can change.
                if returns && holds(d, after) {
                    revived += 1;
                }
            } else if holds(d, after) && sets.on_list(&filter_after, d) {
                revived += 1;
            } else {
                detached += 1;
            }
        }
        out.revived += revived;
        out.detached += detached;
        match before {
            Some(checked_against) if at_risk > 0 => out.affected.push(Affected {
                tool: unit.tool,
                collection: unit.collection,
                checked_against: checked_against.clone(),
                decisions: at_risk,
                articles_left,
                blocked: after.is_none() || closes,
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

/// Which checks of a unit's list exist on one side of the change.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Filter {
    /// Every check: a list other than a Words list of a book (a Stories list takes every
    /// link), no list at all (no usable links pin answers it), or a rung that pins no
    /// translation words.
    Every,
    /// No check: the rung whose links answer a Words list of a book holds an unusable
    /// translation words pin, and the tool refuses to open the list.
    Closed,
    /// The checks whose article this translation words resource carries: the pin of the
    /// rung whose links answer the list, never the other rung's (the resolving set's
    /// articles, as the tool's preflight reads them).
    Articles(ResourceRef),
}

impl Filter {
    /// Whether two filters keep the same checks, comparing resources as the fold does.
    fn same(&self, other: &Filter) -> bool {
        match (self, other) {
            (Filter::Articles(a), Filter::Articles(b)) => a.same_resource(b),
            (a, b) => a == b,
        }
    }
}

/// The filter of `unit`'s list on the side whose ladder is `pins` and answer `answer`.
fn filter(pins: &dyn Ladder, unit: &Unit<'_>, answer: Option<&Answer>) -> Filter {
    let Some(answer) = answer.filter(|a| a.resource().is_some()) else {
        return Filter::Every;
    };
    match (unit.tool, unit.collection) {
        (ToolId::TranslationWords, Collection::Book(_)) => {
            match pins.held(answer.rung(), SetSlot::TranslationWords) {
                None => Filter::Every,
                Some(SlotPin::Usable(pin)) => Filter::Articles(pin.resource),
                Some(SlotPin::Unusable(_)) => Filter::Closed,
            }
        }
        _ => Filter::Every,
    }
}

/// The caller's article sets, each translation words resource read the first time a
/// decision's place on a list needs it, and only then.
struct ArticleSets<F> {
    read: F,
    read_so_far: Vec<(ResourceRef, Option<BTreeSet<WordArticleId>>)>,
}

impl<F: Fn(&ResourceRef) -> Option<BTreeSet<WordArticleId>>> ArticleSets<F> {
    /// Whether `d`'s check is on a list kept by `filter`.
    fn on_list(&mut self, filter: &Filter, d: &StoredDecision) -> bool {
        let resource = match filter {
            Filter::Every => return true,
            Filter::Closed => return false,
            Filter::Articles(resource) => resource,
        };
        let at = match self
            .read_so_far
            .iter()
            .position(|(r, _)| r.same_resource(resource))
        {
            Some(at) => at,
            None => {
                let set = (self.read)(resource);
                self.read_so_far.push((resource.clone(), set));
                self.read_so_far.len() - 1
            }
        };
        on_list(self.read_so_far[at].1.as_ref(), &article_keys(d))
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
