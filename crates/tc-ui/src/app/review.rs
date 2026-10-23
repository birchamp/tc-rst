//! The conflict review (unit B11): tC4's `ConflictReview.jsx` and its actions in `state.jsx`
//! (`openConflictReview`, `showConflict`, `closeConflictReview`, `keepVersion`) as the
//! decisions its view draws, and the tabs the rebuild adds for the other kinds (STORAGE §8.5,
//! a rebuild-only design under Q-9).
//!
//! Colours follow the owner, never the position: "Mine" is light blue on every conflict and
//! every teammate's version cream (owner ruling 2026-10-13, #679). One label function names a
//! person here and on the sync result (#680, [`crate::app::sync::person_label`]).

use tc_api::conflicts::{
    ConflictEntry, MadeBy, NoticeView, ReviewPlace, ReviewTab, TabCount, Version, VersionCause,
};
use tc_api::ids::{ChangeRef, ConflictRef, RecordRef, RequestId};
use tc_api::shown::Who;
use tc_core::BookId;

use crate::app::sync::person_label;
use crate::i18n::{keys, t};

/// The review as it stands: tC4's `s.conflictReview` and the view's own state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewState {
    pub tab: ReviewTab,
    /// The conflict shown; `None` shows the first.
    pub conflict: Option<ConflictRef>,
    /// Where the shown conflict stood in the list. After a keep the conflict leaves the list
    /// and the one now at its place shows (tC4 `keepOutcome`).
    pub position: usize,
    /// The version clicked: the others fade.
    pub selected: Option<ChangeRef>,
    /// Where the last keep stands.
    pub keep: KeepPhase,
    /// The record whose History shows.
    pub history: Option<RecordRef>,
}

/// Where the last keep stands: tC4's `busy`, `keepOutcome` and the request it sent as one
/// state, so a keep is never both in flight and answered. The first network-reply state of
/// the UI; UI-3 generalises it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepPhase {
    /// Nothing sent since the conflict showed.
    Idle,
    /// Sent and not answered: Back, Previous, Next and Keep are off.
    InFlight(SentKeep),
    /// Sent, and no usable answer came (the server did not answer, or the project was closed
    /// under the review). The same press sends the same request again, and the server gives
    /// its first reply if the keep was made (A-27); `error` says why it stopped.
    ReplyLost { sent: SentKeep, error: String },
    /// The server answered, with an `Ack` or a refusal: the next press is a new choice and a
    /// new request, since the same one would get the same refusal again.
    Answered { outcome: Option<KeepOutcome> },
}

/// A keep on its way: which version of which conflict, under which request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentKeep {
    pub conflict: ConflictRef,
    pub change: ChangeRef,
    pub request: RequestId,
}

/// What the server's reply to a keep came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepReply {
    /// The version was kept; the disk may have complained afterwards.
    Kept { disk_warning: Option<String> },
    /// The server refused, in these words.
    Refused(String),
    /// No usable answer, for this reason.
    Lost(String),
}

/// The outcome of the last keep: an error to show, and whether the version was kept anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepOutcome {
    pub kept: bool,
    pub error: String,
}

impl KeepPhase {
    /// A press of Keep for `change` in `conflict`: the request to send, or `None` while one
    /// is in flight. After a lost reply the same choice sends the same request; any other
    /// choice, and any press after an answer, is a new request from `mint`.
    pub fn press(
        &mut self,
        conflict: &ConflictRef,
        change: &ChangeRef,
        mint: impl FnOnce() -> RequestId,
    ) -> Option<RequestId> {
        let sent = match self {
            Self::InFlight(_) => return None,
            Self::ReplyLost { sent, .. }
                if &sent.conflict == conflict && &sent.change == change =>
            {
                sent.clone()
            }
            Self::Idle | Self::ReplyLost { .. } | Self::Answered { .. } => SentKeep {
                conflict: conflict.clone(),
                change: change.clone(),
                request: mint(),
            },
        };
        let request = sent.request;
        *self = Self::InFlight(sent);
        Some(request)
    }

    /// The reply to the keep in flight. A reply is only awaited after a press went in flight,
    /// so in any other state there is nothing it answers.
    pub fn reply(&mut self, reply: KeepReply) {
        let Self::InFlight(sent) = self else {
            return;
        };
        let sent = sent.clone();
        *self = match reply {
            KeepReply::Kept { disk_warning } => Self::Answered {
                outcome: disk_warning.map(|error| KeepOutcome { kept: true, error }),
            },
            KeepReply::Refused(error) => Self::Answered {
                outcome: Some(KeepOutcome { kept: false, error }),
            },
            KeepReply::Lost(error) => Self::ReplyLost { sent, error },
        };
    }

    /// Another conflict shows: an answer is no longer about it. A lost reply stays, so the
    /// same press on the same conflict still sends the same request.
    pub fn dismiss(&mut self) {
        if let Self::Answered { .. } = self {
            *self = Self::Idle;
        }
    }

    /// A keep is in flight: Back, Previous, Next and Keep are off.
    pub fn busy(&self) -> bool {
        matches!(self, Self::InFlight(_))
    }

    /// What the last keep left to say, shown with or without a conflict left.
    pub fn outcome(&self) -> Option<KeepOutcome> {
        match self {
            Self::Idle | Self::InFlight(_) => None,
            Self::ReplyLost { error, .. } => Some(KeepOutcome {
                kept: false,
                error: error.clone(),
            }),
            Self::Answered { outcome } => outcome.clone(),
        }
    }
}

impl ReviewState {
    /// The review at `conflict` of `tab`, or at the tab's first.
    pub fn at(tab: ReviewTab, conflict: Option<ConflictRef>) -> Self {
        Self {
            tab,
            conflict,
            position: 0,
            selected: None,
            keep: KeepPhase::Idle,
            history: None,
        }
    }

    /// The index of the conflict shown among `entries`: the one asked for, else the one at
    /// its last place, else the first. `None` when the tab lists nothing.
    pub fn shown(&self, entries: &[ConflictEntry]) -> Option<usize> {
        if entries.is_empty() {
            return None;
        }
        let at = self
            .conflict
            .as_ref()
            .and_then(|c| entries.iter().position(|e| &e.conflict == c));
        Some(at.unwrap_or_else(|| self.position.min(entries.len() - 1)))
    }

    /// Previous or Next, or a marker: the conflict shows with nothing selected.
    pub fn show(&mut self, conflict: ConflictRef, position: usize) {
        if self.conflict.as_ref() != Some(&conflict) {
            self.selected = None;
            self.history = None;
        }
        self.conflict = Some(conflict);
        self.position = position;
    }

    /// Another tab: its first entry shows.
    pub fn choose_tab(&mut self, tab: ReviewTab) {
        if self.tab != tab {
            *self = Self::at(tab, None);
        }
    }
}

/// Whose version it is, for its colour and its pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Mine,
    Teammate,
}

impl Owner {
    pub fn of(by: &MadeBy) -> Self {
        match by {
            MadeBy::Person(label) if label.who == Who::Mine => Self::Mine,
            _ => Self::Teammate,
        }
    }

    /// `data-owner`.
    pub fn attr(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Teammate => "teammate",
        }
    }

    /// tC4's `OWNER_FILL`.
    pub fn fill(self) -> &'static str {
        match self {
            Self::Mine => "var(--uw-inspire-200)",
            Self::Teammate => "var(--uw-cream)",
        }
    }
}

/// The label of who made a version: the person's, or the source of a change no person made
/// in an app (STORAGE §8.5).
pub fn made_by(by: &MadeBy) -> String {
    match by {
        MadeBy::Person(label) => person_label(label),
        MadeBy::Door43 => t(keys::review::by::DOOR43, &[]),
        MadeBy::ExchangeFile => t(keys::review::by::EXCHANGE_FILE, &[]),
        MadeBy::Import => t(keys::review::by::IMPORT, &[]),
    }
}

/// The footer's name of the kept version: "Your version" or "Teammate 2’s version".
pub fn kept_name(version: &Version) -> String {
    match Owner::of(&version.by) {
        Owner::Mine => t(keys::conflicts::MY_VERSION, &[]),
        Owner::Teammate => t(
            keys::conflicts::THEIR_VERSION,
            &[("owner", &made_by(&version.by))],
        ),
    }
}

/// What Keep this version does, split around the kept version's name, which the footer
/// draws bold: tC4's `KeepBar`.
pub fn consequence(versions: usize) -> (String, String) {
    let others = versions.saturating_sub(1);
    let text = if others == 1 {
        t(keys::conflicts::REPLACES_ONE, &[("n", &others)])
    } else {
        t(keys::conflicts::REPLACES_MANY, &[("n", &others)])
    };
    match text.split_once("{version}") {
        Some((before, after)) => (before.to_owned(), after.to_owned()),
        None => (text, String::new()),
    }
}

/// Where an entry is, in words: "Titus 1:1".
pub fn place_label(place: &ReviewPlace) -> String {
    match place {
        ReviewPlace::Verse {
            book,
            chapter,
            verse,
        } => format!("{} {chapter}:{verse}", book.english_name()),
        ReviewPlace::ChapterFront { book, chapter } => t(
            keys::review::place::CHAPTER_FRONT,
            &[("book", &book.english_name()), ("chapter", chapter)],
        ),
        ReviewPlace::Book { book } => book.english_name().to_owned(),
        ReviewPlace::Check { tool, place } => {
            let tool = match tool {
                Some(tc_api::check::Tool::TranslationNotes) => {
                    t(keys::check::tool::TRANSLATION_NOTES, &[])
                }
                Some(tc_api::check::Tool::TranslationWords) => {
                    t(keys::check::tool::TRANSLATION_WORDS, &[])
                }
                None => String::new(),
            };
            let at = match place {
                tc_api::check::CheckPlace::Verse {
                    book,
                    chapter,
                    verse,
                } => format!("{} {chapter}:{verse}", book.english_name()),
                tc_api::check::CheckPlace::Frame { story, frame } => t(
                    keys::review::place::FRAME,
                    &[("story", &story.get()), ("frame", frame)],
                ),
            };
            if tool.is_empty() {
                at
            } else {
                format!("{at} · {tool}")
            }
        }
        ReviewPlace::Story { story, frame } => match frame {
            Some(frame) => t(
                keys::review::place::FRAME,
                &[("story", &story.get()), ("frame", frame)],
            ),
            None => t(keys::review::place::STORY, &[("story", &story.get())]),
        },
        ReviewPlace::Project => t(keys::review::place::PROJECT, &[]),
    }
}

/// The book an entry is in: Translate opens there for the review (tC4
/// `openProjectConflicts`: the book of the project's first conflict).
pub fn place_book(place: &ReviewPlace) -> Option<BookId> {
    match place {
        ReviewPlace::Verse { book, .. }
        | ReviewPlace::ChapterFront { book, .. }
        | ReviewPlace::Book { book } => Some(*book),
        ReviewPlace::Check {
            place: tc_api::check::CheckPlace::Verse { book, .. },
            ..
        } => Some(*book),
        ReviewPlace::Check { .. } | ReviewPlace::Story { .. } | ReviewPlace::Project => None,
    }
}

/// Where Back returns to: Translate at its chapter, or the Check screen the review was opened
/// over from an aligner or a check (a rebuild-only entry, Q-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackTo {
    Translate(Option<BookId>, u16),
    Check,
}

/// "Back to Titus 1" while a book is open in Translate, else "Back to Translate"; "Back to
/// Check" over the Check screen.
pub fn back_label(to: BackTo) -> String {
    match to {
        BackTo::Translate(Some(book), chapter) => t(
            keys::conflicts::BACK,
            &[("place", &format!("{} {chapter}", book.english_name()))],
        ),
        BackTo::Translate(None, _) => t(keys::conflicts::BACK_TO_TRANSLATE, &[]),
        BackTo::Check => t(
            keys::conflicts::BACK,
            &[("place", &t(keys::nav::CHECK, &[]))],
        ),
    }
}

/// How many other entries of the list one set of changes made with `entry` (STORAGE §8.1).
pub fn group_others(entries: &[ConflictEntry], entry: &ConflictEntry) -> usize {
    entries
        .iter()
        .filter(|e| e.group == entry.group && e.conflict != entry.conflict)
        .count()
}

/// A tab's name.
pub fn tab_label(tab: ReviewTab) -> String {
    t(
        match tab {
            ReviewTab::Text => keys::review::tab::TEXT,
            ReviewTab::Alignments => keys::review::tab::ALIGNMENTS,
            ReviewTab::Checks => keys::review::tab::CHECKS,
            ReviewTab::Settings => keys::review::tab::SETTINGS,
            ReviewTab::Detached => keys::review::tab::DETACHED,
            ReviewTab::Notices => keys::review::tab::NOTICES,
        },
        &[],
    )
}

/// The tabs drawn: none while only the Text tab holds anything, so the review is tC4's one
/// tab; else every tab that holds something, and the one shown.
pub fn tabs_shown(tabs: &[TabCount], shown: ReviewTab) -> Vec<TabCount> {
    let others = tabs.iter().any(|t| t.tab != ReviewTab::Text && t.count > 0);
    if !others && shown == ReviewTab::Text {
        return Vec::new();
    }
    tabs.iter()
        .filter(|t| t.count > 0 || t.tab == shown || t.tab == ReviewTab::Text)
        .copied()
        .collect()
}

/// How a version of History came to be.
pub fn cause_label(cause: VersionCause) -> String {
    use keys::review::cause as c;
    t(
        match cause {
            VersionCause::Edited => c::EDITED,
            VersionCause::Kept => c::KEPT,
            VersionCause::Restored => c::RESTORED,
            VersionCause::CarriedOver => c::CARRIED_OVER,
            VersionCause::EditedOnDoor43 => c::EDITED_ON_DOOR43,
            VersionCause::EditedInAnotherProgram => c::EDITED_IN_ANOTHER_PROGRAM,
            VersionCause::FromAFile => c::FROM_AFILE,
            VersionCause::Imported => c::IMPORTED,
            VersionCause::Created => c::CREATED,
            VersionCause::StartedOver => c::STARTED_OVER,
            VersionCause::ByTheApp => c::BY_THE_APP,
            VersionCause::Newer => c::NEWER,
        },
        &[],
    )
}

/// A notice's sentence.
pub fn notice_text(notice: &NoticeView) -> String {
    use keys::review::notice as n;
    match notice {
        NoticeView::DamagedChanges { recovered } => {
            t(n::DAMAGED_CHANGES, &[("recovered", recovered)])
        }
        NoticeView::NewerApp => t(n::NEWER_APP, &[]),
        NoticeView::ClockAhead { by, days } => {
            let who = by.as_ref().map_or_else(
                || t(n::ANOTHER_COMPUTER, &[]),
                crate::app::sync::person_label,
            );
            t(n::CLOCK_AHEAD, &[("who", &who), ("days", days)])
        }
        NoticeView::OutsideEditRecorded { file, changes } => t(
            n::OUTSIDE_EDIT_RECORDED,
            &[("file", file), ("changes", changes)],
        ),
        NoticeView::OutsideEditHeld { file } => t(n::OUTSIDE_EDIT_HELD, &[("file", file)]),
        NoticeView::OutsideEditUnreadable { file } => {
            t(n::OUTSIDE_EDIT_UNREADABLE, &[("file", file)])
        }
        NoticeView::DetailsEditedOutside => t(n::DETAILS_EDITED_OUTSIDE, &[]),
        NoticeView::WriteBlocked { file } => t(n::WRITE_BLOCKED, &[("file", file)]),
        NoticeView::FileRepaired { file } => t(n::FILE_REPAIRED, &[("file", file)]),
        NoticeView::FileMissing { file } => t(n::FILE_MISSING, &[("file", file)]),
        NoticeView::FileUnreadable { file } => t(n::FILE_UNREADABLE, &[("file", file)]),
        NoticeView::StorageDamage { file } => t(n::STORAGE_DAMAGE, &[("file", file)]),
        NoticeView::FileMoved { file, to } => t(n::FILE_MOVED, &[("file", file), ("to", to)]),
        NoticeView::FolderCopied => t(n::FOLDER_COPIED, &[]),
        NoticeView::SecondWriter => t(n::SECOND_WRITER, &[]),
        NoticeView::SavedWithDiskError => t(n::SAVED_WITH_DISK_ERROR, &[]),
        NoticeView::ImportUnfinished => t(n::IMPORT_UNFINISHED, &[]),
        NoticeView::IncompleteChanges { have, of } => {
            t(n::INCOMPLETE_CHANGES, &[("have", have), ("of", of)])
        }
        NoticeView::KeptApart { changes } => t(n::KEPT_APART, &[("changes", changes)]),
    }
}
