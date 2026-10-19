//! The Check rail: progress, the filter chips, the sort modes and where a tool opens
//! (tC4 `checkFilters.js`, `railGroupsOf` in `Check.jsx`, `checkStartIndex`).
//!
//! The chips' counts and the rows they show come from one predicate per chip, and
//! progress uses the one counting rule, [`CheckState::is_decided`], so the numbers and
//! the list can never disagree.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::Work;
use crate::item::{Category, CheckItem};
use crate::js;
use crate::key::{CheckTarget, ToolId};
use crate::status::{CheckState, UserStatus};

/// Decided checks over the checks on the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub decided: usize,
    pub total: usize,
}

pub fn progress(states: &[CheckState]) -> Progress {
    Progress {
        decided: states.iter().filter(|s| s.is_decided()).count(),
        total: states.len(),
    }
}

impl Progress {
    /// The progress after one check's state changed from `before` to `after`: a decision
    /// recorded, or a verse edit that invalidated one. It reads the two states and nothing
    /// else, so the counts of a list of any length update in the same time; it equals
    /// [`progress`] over the changed states.
    pub fn update(&mut self, before: &CheckState, after: &CheckState) {
        match (before.is_decided(), after.is_decided()) {
            (false, true) => self.decided += 1,
            (true, false) => self.decided = self.decided.saturating_sub(1),
            _ => {}
        }
    }
}

/// The rail's filter chips, in the order they appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    All,
    /// Not decided by [`CheckState::is_decided`]: every invalidated check, and every
    /// check without selected words, nothing-to-select or an Invalid.
    ToDo,
    /// The status shown is Invalid: the person's Invalid, or an invalidated decision.
    Invalid,
    Bookmarked,
    /// The check has a non-empty comment.
    Commented,
}

impl Filter {
    pub const ORDER: [Filter; 5] = [
        Filter::All,
        Filter::ToDo,
        Filter::Invalid,
        Filter::Bookmarked,
        Filter::Commented,
    ];

    pub fn matches(self, state: &CheckState) -> bool {
        match self {
            Self::All => true,
            Self::ToDo => !state.is_decided(),
            Self::Invalid => state.status() == Some(UserStatus::Invalid),
            Self::Bookmarked => state.bookmarked,
            Self::Commented => state.commented,
        }
    }
}

/// Each chip's count, in chip order.
pub fn counts(states: &[CheckState]) -> [(Filter, usize); 5] {
    Filter::ORDER.map(|f| (f, states.iter().filter(|s| f.matches(s)).count()))
}

/// How the rail orders the checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortMode {
    /// One group in reference order ("By frame" for a story).
    ByVerse,
    /// Grouped by term, terms in alphabetical order (tW).
    ByWord,
    /// Grouped by tN category, categories in order of first appearance.
    ByCategory,
}

impl SortMode {
    /// The modes a tool offers; the first is its default.
    pub fn modes(tool: ToolId) -> &'static [SortMode] {
        match tool {
            ToolId::TranslationWords => &[SortMode::ByWord, SortMode::ByVerse],
            ToolId::TranslationNotes => &[SortMode::ByVerse, SortMode::ByCategory],
            ToolId::TranslationQuestions => &[SortMode::ByVerse],
        }
    }

    pub fn default_for(tool: ToolId) -> Self {
        Self::modes(tool)[0]
    }
}

/// The heading over a group of rail rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupLabel {
    /// The book or story name, which the screen supplies.
    Unit,
    /// A tW term: the article slug.
    Term(String),
    Category(Category),
}

/// A group of rail rows: indices into the check list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailGroup {
    pub label: GroupLabel,
    pub rows: Vec<usize>,
}

/// The rail's groups for the checks the filter shows, in the order the rail shows them
/// (which is also the order the arrow keys move through).
pub fn groups(
    items: &[CheckItem],
    states: &[CheckState],
    filter: Filter,
    mode: SortMode,
) -> Vec<RailGroup> {
    groups_counted(items, states, filter, mode, &mut Work::default())
}

/// [`groups`], adding what it touched to `work`: each check is tested once, its sort key
/// read once and the row put into its group once, and the sorts compare cached keys
/// (`rows_tested`, `keys_read`, `comparisons`, `bucketed`).
pub fn groups_counted(
    items: &[CheckItem],
    states: &[CheckState],
    filter: Filter,
    mode: SortMode,
    work: &mut Work,
) -> Vec<RailGroup> {
    let mut keyed: Vec<(f64, f64, usize)> = Vec::new();
    for (i, (item, state)) in items.iter().zip(states).enumerate() {
        work.rows_tested += 1;
        if filter.matches(state) {
            work.keys_read += 1;
            let (chapter, verse) = sort_key(&item.key.target);
            keyed.push((chapter, verse, i));
        }
    }
    // The sort is stable, as JavaScript's is, so the checks of one verse keep the
    // resource's order. The keys are numbers (a part that is not one sorts first), so the
    // pair compares as the two parts do one after the other.
    keyed.sort_by(|a, b| {
        work.comparisons += 1;
        number_order(a.0, b.0).then_with(|| number_order(a.1, b.1))
    });
    let rows: Vec<usize> = keyed.into_iter().map(|(_, _, i)| i).collect();
    match mode {
        SortMode::ByVerse => vec![RailGroup {
            label: GroupLabel::Unit,
            rows,
        }],
        SortMode::ByWord => {
            let (terms, buckets) = bucket(&rows, |i| items[i].group_id.as_str(), work);
            let mut order: Vec<(LocaleKey, usize)> = terms
                .iter()
                .enumerate()
                .map(|(b, term)| (LocaleKey::of(term), b))
                .collect();
            order.sort_by(|(a, _), (b, _)| {
                work.comparisons += 1;
                a.cmp(b)
            });
            let mut buckets: Vec<Option<Vec<usize>>> = buckets.into_iter().map(Some).collect();
            order
                .into_iter()
                .map(|(_, b)| RailGroup {
                    label: GroupLabel::Term(terms[b].to_owned()),
                    rows: buckets[b].take().unwrap_or_default(),
                })
                .collect()
        }
        SortMode::ByCategory => {
            let (categories, buckets) = bucket(&rows, |i| items[i].category, work);
            categories
                .into_iter()
                .zip(buckets)
                .map(|(category, rows)| RailGroup {
                    label: GroupLabel::Category(category),
                    rows,
                })
                .collect()
        }
    }
}

/// Puts each of `rows`, in order, into the group `group_of` names, once: the groups in
/// order of first appearance, and each group's rows.
fn bucket<K: std::hash::Hash + Eq + Copy>(
    rows: &[usize],
    group_of: impl Fn(usize) -> K,
    work: &mut Work,
) -> (Vec<K>, Vec<Vec<usize>>) {
    let mut at: HashMap<K, usize> = HashMap::new();
    let mut groups: Vec<K> = Vec::new();
    let mut buckets: Vec<Vec<usize>> = Vec::new();
    for &i in rows {
        work.bucketed += 1;
        let key = group_of(i);
        let b = *at.entry(key).or_insert_with(|| {
            groups.push(key);
            buckets.push(Vec::new());
            buckets.len() - 1
        });
        buckets[b].push(i);
    }
    (groups, buckets)
}

/// The two numbers a check sorts by: its chapter and verse, or its story and frame.
fn sort_key(target: &CheckTarget) -> (f64, f64) {
    match target {
        CheckTarget::Verse { chapter, verse, .. } => {
            (ref_number(chapter.as_str()), ref_number(verse.as_str()))
        }
        CheckTarget::Frame { story, frame } => (f64::from(story.get()), f64::from(frame.0)),
    }
}

/// Finite numbers in order; two equal numbers (and `0` with `-0`) tie.
fn number_order(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// tC4 `refNum`: a part that is a finite number sorts by it; any other text sorts as -1,
/// ahead of every verse.
fn ref_number(part: &str) -> f64 {
    let n = js::to_number(part);
    if n.is_finite() { n } else { -1.0 }
}

/// `String.prototype.localeCompare` in the root locale, for the slugs tW terms are:
/// letters compare without case first (`a` < `B` < `c`), digits come before letters,
/// and ASCII punctuation and symbols come before digits in the root collation's order.
/// Lower case sorts before upper case when the letters are equal. Characters outside
/// ASCII sort after ASCII letters, by code point.
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    LocaleKey::of(a).cmp(&LocaleKey::of(b))
}

/// The weights [`locale_cmp`] compares, made once for a term so a sort of many terms
/// builds them once each, not once per comparison.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LocaleKey {
    primary: Vec<u32>,
    tertiary: Vec<u8>,
    /// Equal weights fall back to the text, so distinct terms never tie.
    text: String,
}

impl LocaleKey {
    fn of(s: &str) -> Self {
        Self {
            primary: s.chars().map(primary_weight).collect(),
            tertiary: s
                .chars()
                .map(|c| u8::from(c.is_ascii_uppercase()))
                .collect(),
            text: s.to_owned(),
        }
    }
}

/// ASCII punctuation and symbols in the order of the CLDR root collation.
const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

fn primary_weight(c: char) -> u32 {
    if js::is_whitespace(c) {
        return u32::from(c);
    }
    if let Some(i) = PUNCTUATION.find(c) {
        return 0x1000 + i as u32;
    }
    if c.is_ascii_digit() {
        return 0x2000 + u32::from(c);
    }
    if c.is_ascii_alphabetic() {
        return 0x3000 + u32::from(c.to_ascii_lowercase());
    }
    0x10_0000 + u32::from(c)
}

/// Where the checker last was in a tool: the check's place, group id and occurrence
/// (tC4 `checkPosOf`). Kept per person, never in the project.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    pub chapter: String,
    pub verse: String,
    pub group_id: String,
    pub occurrence: u32,
}

impl Position {
    /// Whether `item` is the check this position names, compared in place: a list of
    /// thousands is searched without building a position for each check.
    pub fn is_at(&self, item: &CheckItem) -> bool {
        if self.group_id != item.group_id || self.occurrence != item.key.occurrence {
            return false;
        }
        match &item.key.target {
            CheckTarget::Verse { chapter, verse, .. } => {
                self.chapter == chapter.as_str() && self.verse == verse.as_str()
            }
            target @ CheckTarget::Frame { .. } => {
                let (chapter, verse) = target.parts();
                self.chapter == chapter && self.verse == verse
            }
        }
    }

    pub fn of(item: &CheckItem) -> Self {
        let (chapter, verse) = item.key.target.parts();
        Self {
            chapter,
            verse,
            group_id: item.group_id.clone(),
            occurrence: item.key.occurrence,
        }
    }
}

/// The check a tool opens at: the remembered one, else the first not decided, else the
/// first (tC4 `checkStartIndex`).
pub fn start_index(
    items: &[CheckItem],
    states: &[CheckState],
    remembered: Option<&Position>,
) -> usize {
    remembered
        .and_then(|pos| items.iter().position(|it| pos.is_at(it)))
        .or_else(|| next_undecided(states))
        .unwrap_or(0)
}

/// The first check on the list that is not decided: the tool card's "Next".
pub fn next_undecided(states: &[CheckState]) -> Option<usize> {
    states.iter().position(|s| !s.is_decided())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_sort_as_locale_compare_does() {
        let mut terms = vec![
            "god",
            "Abel",
            "high-priest",
            "highpriest",
            "high_priest",
            "10",
            "a",
        ];
        terms.sort_by(|a, b| locale_cmp(a, b));
        assert_eq!(
            terms,
            vec![
                "10",
                "a",
                "Abel",
                "god",
                "high_priest",
                "high-priest",
                "highpriest"
            ]
        );
    }

    #[test]
    fn non_numbers_sort_first() {
        assert_eq!(ref_number("front"), -1.0);
        assert_eq!(ref_number("9-10"), -1.0);
        assert_eq!(ref_number("12"), 12.0);
    }
}
