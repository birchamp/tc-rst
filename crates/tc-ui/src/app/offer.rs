//! The draft offer at an open and the read-only banner (APP-DESIGN 6.4, 6.6, C1b): what the
//! page sends with `session/drafts`, the question each offer asks, what the page's buffer and
//! the shell's mirror drop once an offer is settled, and the banner of a read-only project.
//!
//! "Unsaved text from your last session was found for Titus 1:4. Keep it / Discard." One
//! question at a time, in the server's order. The page carries out the page's actions of
//! `tc_api::model::draft` (APP-DESIGN 6.6): what it sends, the slots it drops once the server
//! holds their text, each only while it still holds that text, and the copy.

use std::collections::BTreeMap;

use tc_api::buffer::{BufferEntry, BufferKey, BufferLine};
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance};
use tc_api::model::draft::{self as machine, Action, Event, FieldDraft};
use tc_api::session::{BufferSlot, DraftOffer, Keep};
use tc_api::write::ReadOnlyReason;

use super::buffer::{self, Store, StoreError};
use crate::i18n::{Key, keys};

/// Every entry of the folder, the page's and the shell's mirror's (`takeBuffered`), the newest
/// per field and run as [`buffer::merge`] does.
fn entries_of(
    store: &dyn Store,
    mirror: &[String],
    project: &ProjectRef,
    id: &ProjectId,
) -> Vec<(BufferKey, BufferEntry)> {
    buffer::merge(
        buffer::entries_of(store, project, id),
        &buffer::current_lines(mirror),
    )
    .into_iter()
    .filter(|(key, _)| key.project == *project && key.id == *id)
    .collect()
}

/// The `buffered` of `session/drafts`: the folder's entries, each a `changed` line, but those
/// whose field this run's save machine holds (`owned`, by field and run): those save
/// themselves, and offering them as well would let Keep store one text twice and the offer's
/// settling drop typing the machine still writes (6.6). An entry of this run that no field of
/// the machine holds (its project was not open at a reload) is offered like any other.
pub fn buffered_lines(
    store: &dyn Store,
    mirror: &[String],
    project: &ProjectRef,
    id: &ProjectId,
    owned: &dyn Fn(&BufferKey, ServerInstance) -> bool,
) -> Vec<String> {
    entries_of(store, mirror, project, id)
        .into_iter()
        .filter(|(key, entry)| !owned(key, entry.instance))
        .filter_map(|(key, entry)| serde_json::to_string(&BufferLine::Changed { key, entry }).ok())
        .collect()
}

/// Carries out the machine's settling of `slots` (`Action::Settle`): each buffer item goes
/// only while it holds the slot's run and text, and the mirror gets a removal that names the
/// text, so the shell keeps an entry its run wrote since. Returns the mirror's lines
/// (`tc4Shell.bufferChanged`).
pub fn settle(store: &mut dyn Store, slots: &[BufferSlot]) -> Vec<String> {
    slots
        .iter()
        .filter_map(|slot| {
            // A storage that refuses keeps the item: it is offered again, never lost.
            let _ = buffer::remove(store, &slot.key, Some(&slot.text), slot.instance);
            buffer::removed_line_if(&slot.key, slot.instance, &slot.text).ok()
        })
        .collect()
}

/// The page's half of the copy of a read-only project (A-10, 6.4), the machine's
/// `Action::Rekey`: every entry of `from` in the buffer and in the mirror is written under
/// `to`, item and mirror line, and only once all are written are the old slots settled, so a
/// failure leaves both and never neither. An entry only the mirror held goes with the others.
/// Returns the mirror's lines.
pub fn copy(
    store: &mut dyn Store,
    mirror: &[String],
    from: (&ProjectRef, &ProjectId),
    to: (&ProjectRef, &ProjectId),
) -> Result<Vec<String>, StoreError> {
    let entries = entries_of(&*store, mirror, from.0, from.1);
    let mut fields: BTreeMap<BufferKey, Vec<BufferSlot>> = BTreeMap::new();
    let mut held: BTreeMap<(BufferKey, ServerInstance), BufferEntry> = BTreeMap::new();
    for (key, entry) in entries {
        fields.entry(key.clone()).or_default().push(BufferSlot {
            key: key.clone(),
            instance: entry.instance,
            text: entry.text.clone(),
        });
        held.insert((key, entry.instance), entry);
    }
    let mut lines = Vec::new();
    let mut settled = Vec::new();
    for (key, slots) in fields {
        let target = BufferKey {
            project: to.0.clone(),
            id: to.1.clone(),
            field: key.field.clone(),
        };
        let (_, actions) = machine::next(
            FieldDraft::<()>::default(),
            Event::Copy { slots, to: target },
        );
        for action in actions {
            match action {
                Action::Rekey { slot, to } => {
                    let Some(entry) = held.get(&(slot.key.clone(), slot.instance)) else {
                        continue;
                    };
                    lines.push(rekey(store, &slot, &to, entry)?);
                    settled.push(slot);
                }
                // The server copied the folder and its `drafts.json` (`session/copy`).
                Action::CopyFolder => {}
                _ => {}
            }
        }
    }
    lines.extend(settle(store, &settled));
    Ok(lines)
}

/// Writes `slot`'s entry under `to` and returns its mirror line. The buffer item moves as it
/// is, with the keys this build does not know (3.10); an entry only the mirror held is written
/// as an item too, unless the target item holds another run's entry.
fn rekey(
    store: &mut dyn Store,
    slot: &BufferSlot,
    to: &BufferKey,
    entry: &BufferEntry,
) -> Result<String, StoreError> {
    let raw = store
        .get(&buffer::item_key(&slot.key)?)
        .filter(|raw| buffer::parse_entry(raw).as_ref() == Some(entry));
    match raw {
        Some(raw) => {
            store.set(&buffer::item_key(to)?, &raw)?;
            buffer::changed_line(to, &raw)
        }
        None => {
            let written = buffer::write_entry(store, to, entry);
            match (written.failure, written.line) {
                (Some(failure), _) => Err(failure),
                (None, Some(line)) => Ok(line),
                (None, None) => Err(StoreError::Encode("an entry has no line".to_owned())),
            }
        }
    }
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
