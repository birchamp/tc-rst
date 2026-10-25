//! The conflict review (unit B11): tC4's `ConflictReview.jsx` and its actions in `state.jsx`
//! (`openConflictReview`, `showConflict`, `closeConflictReview`, `keepVersion`) as the
//! decisions its view draws, and the tabs the rebuild adds for the other kinds (STORAGE §8.5,
//! a rebuild-only design under Q-9).
//!
//! Colours follow the owner, never the position: "Mine" is light blue on every conflict and
//! every teammate's version cream (owner ruling 2026-10-13, #679). One label function names a
//! person here and on the sync result (#680, [`crate::app::sync::person_label`]).

use tc_api::conflicts::{
    ConflictEntry, ConflictKind, KeepChoice, MadeBy, NoticeView, ReviewPlace, ReviewTab, TabCount,
    Version, VersionCause,
};
use tc_api::ids::{ChangeRef, ConflictRef, RecordRef, RequestId};
use tc_api::shown::{PersonLabel, Who};
use tc_api::write::SaveRefusal;
use tc_api::{ApiError, NotSaved};
use tc_core::BookId;

use crate::app::client::CallError;
use crate::app::net::NetEnd;
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
    /// Where the last press stands.
    pub keep: KeepPhase,
    /// The record whose History shows.
    pub history: Option<RecordRef>,
}

/// Where the last choice stands: tC4's `busy`, `keepOutcome` and the request it sent as one
/// state, so a choice is never both in flight and answered. The first network-reply state of
/// the UI; UI-3 generalises it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepPhase {
    /// Nothing sent since the conflict showed.
    Idle,
    /// Sent and not answered: Back, Previous, Next and every choice are off.
    InFlight(Sent),
    /// Sent, and no usable answer came (the server did not answer, or the project was closed
    /// under the review). The same press sends the same request again, and the server gives
    /// its first reply if the choice was made (A-27); `error` says why it stopped.
    ReplyLost { sent: Sent, error: String },
    /// The server answered, with an `Ack` or a refusal: the next press is a new choice and a
    /// new request, since the same one would get the same refusal again.
    Answered { outcome: Option<KeepOutcome> },
}

/// A press on its way: what was pressed, under which request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub press: Press,
    pub request: RequestId,
}

/// What a press of the review asks for. Two presses are the same press when they ask for
/// the same thing, so a lost reply's request goes again only for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// A choice on one conflict.
    Keep {
        conflict: ConflictRef,
        choice: KeepChoice,
    },
    /// *Keep all of Maria's* over every conflict of the tab.
    KeepAll { person: Who },
}

/// What the server's reply to a press came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepReply {
    /// The choice was made; the disk may have complained afterwards.
    Kept { disk_warning: Option<String> },
    /// A bulk keep was made: `kept` of `sent` conflicts, the rest for a choice of their own.
    KeptAll {
        person: PersonLabel,
        kept: u32,
        sent: u32,
        disk_warning: Option<String>,
    },
    /// The server refused, in these words.
    Refused(String),
    /// No usable answer, for this reason.
    Lost(String),
}

/// What the last press left to say, shown with or without a conflict left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepOutcome {
    /// "The version was not kept." and why.
    NotKept(String),
    /// Kept; the disk reported an error afterwards.
    DiskWarning(String),
    /// A bulk keep: whose version, in how many of the conflicts sent.
    KeptAll {
        person: PersonLabel,
        kept: u32,
        sent: u32,
        disk_warning: Option<String>,
    },
}

impl KeepPhase {
    /// A press: the request to send, or `None` while one is in flight. After a lost reply the
    /// same press sends the same request; any other press, and any press after an answer, is
    /// a new request from `mint`.
    pub fn press(&mut self, press: &Press, mint: impl FnOnce() -> RequestId) -> Option<RequestId> {
        let sent = match self {
            Self::InFlight(_) => return None,
            Self::ReplyLost { sent, .. } if &sent.press == press => sent.clone(),
            Self::Idle | Self::ReplyLost { .. } | Self::Answered { .. } => Sent {
                press: press.clone(),
                request: mint(),
            },
        };
        let request = sent.request;
        *self = Self::InFlight(sent);
        Some(request)
    }

    /// The reply to `request`, the press in flight. A reply to another request (a press made
    /// before the review was left and opened again) answers nothing here, and in any other
    /// state there is nothing it answers.
    pub fn reply(&mut self, request: RequestId, reply: KeepReply) {
        let Self::InFlight(sent) = self else {
            return;
        };
        if sent.request != request {
            return;
        }
        let sent = sent.clone();
        *self = match reply {
            KeepReply::Kept { disk_warning } => Self::Answered {
                outcome: disk_warning.map(KeepOutcome::DiskWarning),
            },
            KeepReply::KeptAll {
                person,
                kept,
                sent,
                disk_warning,
            } => Self::Answered {
                outcome: Some(KeepOutcome::KeptAll {
                    person,
                    kept,
                    sent,
                    disk_warning,
                }),
            },
            KeepReply::Refused(error) => Self::Answered {
                outcome: Some(KeepOutcome::NotKept(error)),
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

    /// A press is in flight: Back, Previous, Next and every choice are off.
    pub fn busy(&self) -> bool {
        matches!(self, Self::InFlight(_))
    }

    /// What the last press left to say.
    pub fn outcome(&self) -> Option<KeepOutcome> {
        match self {
            Self::Idle | Self::InFlight(_) => None,
            Self::ReplyLost { error, .. } => Some(KeepOutcome::NotKept(error.clone())),
            Self::Answered { outcome } => outcome.clone(),
        }
    }
}

/// What a choice that was not made says after "The version was not kept.": a refusal in the
/// API's words. One arm per variant, so a new kind of answer does not reach the screen as its
/// debug text.
pub fn failure_text(error: &ApiError) -> String {
    match error {
        ApiError::NotSaved(NotSaved::LookAgain) => t(keys::conflicts::OTHER_KEPT, &[]),
        ApiError::NotSaved(NotSaved::Refused(
            SaveRefusal::Choice { reason } | SaveRefusal::InvalidText { reason },
        )) => reason.clone(),
        ApiError::NotSaved(NotSaved::Refused(SaveRefusal::IncoherentView)) => {
            t(keys::review::refused::INCOHERENT, &[])
        }
        ApiError::NotSaved(NotSaved::Refused(SaveRefusal::LastBook)) => {
            t(keys::review::refused::LAST_BOOK, &[])
        }
        ApiError::NotSaved(NotSaved::Refused(SaveRefusal::NotKept)) => {
            t(keys::outside::NOT_KEPT, &[])
        }
        ApiError::NotSaved(NotSaved::Disk(reason)) => {
            t(keys::review::refused::DISK, &[("reason", reason)])
        }
        ApiError::NotSaved(NotSaved::ReadOnly(reason)) => {
            t(crate::app::offer::banner(*reason).sentence, &[])
        }
        ApiError::NotSaved(NotSaved::PlanStale | NotSaved::StaleScope | NotSaved::BadToken) => {
            t(keys::review::refused::LOOK_AGAIN, &[])
        }
        api @ (ApiError::Version { .. }
        | ApiError::BadRequest { .. }
        | ApiError::StaleScope { .. }
        | ApiError::StaleSession
        | ApiError::ToolNotReady
        | ApiError::RequestReused
        | ApiError::Refused(_)
        | ApiError::NeedsInternet(_)
        | ApiError::NeedsSignIn(_)
        | ApiError::Busy(_)
        | ApiError::Unavailable(_)
        | ApiError::Internal { .. }) => api.to_string(),
    }
}

/// What the server's reply came to: an answer, or no usable one. Only a reply from the API
/// is an answer; a failed connection or a closed project may hide a choice that was made, so
/// the same press sends the same request again (A-27).
pub fn keep_reply<T>(reply: NetEnd<T>, kept: impl FnOnce(T) -> KeepReply) -> KeepReply {
    match reply {
        NetEnd::Ok(value) => kept(value),
        NetEnd::Failed(error) => KeepReply::Refused(failure_text(&error)),
        NetEnd::Transport(transport) => KeepReply::Lost(transport.to_string()),
        NetEnd::Dropped => KeepReply::Lost(CallError::Dropped.to_string()),
        // A choice needs neither the internet nor a sign-in; were it asked, the press is
        // open to go again with the same request.
        NetEnd::Asked => KeepReply::Lost(String::new()),
    }
}

/// What the footer offers for a conflict of the Text tab (STORAGE §8.3): one match on every
/// kind, no wildcard, so a kind added later must say what it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choices {
    /// *Keep this version* on the version clicked.
    Candidate,
    /// *Keep Maria's passage*: one button per person with a version.
    PassageSides,
    /// *Keep the joined verses* or *Keep the verses separate*, with the version clicked as
    /// the pick where a verse holds several.
    JoinedOrSeparate,
    /// No choice in this version of translationCore (C7b): every version is kept.
    Later,
}

pub fn choices(kind: ConflictKind) -> Choices {
    match kind {
        ConflictKind::VerseText
        | ConflictKind::StructureIssue
        | ConflictKind::BookHeader
        | ConflictKind::ChapterFront
        | ConflictKind::StoryBase
        | ConflictKind::Frame
        | ConflictKind::StoryRef => Choices::Candidate,
        ConflictKind::Passage => Choices::PassageSides,
        ConflictKind::VerseStructure => Choices::JoinedOrSeparate,
        ConflictKind::BookPresence
        | ConflictKind::BookRemovedWhileEdited
        | ConflictKind::LastBookRemoved
        | ConflictKind::BookLayout
        | ConflictKind::Alignment
        | ConflictKind::Check
        | ConflictKind::CheckMoved
        | ConflictKind::Comment
        | ConflictKind::Pins
        | ConflictKind::Setting
        | ConflictKind::ProjectMeta
        | ConflictKind::Versification
        | ConflictKind::CarriedFile
        | ConflictKind::PathCase
        | ConflictKind::DeviceProfile
        | ConflictKind::Unknown => Choices::Later,
    }
}

/// The persons with a version in `entry`, each once in the order their versions are listed:
/// the sides a passage choice and a bulk keep are offered for. A change no person made here
/// (Door43, a file, an import) is no side.
pub fn persons(entry: &ConflictEntry) -> Vec<PersonLabel> {
    let mut out: Vec<PersonLabel> = Vec::new();
    for version in &entry.versions {
        if let MadeBy::Person(label) = &version.by
            && !out.contains(label)
        {
            out.push(label.clone());
        }
    }
    out
}

/// The changes of `person`'s versions in `entry`: a passage choice's picks.
pub fn picks_of(entry: &ConflictEntry, person: &PersonLabel) -> Vec<ChangeRef> {
    entry
        .versions
        .iter()
        .filter(|v| matches!(&v.by, MadeBy::Person(label) if label == person))
        .map(|v| v.change.clone())
        .collect()
}

/// "Keep my passage" or "Keep Teammate 2’s passage".
pub fn keep_passage_label(person: &PersonLabel) -> String {
    match person.who {
        Who::Mine if person.computer.is_none() => t(keys::review::KEEP_MY_PASSAGE, &[]),
        _ => t(
            keys::review::KEEP_PASSAGE,
            &[("person", &person_label(person))],
        ),
    }
}

/// "Keep all of mine" or "Keep all of Teammate 2’s".
pub fn keep_all_label(person: &PersonLabel) -> String {
    match person.who {
        Who::Mine if person.computer.is_none() => t(keys::review::KEEP_ALL_MINE, &[]),
        _ => t(keys::review::KEEP_ALL, &[("person", &person_label(person))]),
    }
}

/// What a bulk keep says: whose version was kept in how many of the conflicts sent; the
/// rest are left for a choice of their own.
pub fn kept_all_text(person: &PersonLabel, kept: u32, sent: u32) -> String {
    match person.who {
        Who::Mine if person.computer.is_none() => t(
            keys::review::KEPT_ALL_MINE,
            &[("kept", &kept), ("sent", &sent)],
        ),
        _ => t(
            keys::review::KEPT_ALL,
            &[
                ("person", &person_label(person)),
                ("kept", &kept),
                ("sent", &sent),
            ],
        ),
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
        NoticeView::ChangedWhileReadOnly { file } => t(
            n::CHANGED_WHILE_READ_ONLY,
            &[(
                "name",
                &crate::app::outside::name_of(file, crate::app::outside::book_of_file(file)),
            )],
        ),
    }
}
