//! The Align screen's decisions (unit B6; tC4 `Align.jsx` and `AlignWorkspace` in `Check.jsx`),
//! pure and tested on the host: the rail's filter and counts, the marking row's neighbours,
//! the lens, what a drag carries and what a drop does, the keys that select and place, and the
//! queue that sends the person's clicks one at a time.
//!
//! The aligner itself is on the server (AD18): every click is a request that names the view it
//! was made on and returns the whole view. The queue keeps a fast second click from being
//! refused or lost while the first is on its way (A-14).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use tc_api::align::{
    AlignEdit, AlignPlace, AlignSave, AlignerView, BankClick, BankSelection, Edit, IndexItem,
    RailState, Word,
};
use tc_api::event::{ChangeCause, Event, SessionEvent};
use tc_api::query::QueryKey;
use tc_core::BookId;

use crate::app::client::CallError;

/// tC4's `mode`: the gateway lens or the original-language words (`align-mode-gw`, `-orig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lens {
    Gateway,
    Original,
}

/// The lens that shows: the gateway one only while the verse has it (tC4's `effMode`).
pub fn effective_lens(chosen: Lens, gateway_available: bool) -> Lens {
    if gateway_available && chosen == Lens::Gateway {
        Lens::Gateway
    } else {
        Lens::Original
    }
}

/// tC4's `ALIGN_FILTERS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailFilter {
    All,
    Todo,
    Invalid,
}

impl RailFilter {
    pub const ORDER: [RailFilter; 3] = [Self::All, Self::Todo, Self::Invalid];

    /// The id in `align-filter-<id>` and `check.filter.<id>`.
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Todo => "todo",
            Self::Invalid => "invalid",
        }
    }

    pub fn matches(self, item: &IndexItem) -> bool {
        self.matches_state(item.status.state)
    }

    pub fn matches_state(self, state: RailState) -> bool {
        match self {
            Self::All => true,
            // A verse not drafted yet is still to do.
            Self::Todo => matches!(state, RailState::Todo | RailState::Undrafted),
            Self::Invalid => state == RailState::Invalid,
        }
    }
}

/// The chips' counts, in [`RailFilter::ORDER`], from the state of each verse.
pub fn counts(states: impl IntoIterator<Item = RailState>) -> [usize; 3] {
    let mut out = [0; 3];
    for state in states {
        for (n, filter) in out.iter_mut().zip(RailFilter::ORDER) {
            *n += usize::from(filter.matches_state(state));
        }
    }
    out
}

/// Verses done out of verses drafted (the rail's bar and `align.progressVerses`).
pub fn progress(states: impl IntoIterator<Item = RailState>) -> (usize, usize) {
    states
        .into_iter()
        .filter(|state| *state != RailState::Undrafted)
        .fold((0, 0), |(done, total), state| {
            (done + usize::from(state == RailState::Valid), total + 1)
        })
}

/// The width of the rail's bar, as tC4's `ProgressBar` is given it.
pub fn percent(done: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        done as f64 / total as f64 * 100.0
    }
}

/// The open verse among the drafted ones: tC4's `alignNav`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nav {
    /// The open verse's place among the drafted verses, from 0.
    pub at: Option<usize>,
    pub drafted: usize,
    pub prev: Option<AlignPlace>,
    pub next: Option<AlignPlace>,
}

pub fn nav(items: &[IndexItem], place: AlignPlace) -> Nav {
    let drafted: Vec<AlignPlace> = items
        .iter()
        .filter(|i| i.status.state != RailState::Undrafted)
        .map(|i| i.place)
        .collect();
    let at = drafted.iter().position(|p| *p == place);
    Nav {
        at,
        drafted: drafted.len(),
        prev: at
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| drafted.get(i))
            .copied(),
        next: at.and_then(|i| drafted.get(i + 1)).copied(),
    }
}

/// A place from tC4's `ref` text: `1:9` or `1:9-10` (the Align card's "next verse").
pub fn parse_place(label: &str) -> Option<AlignPlace> {
    let (chapter, verses) = label.split_once(':')?;
    let (first, last) = verses.split_once('-').unwrap_or((verses, verses));
    let place = AlignPlace {
        chapter: chapter.parse().ok()?,
        verse: first.parse().ok()?,
        last: last.parse().ok()?,
    };
    (place.chapter > 0 && place.verse > 0 && place.last >= place.verse).then_some(place)
}

/// Whether two places are the same verse to open: a place the page made from a chosen verse
/// does not know how far its span reaches.
pub fn same_verse(a: AlignPlace, b: AlignPlace) -> bool {
    (a.chapter, a.verse) == (b.chapter, b.verse)
}

/// A word that occurs more than once in its verse shows its number (tC4 #735).
pub fn occurrence(word: &Word) -> Option<u32> {
    (word.occurrences > 1).then_some(word.occurrence)
}

/// The chip's accessible name: "de 3".
pub fn occurrence_name(word: &Word) -> Option<String> {
    occurrence(word).map(|n| format!("{} {n}", word.word))
}

/// What a drag carries (tC4's drag payloads, a string across the browser's boundary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Drag {
    /// Bank words: the whole selection when the dragged chip is part of it, else the chip.
    Bank { words: Vec<Word> },
    /// A placed word, from its card.
    Placed { from: u32, word: Word },
    /// A card, to be merged into another.
    Card { from: u32 },
}

impl Drag {
    /// Reads a payload; a malformed one is none, a no-op and not a crash (tC4 validates the
    /// shape because it crosses a string boundary).
    pub fn parse(text: &str) -> Option<Self> {
        let drag: Self = serde_json::from_str(text).ok()?;
        match &drag {
            Self::Bank { words } if words.is_empty() => None,
            _ => Some(drag),
        }
    }

    pub fn text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// What dragging the bank chip at `index` carries (tC4's `onDragStart`).
pub fn dragged_words(view: &AlignerView, index: usize) -> Vec<Word> {
    let Some(chip) = view.bank.get(index) else {
        return Vec::new();
    };
    match &view.selection {
        BankSelection::Armed(armed) if armed.iter().any(|&i| i as usize == index) => armed
            .iter()
            .filter_map(|&i| view.bank.get(i as usize).cloned())
            .collect(),
        _ => vec![chip.clone()],
    }
}

/// A drop on a card (tC4's `onDropOnCard`).
pub fn drop_on_card(card: u32, drag: Drag) -> AlignEdit {
    match drag {
        Drag::Bank { words } => AlignEdit::PlaceWords { card, words },
        Drag::Placed { from, word } => AlignEdit::Move {
            from,
            to: card,
            word,
        },
        Drag::Card { from } => AlignEdit::Merge { from, to: card },
    }
}

/// A drop on the bank: only a placed word goes back (tC4's `onDropOnBank`).
pub fn drop_on_bank(drag: Drag) -> Option<AlignEdit> {
    match drag {
        Drag::Placed { from, word } => Some(AlignEdit::Unplace { card: from, word }),
        Drag::Bank { .. } | Drag::Card { .. } => None,
    }
}

/// What a click on a bank chip is, from the keys held (Ctrl or Cmd wins over Shift).
pub fn bank_click(ctrl_or_meta: bool, shift: bool) -> BankClick {
    if ctrl_or_meta {
        BankClick::Toggle
    } else if shift {
        BankClick::Range
    } else {
        BankClick::Plain
    }
}

/// Enter or Space on a bank chip with Ctrl, Cmd or Shift held selects from the keyboard; the
/// click a key fires carries no modifier. Without a modifier the key is the button's own click.
pub fn bank_key(key: &str, ctrl_or_meta: bool, shift: bool) -> Option<BankClick> {
    let pressed = key == "Enter" || key == " ";
    (pressed && (ctrl_or_meta || shift)).then(|| bank_click(ctrl_or_meta, shift))
}

/// Enter or Space on a focused card places the shown selection on it (tC4 #568: a person who
/// cannot drag or point can still align; tC4 4.0 has no key for it).
pub fn card_key_places(key: &str, armed: bool) -> bool {
    armed && (key == "Enter" || key == " ")
}

/// Whether the cards take a click: some shown bank word is selected.
pub fn armed(view: &AlignerView) -> bool {
    matches!(&view.selection, BankSelection::Armed(armed) if !armed.is_empty())
}

/// What the Align tool reads again after a frame of the event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Refresh {
    /// The rail's list of verses.
    pub rail: bool,
    /// The open aligner's view, which carries the save state and the suggestion engine's.
    pub aligner: bool,
}

/// The reads a frame makes stale while the tool is open on `book`. A gap reads both: the
/// aligner's save state and the engine's state come only from events and from the view, so the
/// view is what a `Resync` restores (APP-DESIGN 5.4). A change of the engine reads the view. A
/// teammate's change to the book reads the rail; the tool's own saves and the app's own
/// changes do not, or every save would read the whole book's rail.
pub fn refresh_for(event: &Event, book: BookId) -> Refresh {
    match event {
        Event::Resync => Refresh {
            rail: true,
            aligner: true,
        },
        Event::Session {
            event: SessionEvent::Suggest { .. },
            ..
        } => Refresh {
            rail: false,
            aligner: true,
        },
        Event::Session {
            event: SessionEvent::Changed { scope, cause, .. },
            ..
        } if !matches!(cause, ChangeCause::LocalSave { .. } | ChangeCause::System)
            && QueryKey::Book(book).depends_on(scope) =>
        {
            Refresh {
                rail: true,
                aligner: false,
            }
        }
        _ => Refresh::default(),
    }
}

/// The exact request that may already have reached the actor. Retry must keep all of it,
/// including its server run: a restarted server no longer has the original reply cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentEdit {
    pub request: Edit,
    pub run: tc_api::ids::ServerInstance,
}

/// How long Home waits for a click to save before it asks. A click's round trip, painted, is
/// p50 31 ms and p95 42 ms in a debug build (`align_click_round_trip_p95`, 2026-10-26; the
/// release budget is 30 ms), and a retry after a failed send is two round trips (`Hello`, then
/// the edit), sent every 250 ms. So 3 s is about 70 slow clicks: a click still unsaved then is
/// stuck, not slow, and the person is not kept waiting long.
pub const LEAVE_WAIT_MS: f64 = 3_000.0;

/// What Home does with unsaved clicks (see [`EditQueue::on_leave`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveWait {
    /// Every click is saved: go on to save the links.
    Leave,
    /// A click is on its way: wait for its answer.
    Wait,
    /// A click's send failed on the way: send it again and wait.
    Retry,
    /// A click is lost for sure: ask Stay or Leave without it.
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditFailure {
    Retryable(String),
    /// The actor or open changed. Keep the actions visible as unsaved; never replay them on
    /// a different actor merely because the same verse is on screen.
    Refused(String),
}

impl EditFailure {
    pub fn message(&self) -> &str {
        match self {
            Self::Retryable(message) | Self::Refused(message) => message,
        }
    }
}

#[derive(Debug, Default)]
enum Flight {
    #[default]
    Idle,
    Preparing(AlignEdit),
    Sending(SentEdit),
    Failed {
        sent: SentEdit,
        reason: EditFailure,
    },
}

/// The person's edits in order. An unanswered or refused edit stays ahead of every later
/// click until its exact request succeeds; a transport failure is never an acknowledgement.
#[derive(Debug, Default)]
pub struct EditQueue {
    waiting: VecDeque<AlignEdit>,
    flight: Flight,
    generation: u64,
}

impl EditQueue {
    pub fn is_idle(&self) -> bool {
        matches!(self.flight, Flight::Idle) && self.waiting.is_empty()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn failure(&self) -> Option<&EditFailure> {
        match &self.flight {
            Flight::Failed { reason, .. } => Some(reason),
            _ => None,
        }
    }

    pub fn push(&mut self, edit: AlignEdit) -> Option<AlignEdit> {
        self.generation = self.generation.wrapping_add(1);
        self.waiting.push_back(edit);
        self.next()
    }

    /// The web boundary binds a prepared edit to one immutable request before it sends it.
    pub fn sent(&mut self, sent: SentEdit) {
        if matches!(&self.flight, Flight::Preparing(edit) if edit == &sent.request.edit) {
            self.flight = Flight::Sending(sent);
        }
    }

    pub fn failed(&mut self, reason: EditFailure) {
        self.flight = match std::mem::take(&mut self.flight) {
            Flight::Sending(sent) | Flight::Failed { sent, .. } => Flight::Failed { sent, reason },
            other => other,
        };
    }

    /// Only an ambiguous transport failure can resend. A typed refusal retains the actions
    /// for the person and does not silently reinterpret card positions against a newer actor.
    pub fn retry(&mut self) -> Option<SentEdit> {
        match std::mem::take(&mut self.flight) {
            Flight::Failed {
                sent,
                reason: EditFailure::Retryable(_),
            } => {
                self.flight = Flight::Sending(sent.clone());
                Some(sent)
            }
            other => {
                self.flight = other;
                None
            }
        }
    }

    pub fn replied(&mut self) -> Option<AlignEdit> {
        if !matches!(self.flight, Flight::Sending(_) | Flight::Preparing(_)) {
            return None;
        }
        self.flight = Flight::Idle;
        self.next()
    }

    /// What Home does with the clicks not yet saved, `waited_ms` after it was pressed (owner
    /// ruling ALIGN-LEAVE): the person is asked only once a click is lost for sure.
    pub fn on_leave(&self, waited_ms: f64) -> LeaveWait {
        match &self.flight {
            _ if self.is_idle() => LeaveWait::Leave,
            Flight::Failed {
                reason: EditFailure::Refused(_),
                ..
            } => LeaveWait::Ask,
            _ if waited_ms >= LEAVE_WAIT_MS => LeaveWait::Ask,
            Flight::Failed {
                reason: EditFailure::Retryable(_),
                ..
            } => LeaveWait::Retry,
            _ => LeaveWait::Wait,
        }
    }

    /// Used only after the surrounding project/view was explicitly reset. Error replies
    /// retain this queue instead of entering this path.
    pub fn drop_waiting(&mut self) -> usize {
        self.generation = self.generation.wrapping_add(1);
        self.flight = Flight::Idle;
        let waiting = self.waiting.len();
        self.waiting.clear();
        waiting
    }

    fn next(&mut self) -> Option<AlignEdit> {
        if !matches!(self.flight, Flight::Idle) {
            return None;
        }
        let edit = self.waiting.pop_front()?;
        self.flight = Flight::Preparing(edit.clone());
        Some(edit)
    }

    #[cfg(test)]
    fn pending(&self) -> Vec<AlignEdit> {
        let first = match &self.flight {
            Flight::Idle => None,
            Flight::Preparing(edit) => Some(edit.clone()),
            Flight::Sending(sent) | Flight::Failed { sent, .. } => Some(sent.request.edit.clone()),
        };
        first
            .into_iter()
            .chain(self.waiting.iter().cloned())
            .collect()
    }
}

/// What the answer to an aligner's `Close` means for its save state (B6, 5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closed {
    /// Every link is saved and the server let the aligner go.
    Saved,
    /// The server keeps the aligner open: its links are not saved yet, or their save failed.
    Kept(AlignSave),
    /// No answer: the aligner may still be open with links not saved. Its state stays as its
    /// latest view or event said, and its events keep it true.
    Unanswered,
    /// The server answered that the session or the aligner is gone: leaving the project saved
    /// the links, or reported that it could not.
    Gone,
}

pub fn closed(answer: Result<AlignSave, CallError>) -> Closed {
    match answer {
        Ok(AlignSave::Saved) => Closed::Saved,
        Ok(kept) => Closed::Kept(kept),
        Err(CallError::Transport(_)) => Closed::Unanswered,
        Err(CallError::Api(_) | CallError::Dropped) => Closed::Gone,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]
mod tests {
    use tc_api::align::{RailState, RailStatus};

    use crate::app::client::Transport;

    use super::*;

    #[test]
    fn a_close_without_an_answer_keeps_the_aligners_save_state() {
        let unreachable = Err(CallError::Transport(Transport::Unreachable(
            "offline".into(),
        )));
        assert_eq!(closed(unreachable), Closed::Unanswered);
        assert_eq!(
            closed(Err(CallError::Transport(Transport::Status(503)))),
            Closed::Unanswered
        );
    }

    #[test]
    fn a_close_the_server_answers_says_what_happened_to_the_links() {
        assert_eq!(closed(Ok(AlignSave::Saved)), Closed::Saved);
        assert_eq!(
            closed(Ok(AlignSave::Unsaved)),
            Closed::Kept(AlignSave::Unsaved)
        );
        let stale = Err(CallError::Api(tc_api::ApiError::StaleSession));
        assert_eq!(closed(stale), Closed::Gone);
        assert_eq!(closed(Err(CallError::Dropped)), Closed::Gone);
    }

    fn place(chapter: u16, verse: u16) -> AlignPlace {
        AlignPlace {
            chapter,
            verse,
            last: verse,
        }
    }

    fn item(verse: u16, state: RailState) -> IndexItem {
        IndexItem {
            place: place(1, verse),
            text: String::new(),
            status: RailStatus {
                state,
                placed: 0,
                total: 0,
            },
        }
    }

    fn word(text: &str, occurrence: u32, occurrences: u32) -> Word {
        Word {
            word: text.into(),
            occurrence,
            occurrences,
        }
    }

    #[test]
    fn the_rail_counts_and_filters_as_tc4s() {
        let items = [
            item(1, RailState::Valid),
            item(2, RailState::Todo),
            item(3, RailState::Invalid),
            item(4, RailState::Undrafted),
        ];
        let states = || items.iter().map(|i| i.status.state);
        assert_eq!(counts(states()), [4, 2, 1]);
        assert_eq!(progress(states()), (1, 3));
        assert!(RailFilter::Todo.matches(&items[3]), "undrafted is to do");
        assert!(!RailFilter::Todo.matches(&items[0]));
        assert_eq!(percent(1, 4), 25.0);
        assert_eq!(percent(0, 0), 0.0);
    }

    #[test]
    fn the_marking_row_moves_among_the_drafted_verses_only() {
        let items = [
            item(1, RailState::Todo),
            item(2, RailState::Undrafted),
            item(3, RailState::Valid),
            item(4, RailState::Todo),
        ];
        let first = nav(&items, place(1, 1));
        assert_eq!((first.at, first.drafted), (Some(0), 3));
        assert_eq!((first.prev, first.next), (None, Some(place(1, 3))));
        let last = nav(&items, place(1, 4));
        assert_eq!((last.prev, last.next), (Some(place(1, 3)), None));
        let lost = nav(&items, place(1, 2));
        assert_eq!((lost.at, lost.prev, lost.next), (None, None, None));
    }

    #[test]
    fn a_resync_reads_the_aligner_again_and_a_save_of_the_tool_reads_nothing() {
        use tc_api::align::{SuggestStatus, Testament};
        use tc_api::event::{ChangeParts, ChangeScope, ChapterRef};
        use tc_api::ids::{Epoch, ProjectRef, Scope, ViewSeq};
        let at = Scope {
            project: ProjectRef::parse("kr_ulb").unwrap(),
            epoch: Epoch::new(1),
        };
        let session = |event| Event::Session {
            at: at.clone(),
            event,
        };
        let changed = |cause| {
            session(SessionEvent::Changed {
                view: ViewSeq::new(3),
                scope: ChangeScope::Parts(ChangeParts {
                    chapters: vec![ChapterRef {
                        book: BookId::Titus,
                        chapter: 1,
                    }],
                    ..ChangeParts::default()
                }),
                cause,
            })
        };
        let both = Refresh {
            rail: true,
            aligner: true,
        };
        assert_eq!(refresh_for(&Event::Resync, BookId::Titus), both);
        let suggest = session(SessionEvent::Suggest {
            testament: Testament::Nt,
            status: SuggestStatus::Reading,
        });
        assert_eq!(
            refresh_for(&suggest, BookId::Titus),
            Refresh {
                rail: false,
                aligner: true
            }
        );
        // The tool's own save is a change of the book with the cause System: the rail has it.
        assert_eq!(
            refresh_for(&changed(ChangeCause::System), BookId::Titus),
            Refresh::default()
        );
        let teammate = changed(ChangeCause::Intake { from: vec![] });
        assert_eq!(
            refresh_for(&teammate, BookId::Titus),
            Refresh {
                rail: true,
                aligner: false
            }
        );
        assert_eq!(
            refresh_for(&teammate, BookId::Jonah),
            Refresh::default(),
            "another book's change is not this rail's"
        );
        assert_eq!(
            refresh_for(&Event::Internet { on: true }, BookId::Titus),
            Refresh::default()
        );
    }

    #[test]
    fn the_next_verse_of_the_card_names_a_place() {
        assert_eq!(parse_place("1:1"), Some(place(1, 1)));
        assert_eq!(
            parse_place("2:9-10"),
            Some(AlignPlace {
                chapter: 2,
                verse: 9,
                last: 10
            })
        );
        for bad in ["", "1", "a:1", "1:0", "0:1", "1:5-3", "1:2-"] {
            assert_eq!(parse_place(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_repeated_word_shows_its_number_and_is_named_with_it() {
        assert_eq!(occurrence(&word("de", 3, 5)), Some(3));
        assert_eq!(occurrence_name(&word("de", 3, 5)).as_deref(), Some("de 3"));
        assert_eq!(occurrence(&word("Dios", 1, 1)), None);
        assert_eq!(occurrence_name(&word("Dios", 1, 1)), None);
    }

    #[test]
    fn the_gateway_lens_shows_only_while_the_verse_has_it() {
        assert_eq!(effective_lens(Lens::Gateway, true), Lens::Gateway);
        assert_eq!(effective_lens(Lens::Gateway, false), Lens::Original);
        assert_eq!(effective_lens(Lens::Original, true), Lens::Original);
    }

    #[test]
    fn a_malformed_drop_is_no_edit() {
        for text in [
            "",
            "not json",
            "{}",
            r#"{"kind":"bank","words":[]}"#,
            r#"{"kind":"bank","words":[{"word":1}]}"#,
            r#"{"kind":"placed","from":"a"}"#,
            r#"{"kind":"card"}"#,
            r#"{"kind":"other","from":1}"#,
        ] {
            assert_eq!(Drag::parse(text), None, "{text}");
        }
    }

    #[test]
    fn drops_do_what_tc4s_do() {
        let paul = word("Pablo", 1, 1);
        let bank = Drag::Bank {
            words: vec![paul.clone()],
        };
        assert_eq!(Drag::parse(&bank.text()), Some(bank.clone()));
        assert_eq!(
            drop_on_card(2, bank.clone()),
            AlignEdit::PlaceWords {
                card: 2,
                words: vec![paul.clone()]
            }
        );
        let placed = Drag::Placed {
            from: 1,
            word: paul.clone(),
        };
        assert_eq!(
            drop_on_card(2, placed.clone()),
            AlignEdit::Move {
                from: 1,
                to: 2,
                word: paul.clone()
            }
        );
        assert_eq!(
            drop_on_bank(placed),
            Some(AlignEdit::Unplace {
                card: 1,
                word: paul
            })
        );
        assert_eq!(
            drop_on_bank(bank),
            None,
            "bank words are not dropped on the bank"
        );
        let card = Drag::Card { from: 0 };
        assert_eq!(
            drop_on_card(1, card.clone()),
            AlignEdit::Merge { from: 0, to: 1 }
        );
        assert_eq!(drop_on_bank(card), None);
    }

    #[test]
    fn keys_select_and_place_without_a_pointer() {
        assert_eq!(bank_click(false, false), BankClick::Plain);
        assert_eq!(bank_click(true, true), BankClick::Toggle);
        assert_eq!(bank_click(false, true), BankClick::Range);
        assert_eq!(bank_key("Enter", true, false), Some(BankClick::Toggle));
        assert_eq!(bank_key(" ", false, true), Some(BankClick::Range));
        assert_eq!(
            bank_key("Enter", false, false),
            None,
            "a plain Enter is the button's own click"
        );
        assert_eq!(bank_key("a", true, false), None);
        assert!(card_key_places("Enter", true));
        assert!(card_key_places(" ", true));
        assert!(!card_key_places("Enter", false), "nothing is selected");
        assert!(!card_key_places("Tab", true));
    }

    fn click(n: u32) -> AlignEdit {
        AlignEdit::PlaceSelection { card: n }
    }

    /// J5: five clicks made faster than the server replies go out one at a time, in the order
    /// they were made, and none is refused or lost.
    #[test]
    fn fast_clicks_are_queued_not_refused() {
        let mut queue = EditQueue::default();
        assert!(queue.is_idle());
        assert_eq!(
            queue.push(click(0)),
            Some(click(0)),
            "the first goes at once"
        );
        for n in 1..5 {
            assert_eq!(queue.push(click(n)), None, "{n} waits for the reply");
        }
        let mut sent = vec![click(0)];
        assert!(!queue.is_idle(), "a reply is still outstanding");
        while let Some(next) = queue.replied() {
            assert!(!queue.is_idle(), "the next click is now on its way");
            sent.push(next);
        }
        assert!(queue.is_idle(), "the final reply drained the queue");
        assert_eq!(sent, (0..5).map(click).collect::<Vec<_>>());
        // And a click after everything has settled goes at once again.
        assert_eq!(queue.push(click(9)), Some(click(9)));
    }

    #[test]
    fn edits_made_on_a_view_that_is_gone_are_dropped_together() {
        let mut queue = EditQueue::default();
        queue.push(click(0));
        queue.push(click(1));
        queue.push(click(2));
        assert_eq!(queue.drop_waiting(), 2);
        assert_eq!(queue.replied(), None);
        assert_eq!(queue.push(click(3)), Some(click(3)));
    }

    fn sent(edit: AlignEdit, n: u8) -> SentEdit {
        SentEdit {
            request: Edit {
                at: tc_api::ids::Scope {
                    project: tc_api::ids::ProjectRef::parse("queued").unwrap(),
                    epoch: tc_api::ids::Epoch::new(1),
                },
                request: tc_api::ids::RequestId::from_bytes([n; 16]),
                aligner: tc_api::ids::AlignerId::new(7),
                seq: u64::from(n),
                edit,
            },
            run: tc_api::ids::ServerInstance::from_bytes([9; 16]),
        }
    }

    #[test]
    fn a_failed_request_and_later_clicks_stay_pending_until_the_exact_retry_succeeds() {
        let mut queue = EditQueue::default();
        let first = queue.push(click(0)).unwrap();
        let request = sent(first, 1);
        queue.sent(request.clone());
        queue.push(click(1));
        queue.failed(EditFailure::Retryable("reply lost".into()));
        assert!(!queue.is_idle());
        assert_eq!(queue.pending(), vec![click(0), click(1)]);
        assert_eq!(queue.retry(), Some(request));
        assert_eq!(queue.replied(), Some(click(1)));
        assert_eq!(queue.pending(), vec![click(1)]);
        assert_eq!(queue.replied(), None);
        assert!(queue.is_idle());
    }

    #[test]
    fn home_asks_only_once_a_click_is_lost_for_sure() {
        let mut queue = EditQueue::default();
        assert_eq!(queue.on_leave(LEAVE_WAIT_MS), LeaveWait::Leave);
        let first = queue.push(click(0)).unwrap();
        queue.sent(sent(first, 1));
        assert_eq!(queue.on_leave(0.0), LeaveWait::Wait, "on its way");
        queue.failed(EditFailure::Retryable("reply lost".into()));
        assert_eq!(queue.on_leave(10.0), LeaveWait::Retry);
        assert_eq!(
            queue.on_leave(LEAVE_WAIT_MS),
            LeaveWait::Ask,
            "stuck past the limit"
        );
        queue.retry();
        queue.failed(EditFailure::Refused("the aligner changed".into()));
        assert_eq!(queue.on_leave(0.0), LeaveWait::Ask, "refused for good");
    }

    #[test]
    fn a_stale_actor_is_refused_with_the_original_actions_retained() {
        let mut queue = EditQueue::default();
        let first = queue.push(click(0)).unwrap();
        queue.sent(sent(first, 1));
        queue.push(click(1));
        queue.failed(EditFailure::Refused("the aligner changed".into()));
        assert_eq!(queue.retry(), None);
        assert_eq!(queue.replied(), None);
        assert_eq!(queue.pending(), vec![click(0), click(1)]);
        assert!(matches!(queue.failure(), Some(EditFailure::Refused(_))));
        assert!(!queue.is_idle());
    }

    fn conservation(actions: &[u8], drop_on_failure: bool) -> bool {
        let mut queue = EditQueue::default();
        let mut expected = VecDeque::new();
        let mut next = 0u32;
        let mut failed = false;
        for action in actions {
            match action % 4 {
                0 => {
                    let edit = click(next);
                    next += 1;
                    expected.push_back(edit.clone());
                    if let Some(edit) = queue.push(edit) {
                        queue.sent(sent(edit, (next % 250) as u8));
                    }
                }
                1 if !expected.is_empty() && !failed => {
                    if drop_on_failure {
                        queue.drop_waiting();
                    } else {
                        queue.failed(EditFailure::Retryable("lost".into()));
                    }
                    failed = true;
                }
                2 if failed => {
                    if queue.retry().is_none() {
                        return false;
                    }
                    failed = false;
                }
                3 if !expected.is_empty() && !failed => {
                    expected.pop_front();
                    if let Some(edit) = queue.replied() {
                        queue.sent(sent(edit, (next % 250) as u8));
                    }
                }
                _ => {}
            }
            if queue.pending() != expected.iter().cloned().collect::<Vec<_>>() {
                return false;
            }
            if queue.is_idle() != expected.is_empty() {
                return false;
            }
        }
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    proptest::proptest! {
        #[test]
        fn transport_failures_and_retries_conserve_every_unanswered_click(
            actions in proptest::collection::vec(0u8..4, 0..200)
        ) {
            proptest::prop_assert!(conservation(&actions, false));
        }
    }

    #[test]
    fn the_old_drop_on_failure_is_caught_by_the_same_conservation_model() {
        let actions = [0, 0, 1, 2, 3, 3];
        assert!(conservation(&actions, false));
        assert!(!conservation(&actions, true));
    }
}
