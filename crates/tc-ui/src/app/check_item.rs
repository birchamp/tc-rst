//! The check detail's decisions (unit B5b; tC4 `Check.jsx`'s `TargetWords`, `CheckDetail` and
//! `CheckMarks`), pure and tested on the host.
//!
//! - **The target words** are a roving line of words (#42, #737): Left and Right move between
//!   them in the order they are drawn (reversed in a right-to-left translation), Shift with an
//!   arrow selects as a word processor does, Space or Enter toggles the word alone, and a press
//!   and a drag selects whole words as the pointer passes. [`WordNav`] holds the caret, the
//!   Shift run and the drag, and answers each event with the selection it makes.
//! - **The selection** is one local value until a mark saves it (#757). It follows the stored
//!   selection when the item or its words change, and is left alone by any other re-read.
//! - **The comment** is a save-machine field of its own kind: it saves on Done and on leaving
//!   the item while its editor is open (A-10); its place names the check, so the writer can
//!   rebuild the request from the buffer entry alone.
//! - **Item navigation** (#726) steps through the list's own order, as tC4's `activeIndex` does.

use std::collections::BTreeSet;

use tc_api::check::{CheckItemView, CheckListRef, CheckPlace, CheckUnit, DecideEdit, Tool};
use tc_api::model::selection::{drag_range, toggle};
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};

use crate::app::client::CallError;

// ---- Item navigation ----------------------------------------------------------------------

/// The checks Previous and Next lead to, and whether each can be pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemNav {
    pub prev: Option<u32>,
    pub next: Option<u32>,
}

/// tC4's `canPrev` and `canNext`: the neighbours of check `active` in a list of `total`.
pub fn item_nav(active: u32, total: u32) -> ItemNav {
    ItemNav {
        prev: active.checked_sub(1),
        next: (active + 1 < total).then_some(active + 1),
    }
}

// ---- The selection ------------------------------------------------------------------------

/// The selection after a read of the active check. tC4 resets it only when the check or its
/// verse text changes, so a read that follows the person's own mark leaves what they tapped.
pub fn selection_after_read(
    shown: Option<&CheckItemView>,
    read: &CheckItemView,
    local: &BTreeSet<u32>,
) -> BTreeSet<u32> {
    match shown {
        Some(shown) if shown.index == read.index && shown.target.words == read.target.words => {
            local.clone()
        }
        _ => read.target.selected.iter().copied().collect(),
    }
}

/// Mark valid: the tapped words, or "nothing to select" when there are none (tC4's `markValid`).
pub fn valid_edit(selection: &BTreeSet<u32>) -> DecideEdit {
    DecideEdit::Valid {
        words: selection.iter().copied().collect(),
    }
}

// ---- The target words ---------------------------------------------------------------------

/// A key on the line of words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKey {
    Left {
        shift: bool,
    },
    Right {
        shift: bool,
    },
    Home,
    End,
    /// Space or Enter.
    Toggle,
    /// Shift, Control, Alt or Meta alone: it does not end a Shift run.
    Modifier,
    /// Any other key, Tab included: it ends the run.
    Other,
}

impl WordKey {
    /// The key as the browser names it.
    pub fn of(key: &str, shift: bool) -> Self {
        match key {
            "ArrowLeft" => Self::Left { shift },
            "ArrowRight" => Self::Right { shift },
            "Home" => Self::Home,
            "End" => Self::End,
            " " | "Enter" => Self::Toggle,
            "Shift" | "Control" | "Alt" | "Meta" => Self::Modifier,
            _ => Self::Other,
        }
    }
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WordStep {
    /// The new selection, when the key changed it.
    pub selection: Option<BTreeSet<u32>>,
    /// The word to focus.
    pub focus: Option<usize>,
    /// The key was the line's: the page's own action for it is off.
    pub handled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    anchor: usize,
    /// The selection when the run began; the run's range joins it.
    base: BTreeSet<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Drag {
    from: u32,
    base: BTreeSet<u32>,
    moved: bool,
}

/// The line of words as one Tab stop (a roving tab index): the caret sits before one word, the
/// Shift run grows or shrinks a range from where it began, and a drag adds whole words. Keyed
/// by item by the caller: the caret starts before the first word.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordNav {
    /// 0 to the number of words: the gap before word `caret`.
    caret: usize,
    run: Option<Run>,
    drag: Option<Drag>,
    /// A drag ended and the click that follows it must not toggle the last word back.
    dragged: bool,
}

impl WordNav {
    /// The word that is the line's Tab stop.
    pub fn tab_stop(&self, words: usize) -> usize {
        self.caret.min(words.saturating_sub(1))
    }

    fn step_of(key: WordKey, rtl: bool) -> Option<(isize, bool)> {
        let toward = |forward: bool, shift| {
            let step = if forward != rtl { 1 } else { -1 };
            Some((step, shift))
        };
        match key {
            WordKey::Right { shift } => toward(true, shift),
            WordKey::Left { shift } => toward(false, shift),
            _ => None,
        }
    }

    /// A key in the line of `words` words.
    pub fn key(
        &mut self,
        key: WordKey,
        words: usize,
        rtl: bool,
        selection: &BTreeSet<u32>,
    ) -> WordStep {
        if words == 0 {
            return WordStep::default();
        }
        let last = words - 1;
        let at = self.tab_stop(words);
        if let Some((step, shift)) = Self::step_of(key, rtl) {
            return if shift {
                self.extend(step, words, selection)
            } else {
                let to = at.saturating_add_signed(step).min(last);
                self.move_to(to)
            };
        }
        match key {
            WordKey::Home => self.move_to(0),
            WordKey::End => self.move_to(last),
            WordKey::Toggle => {
                self.run = None;
                self.caret = at;
                WordStep {
                    selection: Some(toggle(selection, at as u32)),
                    focus: None,
                    handled: true,
                }
            }
            // A bare modifier does not end a run; any other key does, and the selection stays as
            // the next run's base (#42, decision point 2).
            WordKey::Modifier => WordStep::default(),
            _ => {
                self.run = None;
                WordStep::default()
            }
        }
    }

    fn move_to(&mut self, to: usize) -> WordStep {
        self.run = None;
        self.caret = to;
        WordStep {
            selection: None,
            focus: Some(to),
            handled: true,
        }
    }

    fn extend(&mut self, step: isize, words: usize, selection: &BTreeSet<u32>) -> WordStep {
        let run = self.run.get_or_insert_with(|| Run {
            anchor: self.caret,
            base: selection.clone(),
        });
        let next = self.caret.saturating_add_signed(step).min(words);
        let (from, to) = (run.anchor.min(next), run.anchor.max(next));
        let mut range = run.base.clone();
        range.extend((from..to).map(|i| i as u32));
        self.caret = next;
        WordStep {
            selection: Some(range),
            focus: Some(next.min(words - 1)),
            handled: true,
        }
    }

    /// Focus left the line: a click elsewhere, then back, starts a new run.
    pub fn left_the_line(&mut self) {
        self.run = None;
    }

    /// The pointer went down on word `at` (tC4's mousedown): the keyboard then acts on this
    /// word, as after a click, and a drag may begin.
    pub fn press(&mut self, at: usize, selection: &BTreeSet<u32>) {
        self.run = None;
        self.caret = at;
        self.drag = Some(Drag {
            from: at as u32,
            base: selection.clone(),
            moved: false,
        });
    }

    /// The pointer entered word `at` with the button down: the words from where it went down to
    /// here, whole, join the words selected before.
    pub fn enter(&mut self, at: usize) -> Option<BTreeSet<u32>> {
        let drag = self.drag.as_mut()?;
        if !drag.moved && at as u32 == drag.from {
            return None;
        }
        drag.moved = true;
        Some(drag_range(&drag.base, drag.from, at as u32))
    }

    /// The button went up anywhere. True when a drag selected words.
    pub fn release(&mut self) -> bool {
        let moved = self.drag.as_ref().is_some_and(|d| d.moved);
        if moved {
            self.run = None;
            self.dragged = true;
        }
        self.drag = None;
        moved
    }

    /// The click on word `at`: it toggles the word, unless it is the click a drag ends with.
    pub fn click(&mut self, at: usize, selection: &BTreeSet<u32>) -> Option<BTreeSet<u32>> {
        if self.dragged {
            return None;
        }
        self.run = None;
        self.caret = at;
        Some(toggle(selection, at as u32))
    }

    /// The task after a drag's release: the next click is a click again.
    pub fn clear_dragged(&mut self) {
        self.dragged = false;
    }
}

// ---- A mark that was not saved ------------------------------------------------------------

/// Why a mark was not saved. The mark is one act and its buttons stay on screen, so pressing
/// it again is the retry; nothing the person chose is lost, since the selection is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkFailure {
    /// The check or its verse changed since the detail showed them: look again.
    LookAgain,
    /// The project is read-only now.
    ReadOnly(ReadOnlyReason),
    /// Any other refusal or failure, in the words the server or the transport gave.
    Other(String),
}

impl MarkFailure {
    pub fn of(error: &CallError) -> Self {
        match error {
            CallError::Api(ApiError::NotSaved(NotSaved::LookAgain)) => Self::LookAgain,
            CallError::Api(ApiError::NotSaved(NotSaved::ReadOnly(reason))) => {
                Self::ReadOnly(*reason)
            }
            other => Self::Other(other.to_string()),
        }
    }
}

// ---- The check comment's field ------------------------------------------------------------

/// A check comment's place in the draft buffer: the tool, the check's place and its row's id.
/// The save machine hands the writer only this and the entry, so the writer reads the request
/// back from it.
pub fn comment_place(tool: Tool, place: &CheckPlace, check_id: &str) -> String {
    let tool = match tool {
        Tool::TranslationNotes => "tn",
        Tool::TranslationWords => "tw",
    };
    format!("{tool}|{place}|{check_id}")
}

/// The list, the place and the row id a comment's place names.
pub fn parse_comment_place(text: &str) -> Option<(CheckListRef, CheckPlace, String)> {
    let mut parts = text.splitn(3, '|');
    let tool = match parts.next()? {
        "tn" => Tool::TranslationNotes,
        "tw" => Tool::TranslationWords,
        _ => return None,
    };
    let place: CheckPlace = parts.next()?.parse().ok()?;
    let check_id = parts.next().filter(|id| !id.is_empty())?.to_owned();
    let unit = match &place {
        CheckPlace::Verse { book, .. } => CheckUnit::Book(*book),
        CheckPlace::Frame { .. } => CheckUnit::Stories,
    };
    Some((CheckListRef { tool, unit }, place, check_id))
}

/// Done (tC4's `done`): the text to save, trimmed, when it differs from the stored comment; an
/// empty one removes the comment. `None` when there is nothing to save.
pub fn done_text(draft: &str, stored: Option<&str>) -> Option<String> {
    let text = draft.trim();
    (text != stored.unwrap_or_default()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_api::check::{
        Category, CheckArticle, DecisionView, GatewayView, NoteCategory, OriginalView, TargetWords,
        Testament,
    };
    use tc_api::helps::ArticleLink;
    use tc_api::shown::Shown;
    use tc_api::token::ShownToken;

    fn set(items: &[u32]) -> BTreeSet<u32> {
        items.iter().copied().collect()
    }

    /// The line of four words `a b c d`, driven as tC4's `checkKeyboard.test.tsx` drives it.
    struct Line {
        nav: WordNav,
        selection: BTreeSet<u32>,
        focus: usize,
    }

    impl Line {
        fn new() -> Self {
            Self {
                nav: WordNav::default(),
                selection: BTreeSet::new(),
                focus: 0,
            }
        }

        fn key(&mut self, key: &str, shift: bool) {
            let step = self
                .nav
                .key(WordKey::of(key, shift), 4, false, &self.selection);
            if let Some(selection) = step.selection {
                self.selection = selection;
            }
            if let Some(focus) = step.focus {
                self.focus = focus;
            }
        }

        fn drag(&mut self, from: usize, to: usize) {
            self.nav.press(from, &self.selection);
            self.focus = from;
            for at in from + 1..=to {
                if let Some(selection) = self.nav.enter(at) {
                    self.selection = selection;
                }
            }
            self.nav.release();
        }
    }

    #[test]
    fn tab_away_and_back_shift_left_starts_a_new_run_and_keeps_both_words() {
        let mut line = Line::new();
        line.key("ArrowRight", true);
        line.key("ArrowRight", true);
        assert_eq!(line.selection, set(&[0, 1]));
        line.key("Tab", false);
        line.key("Shift", true);
        line.key("ArrowLeft", true);
        assert_eq!(line.selection, set(&[0, 1]));
    }

    #[test]
    fn a_blur_that_leaves_the_line_ends_the_run_too() {
        let mut line = Line::new();
        line.key("ArrowRight", true);
        line.key("ArrowRight", true);
        line.nav.left_the_line();
        line.key("ArrowLeft", true);
        assert_eq!(line.selection, set(&[0, 1]));
    }

    #[test]
    fn one_unbroken_run_still_shrinks() {
        let mut line = Line::new();
        line.key("ArrowRight", true);
        line.key("ArrowRight", true);
        line.key("ArrowLeft", true);
        assert_eq!(line.selection, set(&[0]));
    }

    /// tC4 #737: a drag selects the words it passed over, whole.
    #[test]
    fn a_drag_selects_the_words_it_passed_over_whole() {
        let mut line = Line::new();
        line.drag(1, 3);
        assert_eq!(line.selection, set(&[1, 2, 3]));
    }

    #[test]
    fn space_right_after_a_drag_toggles_the_word_the_drag_started_on() {
        let mut line = Line::new();
        line.drag(1, 3);
        line.key(" ", false);
        assert_eq!(line.selection, set(&[2, 3]));
    }

    #[test]
    fn shift_right_right_after_a_drag_extends_from_the_word_the_drag_started_on() {
        let mut line = Line::new();
        line.drag(1, 2);
        line.key("Escape", false);
        line.key("ArrowRight", true);
        assert_eq!(line.selection, set(&[1, 2]));
        assert_eq!(line.focus, 2);
    }

    /// The click that ends a drag does not toggle the last word back; the next one does.
    #[test]
    fn the_click_a_drag_ends_with_toggles_nothing() {
        let mut nav = WordNav::default();
        let selection = set(&[1, 2, 3]);
        nav.press(1, &BTreeSet::new());
        assert_eq!(nav.enter(2), Some(set(&[1, 2])));
        assert!(nav.release());
        assert_eq!(nav.click(3, &selection), None);
        nav.clear_dragged();
        assert_eq!(nav.click(3, &selection), Some(set(&[1, 2])));
    }

    /// A press and a release on one word is a click, not a drag.
    #[test]
    fn a_press_and_release_on_one_word_is_a_click() {
        let mut nav = WordNav::default();
        nav.press(2, &BTreeSet::new());
        assert_eq!(nav.enter(2), None);
        assert!(!nav.release());
        assert_eq!(nav.click(2, &BTreeSet::new()), Some(set(&[2])));
    }

    /// In a right-to-left translation Left moves to the next word.
    #[test]
    fn the_arrows_follow_the_direction_of_the_translation() {
        let mut nav = WordNav::default();
        let none = BTreeSet::new();
        let step = nav.key(WordKey::Left { shift: false }, 4, true, &none);
        assert_eq!(step.focus, Some(1));
        let step = nav.key(WordKey::Right { shift: false }, 4, true, &none);
        assert_eq!(step.focus, Some(0));
        // At the first word Right (Left in left-to-right) stays.
        let step = nav.key(WordKey::Right { shift: false }, 4, true, &none);
        assert_eq!(step.focus, Some(0));
        assert_eq!(nav.tab_stop(4), 0);
        let step = nav.key(WordKey::End, 4, false, &none);
        assert_eq!(step.focus, Some(3));
        assert_eq!(nav.tab_stop(4), 3);
        // A line of no word has no key.
        assert_eq!(nav.key(WordKey::End, 0, false, &none), WordStep::default());
    }

    /// tC4's `canPrev` and `canNext`.
    #[test]
    fn previous_and_next_stop_at_the_ends_of_the_list() {
        assert_eq!(
            item_nav(0, 157),
            ItemNav {
                prev: None,
                next: Some(1)
            }
        );
        assert_eq!(
            item_nav(1, 157),
            ItemNav {
                prev: Some(0),
                next: Some(2)
            }
        );
        assert_eq!(
            item_nav(156, 157),
            ItemNav {
                prev: Some(155),
                next: None
            }
        );
        assert_eq!(
            item_nav(0, 1),
            ItemNav {
                prev: None,
                next: None
            }
        );
    }

    fn view(index: u32, words: &[&str], selected: &[u32]) -> CheckItemView {
        let token: ShownToken =
            serde_json::from_value(serde_json::json!("st1.000000000000000000000001")).unwrap();
        CheckItemView {
            index,
            view: tc_api::ids::ViewSeq::new(1),
            check_id: "abcd".into(),
            place: "TIT 1:1".parse().unwrap(),
            title: "faith".into(),
            note: Vec::new(),
            helps_dir: None,
            article: CheckArticle {
                link: ArticleLink::Academy {
                    slug: "figs-abstractnouns".into(),
                },
                title: None,
                category: Category::Note(NoteCategory::Grammar),
            },
            original: OriginalView::Unavailable {
                testament: Testament::Nt,
            },
            gateway: GatewayView::NoPane,
            target: TargetWords {
                words: words.iter().map(|w| (*w).to_owned()).collect(),
                selected: selected.to_vec(),
                verse: None,
            },
            decision: Shown {
                value: DecisionView {
                    status: None,
                    nothing_to_select: false,
                    bookmarked: false,
                    comment: None,
                    invalidated: false,
                },
                token,
                marker: None,
            },
            notices: Vec::new(),
        }
    }

    /// tC4 #532 and #737: the selection follows the stored one when the check or its draft
    /// changes, and a read that follows the person's own mark leaves what they tapped.
    #[test]
    fn check_shows_new_draft_after_switching_back() {
        let shown = view(3, &["Pablo", "siervo"], &[0]);
        let local = set(&[0, 1]);
        // The same check and the same words: the person's taps stay (a re-read after a mark).
        assert_eq!(selection_after_read(Some(&shown), &shown, &local), local);
        // The draft changed while the person was away: the new words, and the stored selection.
        let drafted = view(3, &["Pablo", "esclavo", "de"], &[1]);
        assert_eq!(
            selection_after_read(Some(&shown), &drafted, &local),
            set(&[1])
        );
        // Another check: its own stored selection.
        let other = view(4, &["Pablo", "siervo"], &[]);
        assert_eq!(selection_after_read(Some(&shown), &other, &local), set(&[]));
        // The first read.
        assert_eq!(selection_after_read(None, &shown, &local), set(&[0]));
    }

    #[test]
    fn a_refused_mark_says_why_and_keeps_the_choice() {
        let look = CallError::Api(ApiError::NotSaved(NotSaved::LookAgain));
        assert_eq!(MarkFailure::of(&look), MarkFailure::LookAgain);
        let read_only = CallError::Api(ApiError::NotSaved(NotSaved::ReadOnly(
            ReadOnlyReason::SecondWriter,
        )));
        assert_eq!(
            MarkFailure::of(&read_only),
            MarkFailure::ReadOnly(ReadOnlyReason::SecondWriter)
        );
        let disk = CallError::Api(ApiError::NotSaved(NotSaved::Disk("full".into())));
        assert!(
            matches!(MarkFailure::of(&disk), MarkFailure::Other(text) if text.contains("full"))
        );
    }

    #[test]
    fn mark_valid_with_no_word_is_nothing_to_select() {
        assert_eq!(valid_edit(&set(&[])), DecideEdit::Valid { words: vec![] });
        assert_eq!(
            valid_edit(&set(&[3, 1])),
            DecideEdit::Valid { words: vec![1, 3] }
        );
    }

    #[test]
    fn a_comments_place_names_its_check_and_reads_back() {
        for (tool, at, id) in [
            (Tool::TranslationNotes, "TIT 1:1", "abcd"),
            (Tool::TranslationWords, "PSA 60:front", "x1y2"),
            (Tool::TranslationNotes, "PSA 5:1,3,8,12", "k9"),
            (Tool::TranslationNotes, "TIT front:intro", "z"),
        ] {
            let place: CheckPlace = at.parse().unwrap();
            let text = comment_place(tool, &place, id);
            let (list, back, check_id) = parse_comment_place(&text).unwrap();
            assert_eq!(
                (list.tool, &back, check_id.as_str()),
                (tool, &place, id),
                "{text}"
            );
            assert!(matches!(list.unit, CheckUnit::Book(_)));
        }
        assert_eq!(parse_comment_place("tn|TIT 1:1|"), None);
        assert_eq!(parse_comment_place("zz|TIT 1:1|abcd"), None);
        assert_eq!(parse_comment_place("tn|nowhere|abcd"), None);
    }

    /// tC4's `done`: a comment is saved trimmed, only when it differs from the stored one, and
    /// an empty one removes it.
    #[test]
    fn done_saves_the_trimmed_text_only_when_it_changed() {
        assert_eq!(done_text("  ask Pablo \n", None), Some("ask Pablo".into()));
        assert_eq!(done_text("ask Pablo", Some("ask Pablo")), None);
        assert_eq!(done_text("  ", None), None);
        assert_eq!(done_text("", Some("ask Pablo")), Some(String::new()));
    }
}
