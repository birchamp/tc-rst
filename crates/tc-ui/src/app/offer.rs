//! The draft offer at an open and the read-only banner (APP-DESIGN 6.4, 6.6, C1b): what the
//! page sends with `session/drafts`, the question each offer asks, what the page's buffer and
//! the shell's mirror drop once an offer is settled, and the banner of a read-only project.
//!
//! "Unsaved text from your last session was found for Titus 1:4. Keep it / Discard." One
//! question at a time, in the server's order. After *Keep* or *Discard* the offer's buffer
//! items go, each only while it still holds the offered text, so text typed since stays.

use tc_api::buffer::BufferLine;
use tc_api::ids::{ProjectId, ProjectRef};
use tc_api::session::{BufferSlot, DraftOffer, Keep};
use tc_api::write::ReadOnlyReason;

use super::buffer::{self, Store};
use crate::i18n::{Key, keys};

/// The folder's unsaved entries, the page's and the shell's mirror's (`takeBuffered`) merged
/// as [`buffer::merge`] does, each a `changed` line: the `buffered` of `session/drafts`. Every
/// run's entry goes: at an open, none of them belongs to the open's session.
pub fn buffered_lines(
    store: &dyn Store,
    mirror: &[String],
    project: &ProjectRef,
    id: &ProjectId,
) -> Vec<String> {
    let local = buffer::entries_of(store, project, id);
    buffer::merge(local, &buffer::current_lines(mirror))
        .into_iter()
        .filter(|(key, _)| key.project == *project && key.id == *id)
        .filter_map(|(key, entry)| serde_json::to_string(&BufferLine::Changed { key, entry }).ok())
        .collect()
}

/// Drops the buffer items `slots` name, each only while it holds the slot's run and text, and
/// returns the mirror's removal lines (`tc4Shell.bufferChanged`), one per slot: a removal drops
/// only its own run's entry, and the shell may hold an entry the page's storage lost.
pub fn settle(store: &mut dyn Store, slots: &[BufferSlot]) -> Vec<String> {
    slots
        .iter()
        .filter_map(|slot| {
            // A storage that refuses keeps the item: it is offered again, never lost.
            let _ = buffer::remove(store, &slot.key, Some(&slot.text), slot.instance);
            buffer::removed_line(&slot.key, slot.instance).ok()
        })
        .collect()
}

/// The words of one offer: the sentence (with `{place}`) and its two buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfferWords {
    pub sentence: Key,
    pub keep: Key,
    pub discard: Key,
}

pub fn offer_words(offer: &DraftOffer) -> OfferWords {
    let (sentence, keep) = match offer.keep {
        Keep::Save => (keys::drafts::offer::FOUND, keys::drafts::offer::KEEP),
        Keep::AsVersion => (
            keys::drafts::offer::UNVERIFIED,
            keys::drafts::offer::KEEP_AS_VERSION,
        ),
        Keep::CopyText => (keys::drafts::offer::FOUND, keys::drafts::offer::COPY_TEXT),
    };
    OfferWords {
        sentence,
        keep,
        discard: keys::drafts::offer::DISCARD,
    }
}

/// What the banner of a read-only project offers besides its sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerAction {
    /// "Copy the project": `session/copy`, the buffer moved to the copy, the copy opened.
    Copy,
    /// "That computer is closed": `session/otherComputerClosed`, then the project opened
    /// again.
    OtherComputerClosed,
}

/// The banner over a read-only project's screens: its sentence and its buttons. Edit controls
/// are off and typed text stays in the buffer (6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub sentence: Key,
    pub actions: Vec<BannerAction>,
}

/// A copy helps where the folder's place or another writer is the reason; a newer format or a
/// folder that cannot be written is the same in a copy.
pub fn banner(reason: ReadOnlyReason) -> Banner {
    use BannerAction::{Copy, OtherComputerClosed};
    let (sentence, actions) = match reason {
        ReadOnlyReason::NeedsNewerApp => (keys::read_only::NEEDS_NEWER_APP, vec![]),
        ReadOnlyReason::UnsafeDrive => (keys::read_only::UNSAFE_DRIVE, vec![Copy]),
        ReadOnlyReason::CloudSynced => (keys::read_only::CLOUD_SYNCED, vec![Copy]),
        ReadOnlyReason::SecondWriter => (keys::read_only::SECOND_WRITER, vec![Copy]),
        ReadOnlyReason::OpenOnAnotherComputer => (
            keys::read_only::OPEN_ON_ANOTHER_COMPUTER,
            vec![Copy, OtherComputerClosed],
        ),
        ReadOnlyReason::FolderNotWritable => (keys::read_only::FOLDER_NOT_WRITABLE, vec![]),
    };
    Banner { sentence, actions }
}

/// The button's words of a banner action.
pub fn action_words(action: BannerAction) -> Key {
    match action {
        BannerAction::Copy => keys::read_only::COPY,
        BannerAction::OtherComputerClosed => keys::read_only::OTHER_COMPUTER_CLOSED,
    }
}
