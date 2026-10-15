//! The Check rail: progress, the filter chips, the sort modes and where a tool opens
//! (tC4 `checkFilters.js`, `railGroupsOf` in `Check.jsx`, `checkStartIndex`).
//!
//! The chips' counts and the rows they show come from one predicate per chip, and
//! progress uses the one counting rule, [`CheckState::is_decided`], so the numbers and
//! the list can never disagree.

use std::cmp::Ordering;

use crate::item::{Category, CheckItem};
use crate::js;
use crate::key::ToolId;
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
    let mut rows: Vec<usize> = (0..items.len())
        .filter(|&i| states.get(i).is_some_and(|s| filter.matches(s)))
        .collect();
    // The sort is stable, as JavaScript's is, so the checks of one verse keep the
    // resource's order.
    rows.sort_by(|&a, &b| {
        let (ac, av) = items[a].key.target.parts();
        let (bc, bv) = items[b].key.target.parts();
        let by = |x: &str, y: &str| ref_number(x).partial_cmp(&ref_number(y));
        by(&ac, &bc)
            .filter(|o| o.is_ne())
            .or_else(|| by(&av, &bv))
            .unwrap_or(Ordering::Equal)
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
            terms.sort_by(|a, b| locale_cmp(a, b));
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
            let mut categories: Vec<Category> = Vec::new();
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
    let primary = |s: &str| s.chars().map(primary_weight).collect::<Vec<u32>>();
    let tertiary = |s: &str| {
        s.chars()
            .map(|c| u8::from(c.is_ascii_uppercase()))
            .collect::<Vec<u8>>()
    };
    primary(a)
        .cmp(&primary(b))
        .then_with(|| tertiary(a).cmp(&tertiary(b)))
        .then_with(|| a.cmp(b))
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
        .and_then(|pos| items.iter().position(|it| Position::of(it) == *pos))
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
