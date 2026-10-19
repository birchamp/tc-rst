//! The Check screen's decisions (unit B5a; tC4 `Check.jsx`), pure and tested on the host: which
//! of the picker and a tool shows, the picker cards' words, the rail's keys and its virtual
//! list.
//!
//! - **The screen** is the picker or one tool's session (tC4 `s.checkTool`). An open that lands
//!   in Check with a remembered tool shows that tool at once, so a resume never shows a picker
//!   to click while the tool opens (#518), and asks for it once (#513).
//! - **The cards** follow tC4's `ctaFor`, `nextLineFor` and `CardProgress`.
//! - **The rail's keys** are tC4's `railKeyDown`: the list is one Tab stop; Up and Down move in
//!   the order the rail shows and wrap, Home and End jump.
//! - **The virtual list** (A-24): a rail longer than [`VIRTUAL_FROM`] rows draws only the rows in
//!   view, at offsets computed from two fixed heights, and keeps the tab-stop row mounted
//!   wherever it is. tC4 draws every row; a Psalms rail has 8,212.

use std::ops::Range;

use tc_api::check::{AlignProgress, CardCount, CheckPlace, CheckProgress, Tool};
use tc_api::prefs::PlaceMode;

/// What the Check screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckView {
    Picker,
    Tool(Tool),
}

/// Where an open lands in Check: the remembered tool, at once, else the picker.
pub fn landing(mode: PlaceMode, tool: Option<Tool>) -> CheckView {
    match (mode, tool) {
        (PlaceMode::Check, Some(tool)) => CheckView::Tool(tool),
        _ => CheckView::Picker,
    }
}

/// The CTA's words by progress (tC4 `ctaFor`): Start, Continue or Review; "Open this tool"
/// while the count is unknown, failed or empty.
pub fn cta_key(done: Option<(u32, u32)>) -> &'static str {
    match done {
        None | Some((_, 0)) => "check.open",
        Some((0, _)) => "check.cta.start",
        Some((done, total)) if done >= total => "check.cta.review",
        Some(_) => "check.cta.continue",
    }
}

/// A helps tool's count as the CTA reads it.
pub fn check_done(progress: Option<&CardCount<CheckProgress>>) -> Option<(u32, u32)> {
    match progress? {
        CardCount::Counted(p) => Some((p.decided, p.total)),
        CardCount::Failed { .. } => None,
    }
}

/// The Align card's count as the CTA reads it.
pub fn align_done(progress: &CardCount<AlignProgress>) -> Option<(u32, u32)> {
    match progress {
        CardCount::Counted(p) => Some((p.done, p.total)),
        CardCount::Failed { .. } => None,
    }
}

/// The count line of a card (tC4 `CardProgress`'s `countLabel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountLine {
    /// Still loading: "—".
    Pending,
    /// tC4's `check.progressError`, in the warn colour.
    Failed,
    /// `check.noItems`.
    NoItems,
    /// `{decided} of {total} resolved`, or for Align `{done} of {total} verses aligned`.
    Counted { done: u32, total: u32 },
}

pub fn count_line<T>(
    progress: Option<&CardCount<T>>,
    done: impl Fn(&T) -> (u32, u32),
) -> CountLine {
    match progress {
        None => CountLine::Pending,
        Some(CardCount::Failed { .. }) => CountLine::Failed,
        Some(CardCount::Counted(p)) => match done(p) {
            (_, 0) => CountLine::NoItems,
            (done, total) => CountLine::Counted { done, total },
        },
    }
}

/// The line under the count (tC4 `nextLineFor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextLine {
    None,
    /// `check.allResolved`.
    AllResolved,
    /// "Next: “{title}” · {c}:{v}".
    Check {
        title: String,
        chapter: String,
        verse: String,
    },
    /// "Next: verse {ref}".
    Verse(String),
}

pub fn next_check(progress: Option<&CardCount<CheckProgress>>) -> NextLine {
    let Some(CardCount::Counted(p)) = progress else {
        return NextLine::None;
    };
    if p.total == 0 {
        return NextLine::None;
    }
    if p.decided >= p.total {
        return NextLine::AllResolved;
    }
    match &p.next {
        Some(next) => {
            let (chapter, verse) = place_parts(&next.place);
            NextLine::Check {
                title: next.title.clone(),
                chapter,
                verse,
            }
        }
        None => NextLine::None,
    }
}

pub fn next_verse(progress: &CardCount<AlignProgress>) -> NextLine {
    let CardCount::Counted(p) = progress else {
        return NextLine::None;
    };
    if p.total == 0 {
        return NextLine::None;
    }
    if p.done >= p.total {
        return NextLine::AllResolved;
    }
    p.next.clone().map_or(NextLine::None, NextLine::Verse)
}

/// tC4's `referenceParts`: the chapter and verse of a book's check, the story and frame of a
/// story's.
pub fn place_parts(place: &CheckPlace) -> (String, String) {
    match place {
        CheckPlace::Verse { chapter, verse, .. } => {
            (chapter.as_str().to_owned(), verse.as_str().to_owned())
        }
        CheckPlace::Frame { story, frame } => (story.get().to_string(), frame.to_string()),
    }
}

/// The share a check bar fills, 0 to 100.
pub fn bar_value(done: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(done) / f64::from(total) * 100.0
    }
}

/// The smallest fill a check bar draws once one check is resolved (tC4 #764, a declared
/// difference): 1 of 112 is under a pixel's sliver of tC4's bar, which reads as nothing done.
/// As tall as the picker's 6 px bar, so the fill shows as a dot.
pub const MIN_FILL_PX: f64 = 6.0;

/// The minimum fill for `done` checks: none until one is resolved.
pub fn min_fill(done: u32) -> Option<f64> {
    (done > 0).then_some(MIN_FILL_PX)
}

// ---- The rail's keys -------------------------------------------------------------------

/// A key on the rail's list (tC4 `railKeyDown`): the check to select, or `None` when the key
/// is not the list's. `order` is the rail's order of check indices; `current` the selected one,
/// which a filter may hide: Up and Down then select the first row shown.
pub fn rail_key(order: &[u32], current: Option<u32>, key: &str) -> Option<u32> {
    if order.is_empty() {
        return None;
    }
    let last = order.len() - 1;
    let at = current.and_then(|c| order.iter().position(|&i| i == c));
    let next = match key {
        "Home" => 0,
        "End" => last,
        "ArrowDown" => at.map_or(0, |p| if p == last { 0 } else { p + 1 }),
        "ArrowUp" => at.map_or(0, |p| if p == 0 { last } else { p - 1 }),
        _ => return None,
    };
    order.get(next).copied()
}

/// The row that is the list's one Tab stop: the selected check when the rail shows it, else
/// the first row shown.
pub fn tab_stop(order: &[u32], current: Option<u32>) -> Option<u32> {
    current
        .filter(|c| order.contains(c))
        .or_else(|| order.first().copied())
}

// ---- The virtual list ------------------------------------------------------------------

/// Rails of this many rows or fewer draw whole, as tC4 does (§6.11, Q-12).
pub const VIRTUAL_FROM: usize = 400;

/// A row's height in a virtual rail: one line of title (`Check.jsx`'s row: 10 px of padding
/// above and below, a 1 px border and a 17 px line), with the 6 px gap below it.
pub const ROW_HEIGHT: f64 = 39.0 + 6.0;
/// A group heading's height in a virtual rail: its line and the 6 px under it, then the 16 px
/// gap between groups above the next.
pub const HEADER_HEIGHT: f64 = 16.0 + 6.0;
/// The space between two groups.
pub const GROUP_GAP: f64 = 16.0;
/// Rows drawn beyond each edge of the view, so a fast scroll shows no gap.
pub const OVERSCAN: usize = 8;

/// One line of a virtual rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// The heading of group `n`.
    Header(usize),
    /// The row of check `index`.
    Row(u32),
}

/// A rail laid out at fixed heights: each line's offset from the top of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct RailLayout {
    pub lines: Vec<(Line, f64)>,
    pub height: f64,
}

impl RailLayout {
    /// `groups` holds each group's check indices in rail order.
    pub fn new(groups: &[Vec<u32>]) -> Self {
        let mut lines = Vec::new();
        let mut y = 0.0;
        for (n, rows) in groups.iter().enumerate() {
            if n > 0 {
                y += GROUP_GAP;
            }
            lines.push((Line::Header(n), y));
            y += HEADER_HEIGHT;
            for &index in rows {
                lines.push((Line::Row(index), y));
                y += ROW_HEIGHT;
            }
        }
        Self { lines, height: y }
    }

    /// The offset of check `index`'s row.
    pub fn offset_of(&self, index: u32) -> Option<f64> {
        self.lines
            .iter()
            .find(|(line, _)| *line == Line::Row(index))
            .map(|(_, y)| *y)
    }

    /// The lines in view from `top` for `height` pixels, with [`OVERSCAN`] on each side.
    pub fn window(&self, top: f64, height: f64) -> Range<usize> {
        let first = self.lines.partition_point(|(_, y)| *y + ROW_HEIGHT <= top);
        let end = self.lines.partition_point(|(_, y)| *y < top + height);
        first.saturating_sub(OVERSCAN)..(end + OVERSCAN).min(self.lines.len())
    }

    /// The lines to mount: those in view and the row of each check in `keep` (the tab stop and
    /// the row focus returns to), in list order.
    pub fn mounted(&self, view: Range<usize>, keep: &[u32]) -> Vec<(Line, f64)> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(at, (line, _))| {
                view.contains(at) || matches!(line, Line::Row(i) if keep.contains(i))
            })
            .map(|(_, line)| *line)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_api::check::NextCheck;

    /// tC4's `checkKeyboard.test.tsx`, "railKeyDown — the Switcher roving pattern".
    #[test]
    fn rail_keys_are_tc4s_roving_pattern() {
        let order = [3, 5, 7];
        let run = |key, current| rail_key(&order, Some(current), key);
        // The active item hidden by a filter: Down and Up select the focused first row.
        assert_eq!(run("ArrowDown", 9), Some(3));
        assert_eq!(run("ArrowUp", 9), Some(3));
        // The active item shown: Down and Up step and wrap; Home and End jump.
        assert_eq!(run("ArrowDown", 5), Some(7));
        assert_eq!(run("ArrowDown", 7), Some(3));
        assert_eq!(run("ArrowUp", 3), Some(7));
        assert_eq!(run("Home", 9), Some(3));
        assert_eq!(run("End", 9), Some(7));
        assert_eq!(run("Tab", 5), None, "Tab leaves the list");
        assert_eq!(rail_key(&[], Some(1), "ArrowDown"), None);
        assert_eq!(tab_stop(&order, Some(9)), Some(3));
        assert_eq!(tab_stop(&order, Some(5)), Some(5));
    }

    /// tC4 #518 and #513: a resume into a tool shows the tool at once; the picker shows only
    /// when nothing is remembered.
    #[test]
    fn resume_shows_no_clickable_picker() {
        assert_eq!(
            landing(PlaceMode::Check, Some(Tool::TranslationNotes)),
            CheckView::Tool(Tool::TranslationNotes)
        );
        assert_eq!(landing(PlaceMode::Check, None), CheckView::Picker);
        assert_eq!(
            landing(PlaceMode::Read, Some(Tool::TranslationNotes)),
            CheckView::Picker
        );
    }

    #[test]
    fn the_cta_follows_tc4s_cta_for() {
        assert_eq!(cta_key(None), "check.open");
        assert_eq!(cta_key(Some((0, 0))), "check.open");
        assert_eq!(cta_key(Some((0, 157))), "check.cta.start");
        assert_eq!(cta_key(Some((1, 112))), "check.cta.continue");
        assert_eq!(cta_key(Some((112, 112))), "check.cta.review");
    }

    #[test]
    fn the_next_line_follows_tc4s_next_line_for() {
        let counted = |decided, total, next: Option<NextCheck>| {
            CardCount::Counted(CheckProgress {
                decided,
                total,
                dropped: None,
                next,
            })
        };
        let knowledge = NextCheck {
            title: "knowledge".into(),
            place: "TIT 1:1".parse().unwrap(),
        };
        assert_eq!(
            next_check(Some(&counted(1, 112, Some(knowledge.clone())))),
            NextLine::Check {
                title: "knowledge".into(),
                chapter: "1".into(),
                verse: "1".into()
            }
        );
        assert_eq!(
            next_check(Some(&counted(3, 3, None))),
            NextLine::AllResolved
        );
        assert_eq!(next_check(Some(&counted(0, 0, None))), NextLine::None);
        assert_eq!(next_check(None), NextLine::None);
        assert_eq!(
            count_line(Some(&counted(0, 0, None)), |p| (p.decided, p.total)),
            CountLine::NoItems
        );
        assert_eq!(
            count_line::<CheckProgress>(Some(&CardCount::Failed { reason: "x".into() }), |p| (
                p.decided, p.total
            )),
            CountLine::Failed
        );
    }

    /// tC4 #764: once one check is resolved its bar shows a fill a person can see.
    #[test]
    fn check_progress_bar_is_visible_after_one_resolved_item() {
        assert_eq!(min_fill(0), None);
        assert_eq!(min_fill(1), Some(MIN_FILL_PX));
        // tC4's bar is 270 px wide on the picker: 1 of 112 is 2.4 px of it.
        assert!(bar_value(1, 112) / 100.0 * 270.0 < MIN_FILL_PX);
    }

    /// A-24: a long rail lays out at fixed heights, draws the rows in view, and keeps the
    /// tab-stop row mounted far out of view.
    #[test]
    fn a_long_rail_mounts_its_view_and_its_tab_stop() {
        let groups = vec![(0..8_212).collect::<Vec<u32>>()];
        let layout = RailLayout::new(&groups);
        assert_eq!(layout.lines.len(), 8_213);
        assert_eq!(layout.offset_of(0), Some(HEADER_HEIGHT));
        assert_eq!(
            layout.offset_of(10),
            Some(HEADER_HEIGHT + 10.0 * ROW_HEIGHT)
        );
        let view = layout.window(0.0, 600.0);
        assert!(view.len() < 40, "{view:?}");
        let mounted = layout.mounted(view, &[8_000]);
        assert!(mounted.iter().any(|(l, _)| *l == Line::Row(8_000)));
        assert!(mounted.len() < 42);
        // Scrolled to the far end, the first rows are not drawn.
        let end = layout.window(layout.height - 600.0, 600.0);
        assert!(end.start > 8_000);
        // Two groups: the second heading sits after the gap.
        let two = RailLayout::new(&[vec![1, 2], vec![3]]);
        assert_eq!(
            two.lines.get(3),
            Some(&(
                Line::Header(1),
                HEADER_HEIGHT + 2.0 * ROW_HEIGHT + GROUP_GAP
            ))
        );
    }
}
