//! The Check rail's filter chips, sort modes and groups, and the patch of its rows (tC4
//! `checkFilters.js` and `railGroupsOf`, APP-DESIGN §3.8, AD17).
//!
//! The rows arrive in the order By verse lists them, so a filter keeps that order and a
//! grouping needs no sort: rows go into buckets by term index (By word), or by figure in
//! order of first appearance among the rows shown (By figure, tC4 #769), in one pass over the rows
//! the filter keeps (A-13). tc-app holds this to `tc_check::rail::groups`, which filters,
//! sorts and groups as tC4 does, for every filter and mode on Titus and Psalms.

use crate::check::{Dot, RailRow, RailRowPatch, RowFlags, Tool};

/// The rail's filter chips, in the order they appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    All,
    /// Not decided (tC4 `isDecided`).
    ToDo,
    /// The status shown is Invalid.
    Invalid,
    Bookmarked,
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

    /// One predicate per chip: the counts and the rows shown come from it.
    pub fn matches(self, flags: &RowFlags) -> bool {
        match self {
            Self::All => true,
            Self::ToDo => !flags.decided,
            Self::Invalid => flags.invalid,
            Self::Bookmarked => flags.bookmarked,
            Self::Commented => flags.commented,
        }
    }

    /// tC4's chip id, which its test ids and catalog keys use.
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ToDo => "todo",
            Self::Invalid => "invalid",
            Self::Bookmarked => "bookmarked",
            Self::Commented => "commented",
        }
    }
}

/// Each chip's count, in chip order.
pub fn counts(rows: &[RailRow]) -> [(Filter, u32); 5] {
    Filter::ORDER.map(|f| {
        let n = rows.iter().filter(|r| f.matches(&r.flags)).count();
        (f, u32::try_from(n).unwrap_or(u32::MAX))
    })
}

/// Decided rows: the progress the rail's header shows.
pub fn decided(rows: &[RailRow]) -> u32 {
    let n = rows.iter().filter(|r| r.flags.decided).count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// How the rail orders the checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortMode {
    /// One group in reference order ("By frame" for a story).
    ByVerse,
    /// Grouped by term, terms in the order of the session's `terms` (tW).
    ByWord,
    /// Grouped by figure (the note's Translation Academy article, its term), in order of
    /// first appearance among the rows shown (tN, tC4 #769).
    ByFigure,
}

impl SortMode {
    /// The modes a tool offers; the first is its default (tC4's `SORTS`).
    pub fn modes(tool: Tool) -> &'static [SortMode] {
        match tool {
            Tool::TranslationWords => &[SortMode::ByWord, SortMode::ByVerse],
            Tool::TranslationNotes => &[SortMode::ByVerse, SortMode::ByFigure],
        }
    }

    pub fn default_for(tool: Tool) -> Self {
        Self::modes(tool)[0]
    }

    /// tC4's mode id, which its test ids and catalog keys use.
    pub fn id(self) -> &'static str {
        match self {
            Self::ByVerse => "byVerse",
            Self::ByWord => "byWord",
            Self::ByFigure => "byFigure",
        }
    }
}

/// The heading over a group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupLabel {
    /// The book or story name, which the screen supplies; also the heading of rows a
    /// grouping has no key for.
    Unit,
    /// A tW term: an index into the session's terms.
    Term(u32),
    /// A tN figure: an index into the session's terms, which hold each note's article slug
    /// (empty for notes with none, "Other notes").
    Figure(u32),
}

/// A group of the rail: positions into the rows it was made from, in rail order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailGroup {
    pub label: GroupLabel,
    pub rows: Vec<usize>,
}

/// The groups of the rows `filter` keeps, in the order the rail shows them, which is also the
/// order the arrow keys move through. `rows` is the rail in By verse order; `terms` is the
/// number of the session's terms. Each row is read once.
pub fn groups<'a>(
    rows: impl IntoIterator<Item = &'a RailRow>,
    terms: usize,
    filter: Filter,
    mode: SortMode,
) -> Vec<RailGroup> {
    let kept = rows
        .into_iter()
        .enumerate()
        .filter(|(_, row)| filter.matches(&row.flags));
    match mode {
        SortMode::ByVerse => vec![RailGroup {
            label: GroupLabel::Unit,
            rows: kept.map(|(at, _)| at).collect(),
        }],
        SortMode::ByWord => {
            let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); terms];
            let mut unkeyed = Vec::new();
            for (at, row) in kept {
                match row
                    .term
                    .and_then(|t| buckets.get_mut(usize::try_from(t).ok()?))
                {
                    Some(bucket) => bucket.push(at),
                    None => unkeyed.push(at),
                }
            }
            let mut out: Vec<RailGroup> = buckets
                .into_iter()
                .enumerate()
                .filter(|(_, rows)| !rows.is_empty())
                .map(|(term, rows)| RailGroup {
                    label: GroupLabel::Term(u32::try_from(term).unwrap_or(u32::MAX)),
                    rows,
                })
                .collect();
            if !unkeyed.is_empty() {
                out.push(RailGroup {
                    label: GroupLabel::Unit,
                    rows: unkeyed,
                });
            }
            out
        }
        SortMode::ByFigure => {
            let mut out: Vec<RailGroup> = Vec::new();
            let mut at: std::collections::HashMap<Option<u32>, usize> =
                std::collections::HashMap::new();
            for (row_at, row) in kept {
                match at.get(&row.term).copied().and_then(|g| out.get_mut(g)) {
                    Some(group) => group.rows.push(row_at),
                    None => {
                        at.insert(row.term, out.len());
                        out.push(RailGroup {
                            label: row.term.map_or(GroupLabel::Unit, GroupLabel::Figure),
                            rows: vec![row_at],
                        });
                    }
                }
            }
            out
        }
    }
}

/// Whether a sort's groups fold (tC4 `FOLDING_SORTS`): By word and By figure.
pub fn folds(mode: SortMode) -> bool {
    matches!(mode, SortMode::ByWord | SortMode::ByFigure)
}

/// The id a group folds by: its term's slug (`terms`), for the groups of a folding sort.
pub fn group_id(label: GroupLabel, terms: &[String]) -> Option<&str> {
    match label {
        GroupLabel::Term(t) | GroupLabel::Figure(t) => {
            terms.get(usize::try_from(t).ok()?).map(String::as_str)
        }
        GroupLabel::Unit => None,
    }
}

/// Which of `groups` are open: under a folding sort, a group is open unless its id is in
/// `folded`, and the group of the active check (`active`, its group id) always is (tC4 #769,
/// INV-THE-ACTIVE-GROUP-IS-OPEN). Costs the groups, not the rows.
pub fn open_groups(
    groups: &[RailGroup],
    terms: &[String],
    folded: &std::collections::BTreeSet<String>,
    active: Option<&str>,
    mode: SortMode,
) -> Vec<bool> {
    groups
        .iter()
        .map(|g| {
            !folds(mode)
                || group_id(g.label, terms)
                    .is_none_or(|id| !folded.contains(id) || Some(id) == active)
        })
        .collect()
}

/// Fold all, or Unfold all once every other shown group is folded (tC4 `foldAll`): the ids to
/// set and whether they fold. Fold all leaves the active check's group (`active`) open.
pub fn fold_all(
    groups: &[RailGroup],
    terms: &[String],
    folded: &std::collections::BTreeSet<String>,
    active: Option<&str>,
) -> (Vec<String>, bool) {
    let ids: Vec<&str> = groups
        .iter()
        .filter_map(|g| group_id(g.label, terms))
        .collect();
    let others: Vec<String> = ids
        .iter()
        .filter(|id| Some(**id) != active)
        .map(|id| (*id).to_owned())
        .collect();
    if others.iter().all(|id| folded.contains(id)) {
        (ids.into_iter().map(str::to_owned).collect(), false)
    } else {
        (others, true)
    }
}

/// Each term's decided and total rows over the whole rail, whatever the filter shows: the
/// counts a folding heading shows (tC4 `groupStatsOf`). One pass over the rows.
pub fn term_progress(rows: &[RailRow], terms: usize) -> Vec<(u32, u32)> {
    let mut out = vec![(0u32, 0u32); terms];
    for row in rows {
        if let Some(slot) = row.term.and_then(|t| out.get_mut(usize::try_from(t).ok()?)) {
            slot.1 += 1;
            slot.0 += u32::from(row.flags.decided);
        }
    }
    out
}

/// Replaces each row a patch names, by its index. False when a patch names a row the rail
/// does not hold: the rail is of another session, and the UI opens the tool again.
pub fn patch(rows: &mut [RailRow], patches: &[RailRowPatch]) -> bool {
    if patches.is_empty() {
        return true;
    }
    let at: std::collections::HashMap<u32, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.index, position))
        .collect();
    let mut all = true;
    for p in patches {
        match at.get(&p.row.index).copied().and_then(|i| rows.get_mut(i)) {
            Some(row) => *row = p.row.clone(),
            None => all = false,
        }
    }
    all
}

/// The dot's state as tC4's row writes it in `data-status`.
pub fn dot_id(dot: Dot) -> &'static str {
    match dot {
        Dot::Valid => "valid",
        Dot::Invalid => "invalid",
        Dot::Todo => "todo",
    }
}
