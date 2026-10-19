//! A write's reply as tc-api's `Ack` (APP-DESIGN §3.4, AD4, AD5): exactly what the store
//! stored, read from the snapshot that holds it.

use std::collections::BTreeMap;

use tc_api::ids::ConflictRef;
use tc_api::shown::{PersonLabel, Who};
use tc_api::write::{Ack, ConflictBrief, SavedValue};
use tc_journal::fold::PersonLabel as Label;
use tc_journal::record::VerseValue;
use tc_journal::{DeviceId, Record};
use unicode_normalization::UnicodeNormalization;

use super::WireError;
use super::error::stored;
use super::scope::{person_label, record_ref, view};
use super::token::token_update;
use crate::backend::{EditToken, StoreReply, Stored};
use crate::counters::CountingSnapshot;

/// The `Ack` of a verse save the store answered. `shown` is the token the request carried and
/// `sent` the words it sent. `read` is a snapshot at least as new as the reply's (`reply.seq`).
///
/// - For `Stored::Unchanged` the store returns no token: the field stays clean only on its own
///   token and its own text (STORAGE §5.1; `test/fieldStore` answers the same way), so the
///   `Ack` hands both back. The store judged the words equal to the NFC form of what was sent.
/// - The saved words are read from the body of the batch's change of the verse, never from the
///   newest projection, which a later write may have changed already. A verse save can write
///   two changes, the verse and its reflowed alignment (R25): both tokens go back.
pub fn ack(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: &EditToken,
    sent: &str,
    me: DeviceId,
) -> Result<Ack, WireError> {
    let key = shown.key();
    let record = record_ref(key).ok_or_else(|| WireError::Record(format!("{key:?}")))?;
    let (stored_as, disk_warning) = stored(&reply.stored);
    let nfc: String = sent.nfc().collect();
    let (tokens, text) = match reply.stored {
        Stored::Unchanged => (vec![token_update(key, shown)?], nfc),
        Stored::Durable | Stored::WrittenNotDurable(_) => {
            let tokens = reply
                .tokens
                .iter()
                .map(|(key, token)| token_update(key, token))
                .collect::<Result<Vec<_>, _>>()?;
            let words = reply
                .changes
                .iter()
                .filter_map(|id| read.body(id))
                .filter(|body| body.record.key() == *key)
                .find_map(|body| match body.record {
                    Record::Verse {
                        value: VerseValue::Text { words, .. },
                        ..
                    } => Some(words),
                    _ => None,
                });
            // A verse save stores the NFC form of the words sent; the body is the record of
            // it, and when it cannot be read the stored write is still reported as stored.
            (tokens, words.unwrap_or(nfc))
        }
    };
    let labels = read.person_labels(me);
    let new_conflicts = reply
        .new_conflicts
        .iter()
        .map(|id| {
            let conflict = ConflictRef::parse(&id.to_hex())
                .map_err(|e| WireError::Record(format!("conflict {}: {e}", id.to_hex())))?;
            let found = read.conflict(id);
            let record = found
                .as_ref()
                .and_then(|c| record_ref(&c.key))
                .unwrap_or_else(|| record.clone());
            let by = found
                .map(|c| writer(&c, reply, read, &labels, me))
                .unwrap_or(PersonLabel {
                    who: Who::Mine,
                    computer: None,
                });
            Ok(ConflictBrief {
                conflict,
                record,
                by,
            })
        })
        .collect::<Result<Vec<_>, WireError>>()?;
    Ok(Ack {
        view: view(reply.seq),
        stored: stored_as,
        tokens,
        new_conflicts,
        saved: vec![SavedValue { record, text }],
        rows: Vec::new(),
        disk_warning,
    })
}

/// Who the person's save now conflicts with: the writer of the conflict's first head that
/// this batch did not write ("Juan changed this verse while you were editing"), or the person
/// themselves when every other head is their own.
fn writer(
    conflict: &tc_journal::fold::Conflict,
    reply: &StoreReply,
    read: &CountingSnapshot,
    labels: &BTreeMap<DeviceId, Label>,
    me: DeviceId,
) -> PersonLabel {
    let mine = PersonLabel {
        who: Who::Mine,
        computer: None,
    };
    conflict
        .candidates
        .iter()
        .flat_map(|c| std::iter::once(c.head).chain(c.also.iter().copied()))
        .filter(|head| !reply.changes.contains(head))
        .filter_map(|head| read.body(&head).map(|body| body.device))
        .find(|device| *device != me)
        .map_or(mine, |device| {
            labels.get(&device).map_or(
                PersonLabel {
                    who: Who::Teammate(0),
                    computer: None,
                },
                person_label,
            )
        })
}
