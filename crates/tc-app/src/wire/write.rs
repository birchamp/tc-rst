//! A write's reply as tc-api's `Ack` (APP-DESIGN §3.4, AD4, AD5): exactly what the store
//! stored, read from the snapshot that holds it.

use std::collections::BTreeMap;

use tc_api::ids::ConflictRef;
use tc_api::shown::{PersonLabel, Who};
use tc_api::write::{Ack, ConflictBrief, SavedValue};
use tc_journal::fold::PersonLabel as Label;
use tc_journal::record::{CheckValue, VerseValue};
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
    text_ack(reply, read, shown, sent, me, TextPart::Words)
}

/// An Ack's field text must match the layout field, never the unchanged verse words.
pub fn layout_ack(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: &EditToken,
    sent: &str,
    me: DeviceId,
) -> Result<Ack, WireError> {
    text_ack(reply, read, shown, sent, me, TextPart::Layout)
}

enum TextPart {
    Words,
    Layout,
}

fn text_ack(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: &EditToken,
    sent: &str,
    me: DeviceId,
    part: TextPart,
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
                        value: VerseValue::Text { words, layout, .. },
                        ..
                    } => Some(match part {
                        TextPart::Words => words,
                        TextPart::Layout => layout,
                    }),
                    _ => None,
                });
            // A verse save stores the NFC form of the words sent; the body is the record of
            // it, and when it cannot be read the stored write is still reported as stored.
            (tokens, words.unwrap_or(nfc))
        }
    };
    let new_conflicts = new_conflicts(reply, read, &record, me)?;
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

/// A frame, reference or user-comment save, including the minted identity of a new
/// comment. Read the acknowledged batch rather than a later projected revision.
pub fn record_ack(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: Option<&EditToken>,
    sent: &str,
    me: DeviceId,
) -> Result<Ack, WireError> {
    let key = shown
        .map(EditToken::key)
        .or_else(|| {
            reply.tokens.iter().find_map(|(key, _)| {
                matches!(key, tc_journal::RecordKey::Comment(_)).then_some(key)
            })
        })
        .ok_or_else(|| WireError::Record("a saved comment has no record token".into()))?;
    let record = record_ref(key).ok_or_else(|| WireError::Record(format!("{key:?}")))?;
    let (stored, disk_warning) = stored(&reply.stored);
    let tokens = if matches!(reply.stored, Stored::Unchanged) {
        vec![token_update(
            key,
            shown.ok_or_else(|| WireError::Record("a new comment was unchanged".into()))?,
        )?]
    } else {
        reply
            .tokens
            .iter()
            .map(|(key, token)| token_update(key, token))
            .collect::<Result<Vec<_>, _>>()?
    };
    let text = reply
        .changes
        .iter()
        .filter_map(|id| read.body(id))
        .filter(|body| body.record.key() == *key)
        .find_map(|body| match body.record {
            Record::Frame { value, .. } => Some(value),
            Record::StoryRef { value, .. } => Some(value.unwrap_or_default()),
            Record::Comment { value, .. } => Some(value.text),
            _ => None,
        })
        .unwrap_or_else(|| sent.nfc().collect());
    Ok(Ack {
        view: view(reply.seq),
        stored,
        tokens,
        new_conflicts: new_conflicts(reply, read, &record, me)?,
        saved: vec![SavedValue { record, text }],
        rows: Vec::new(),
        disk_warning,
    })
}

/// The `Ack` of a decision save the store answered. `shown` is the decision token the request
/// carried; `comment` is the comment the request asked to store, which the `Ack` hands back as
/// what the check comment's field saved (the stored one, read from the batch's change of the
/// decision). `rows` are the check's row as the newest snapshot shows it.
pub fn ack_decision(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: &EditToken,
    comment: &str,
    me: DeviceId,
    rows: Vec<tc_api::check::RailRowPatch>,
) -> Result<Ack, WireError> {
    let key = shown.key();
    let record = record_ref(key).ok_or_else(|| WireError::Record(format!("{key:?}")))?;
    let (stored_as, disk_warning) = stored(&reply.stored);
    let (tokens, text) = match reply.stored {
        Stored::Unchanged => (vec![token_update(key, shown)?], comment.to_owned()),
        Stored::Durable | Stored::WrittenNotDurable(_) => {
            let tokens = reply
                .tokens
                .iter()
                .map(|(key, token)| token_update(key, token))
                .collect::<Result<Vec<_>, _>>()?;
            let stored = reply
                .changes
                .iter()
                .filter_map(|id| read.body(id))
                .filter(|body| body.record.key() == *key)
                .find_map(|body| match body.record {
                    Record::Check {
                        value: CheckValue::Decision(decision),
                        ..
                    } => Some(decision.comment.unwrap_or_default()),
                    _ => None,
                });
            (tokens, stored.unwrap_or_else(|| comment.to_owned()))
        }
    };
    let new_conflicts = new_conflicts(reply, read, &record, me)?;
    Ok(Ack {
        view: view(reply.seq),
        stored: stored_as,
        tokens,
        new_conflicts,
        saved: vec![SavedValue { record, text }],
        rows,
        disk_warning,
    })
}

/// The `Ack` of a section save the store answered. `shown` is the token of every verse the
/// section displayed, in order, and `sent` the text of the card. The field's one value is the
/// card's text, keyed under the first verse, whose token comes back whether the batch wrote
/// it or not; every other displayed verse and every record the batch wrote comes back with
/// the token the next save uses (the batch's own when it wrote the record).
pub fn section_ack(
    reply: &StoreReply,
    read: &CountingSnapshot,
    shown: &[EditToken],
    sent: &str,
    me: DeviceId,
) -> Result<Ack, WireError> {
    let first = shown
        .first()
        .ok_or_else(|| WireError::Record("a section save displayed no verse".to_owned()))?;
    let record =
        record_ref(first.key()).ok_or_else(|| WireError::Record(format!("{:?}", first.key())))?;
    let (stored_as, disk_warning) = stored(&reply.stored);
    let mut tokens = Vec::new();
    for token in shown {
        let next = reply
            .tokens
            .iter()
            .find(|(key, _)| key == token.key())
            .map_or(token, |(_, next)| next);
        tokens.push(token_update(token.key(), next)?);
    }
    for (key, token) in &reply.tokens {
        if !shown.iter().any(|t| t.key() == key) {
            tokens.push(token_update(key, token)?);
        }
    }
    let new_conflicts = new_conflicts(reply, read, &record, me)?;
    Ok(Ack {
        view: view(reply.seq),
        stored: stored_as,
        tokens,
        new_conflicts,
        saved: vec![SavedValue {
            record,
            text: sent.nfc().collect(),
        }],
        rows: Vec::new(),
        disk_warning,
    })
}

/// The conflicts a batch made, each with who the person's text now stands beside. `record`
/// names a conflict the snapshot no longer holds.
pub fn new_conflicts(
    reply: &StoreReply,
    read: &CountingSnapshot,
    record: &tc_api::ids::RecordRef,
    me: DeviceId,
) -> Result<Vec<ConflictBrief>, WireError> {
    if reply.new_conflicts.is_empty() {
        return Ok(Vec::new());
    }
    let labels = read.person_labels(me);
    reply
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
        .collect()
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
