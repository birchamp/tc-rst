//! The draft offer at an open (APP-DESIGN §6.6, C1b): `session/drafts`, `session/keepDraft`,
//! `session/discardDraft`, and the copy offer of a read-only project, `session/copy` (§6.4).
//!
//! Unsaved text of an earlier run is in up to three places: the project's `.tc/drafts.json`
//! (the store's offered entries, O12), the page's draft buffer and the shell's mirror (the UI
//! sends those as `buffered`, but the fields its own save machine holds). The decisions are
//! `tc_api::model::draft`'s table; this module carries out its actions on `drafts.json` and
//! the record, and gives the page the slots it drops:
//!
//! - Only entries of this folder and project id count (A-9), and never a test field.
//! - Each unsaved text is taken into `drafts.json` as an offered entry before the answer, and
//!   only then are its buffer slots answered in `settle`; *Keep* and *Discard* then name that
//!   entry by its number.
//! - A read-only project offers nothing and takes nothing in (STORAGE §5.14): the entries wait
//!   for the copy or a writable open.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use tc_api::buffer::{BufferLine, FieldKind, FieldPlace};
use tc_api::event::SessionMode;
use tc_api::ids::ProjectRef;
use tc_api::model::draft::{
    self as machine, Action, Base, Candidate, Event, FieldDraft, Home, Offer,
};
use tc_api::refusal::CreateRefusal;
use tc_api::session::{
    BufferSlot, CopyProject, DiscardDraft, DraftKind, DraftOffer, Drafts, Keep, KeepDraft,
    KeptDraft, OfferedDrafts, OtherComputerClosed, ProjectCopy, Settles, StoredDraftId,
};
use tc_api::{ApiError, NotSaved, Refusal, ShownToken};
use tc_journal::command::DraftField;
use tc_journal::keys::CheckTarget;
use tc_journal::record::{CheckValue, VerseValue};
use tc_journal::{Record, RecordKey, WallTime};

use crate::app::App;
use crate::backend::{CopyFailure, DraftToken, OfferedDraft, ProjectHandle, StoreCommand};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::reports::facts;
use crate::session::ProjectSession;
use crate::wire;

/// `kind` of a buffered field; `None` for a field of no project (a test field).
fn kind_of_field(kind: FieldKind) -> Option<DraftKind> {
    Some(match kind {
        FieldKind::Verse => DraftKind::Words,
        FieldKind::Layout => DraftKind::Layout,
        FieldKind::Section => DraftKind::Section,
        FieldKind::Comment => DraftKind::Comment,
        FieldKind::CheckComment => DraftKind::CheckComment,
        // A story's title is its frame 0.
        FieldKind::Frame | FieldKind::StoryTitle => DraftKind::Frame,
        FieldKind::StoryRef => DraftKind::StoryRef,
        FieldKind::Test => return None,
    })
}

fn kind_of_draft(field: &DraftField) -> DraftKind {
    match field {
        DraftField::Words => DraftKind::Words,
        DraftField::Layout => DraftKind::Layout,
        DraftField::Section { .. } => DraftKind::Section,
        DraftField::Frame => DraftKind::Frame,
        DraftField::StoryRef => DraftKind::StoryRef,
        DraftField::CheckComment => DraftKind::CheckComment,
        DraftField::UserComment { .. } => DraftKind::Comment,
        DraftField::Alignment => DraftKind::Alignment,
    }
}

/// The text `record` holds in a field of `kind`; `None` where the field's text is no record
/// value this build compares (a section's buffer, an alignment's links).
fn field_text(kind: DraftKind, record: Option<Record>) -> Option<String> {
    match (kind, record?) {
        (
            DraftKind::Words,
            Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            },
            // A draft holds the editor text the card showed (tC4 #810, REF1b).
        ) => Some(tc_usfm::poem::editor_text(&words)),
        (
            DraftKind::Layout,
            Record::Verse {
                value: VerseValue::Text { layout, .. },
                ..
            },
        ) => Some(layout),
        (DraftKind::Frame, Record::Frame { value, .. }) => Some(value),
        (DraftKind::StoryRef, Record::StoryRef { value, .. }) => Some(value.unwrap_or_default()),
        (
            DraftKind::CheckComment,
            Record::Check {
                value: CheckValue::Decision(decision),
                ..
            },
        ) => Some(decision.comment.unwrap_or_default()),
        (DraftKind::Comment, Record::Comment { value, .. }) => Some(value.text),
        _ => None,
    }
}

/// Where a record is, for the question's words: `TIT 1:4`, `TIT 1` for a chapter's front,
/// `03-07` for a story's frame.
fn place_of(key: &RecordKey) -> String {
    match key {
        RecordKey::Verse(book, chapter, verse) | RecordKey::Alignment(book, chapter, verse) => {
            format!("{} {chapter}:{verse}", book.code())
        }
        RecordKey::ChapterFront(book, chapter) => format!("{} {chapter}", book.code()),
        RecordKey::BookHeader(book) | RecordKey::BookLayout(book) => book.code().to_owned(),
        RecordKey::Frame(story, frame) => format!("{:02}-{:02}", story.get(), frame.get()),
        RecordKey::StoryRef(story) => format!("{:02}", story.get()),
        RecordKey::Check(check) => match &check.target {
            CheckTarget::Verse { book, .. } => {
                let (chapter, verse) = check.target.parts();
                format!("{} {chapter}:{verse}", book.code())
            }
            CheckTarget::Frame { story, frame } => format!("{:02}-{:02}", story.get(), frame.get()),
        },
        other => wire::scope::record_ref(other).map_or_else(|| format!("{other:?}"), String::from),
    }
}

fn field_place(text: &str) -> FieldPlace {
    let cut: String = text
        .chars()
        .filter(|c| !c.is_control())
        .scan(0, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= FieldPlace::MAX_BYTES).then_some(c)
        })
        .collect();
    FieldPlace::parse(&cut).unwrap_or_else(|_| {
        // A record always has a place; a key that formats to nothing is still offered.
        FieldPlace::parse("?").unwrap_or_else(|_| unreachable!("'?' is a valid place"))
    })
}

fn identity(kind: DraftKind, key: Option<&RecordKey>, place: &FieldPlace) -> String {
    match key.and_then(wire::scope::record_ref) {
        Some(record) => format!("{kind:?}|{}", String::from(record)),
        None => format!("{kind:?}|place:{place}"),
    }
}

/// The record a comment's target names, for its place.
fn comment_place(target: &tc_journal::record::CommentTarget) -> RecordKey {
    use tc_journal::record::CommentTarget;
    match target {
        CommentTarget::Verse {
            book,
            chapter,
            verse,
        } => RecordKey::Verse(*book, *chapter, *verse),
        CommentTarget::Check(check) => RecordKey::Check(check.clone()),
        CommentTarget::Frame { story, frame } => RecordKey::Frame(*story, *frame),
    }
}

/// The tokens a version stands for, as the store reads them: `None` when one does not parse.
fn parse_tokens(
    handle: &dyn ProjectHandle,
    shown: &[ShownToken],
) -> Option<Vec<(DraftToken, bool)>> {
    shown
        .iter()
        .map(|token| DraftToken::parse(handle, token).ok())
        .collect()
}

/// A new comment's verified base is empty: its durable offer supplies the typed target.
/// Unknown old buffer metadata stays available as Copy text.
fn base_of(
    handle: &dyn ProjectHandle,
    kind: DraftKind,
    shown: Vec<ShownToken>,
    new_comment: bool,
) -> Base<Vec<ShownToken>> {
    if kind == DraftKind::Comment && shown.is_empty() && new_comment {
        return Base::Verified(shown);
    }
    let keepable = match kind {
        DraftKind::Section => false,
        DraftKind::Comment => !shown.is_empty(),
        _ => true,
    };
    match parse_tokens(handle, &shown) {
        Some(tokens) if keepable && !tokens.is_empty() => {
            if tokens.iter().all(|(_, verified)| *verified) {
                Base::Verified(shown)
            } else {
                Base::Unverified(shown)
            }
        }
        _ => Base::None,
    }
}

/// What the store holds of one field, for the machine.
struct FieldRecord<'a> {
    handle: &'a dyn ProjectHandle,
    read: &'a CountingSnapshot,
    kind: DraftKind,
    key: Option<RecordKey>,
    comment_target: Option<&'a tc_journal::record::CommentTarget>,
}

impl machine::Record<Vec<ShownToken>> for FieldRecord<'_> {
    fn heads(&self) -> Vec<String> {
        let Some(key) = &self.key else {
            // A first Add may have committed before its Ack reached the mirror. Equality
            // proves that this text is stored; it does not prove which comment the request
            // minted. Never use this target search as the base of a different text.
            let Some(target) = self.comment_target else {
                return Vec::new();
            };
            let keys = match target {
                tc_journal::record::CommentTarget::Verse { book, .. } => {
                    self.read.comments_of(*book)
                }
                tc_journal::record::CommentTarget::Frame { story, .. } => {
                    self.read.story_comments_of(*story)
                }
                tc_journal::record::CommentTarget::Check(_) => return Vec::new(),
            };
            return keys
                .iter()
                .flat_map(|key| self.read.heads(key))
                .filter_map(|head| self.read.body(&head))
                .filter_map(|body| match body.record {
                    Record::Comment { value, .. } if value.target == *target => Some(value.text),
                    _ => None,
                })
                .collect();
        };
        self.read
            .heads(key)
            .iter()
            .filter_map(|head| self.read.body(head))
            .filter_map(|body| field_text(self.kind, Some(body.record)))
            .collect()
    }

    fn sole(&self) -> Option<Vec<ShownToken>> {
        let key = self.key.as_ref()?;
        (self.read.heads(key).len() == 1).then_some(())?;
        let token = self.read.mint(key, None).ok()?;
        Some(vec![wire::token::shown(&token).ok()?])
    }

    fn text_of(&self, version: &Vec<ShownToken>) -> Option<String> {
        let (token, _) = DraftToken::parse(self.handle, version.first()?).ok()?;
        let head = token.heads().first()?;
        field_text(self.kind, Some(self.read.body(head)?.record))
    }

    fn held_since(&self, version: &Vec<ShownToken>, text: &str) -> bool {
        self.after(version).into_iter().any(|body| {
            field_text(self.kind, Some(body.record))
                .is_some_and(|held| machine::same_text(&held, text))
        })
    }

    fn saved_here_since(&self, version: &Vec<ShownToken>, at: i64) -> bool {
        let me = self.handle.facts().device;
        self.after(version)
            .into_iter()
            .any(|body| body.device == me && body.at.0 >= at)
    }
}

impl FieldRecord<'_> {
    /// The changes of the record after `version`: those it is an ancestor of, back from the
    /// heads. A head on another branch is not after it.
    fn after(&self, version: &[ShownToken]) -> Vec<tc_journal::ChangeBody> {
        let (Some(key), Some(first)) = (&self.key, version.first()) else {
            return Vec::new();
        };
        let Ok((token, _)) = DraftToken::parse(self.handle, first) else {
            return Vec::new();
        };
        let base = token.heads();
        let mut memo: BTreeMap<tc_journal::ChangeId, bool> = BTreeMap::new();
        let mut stack: Vec<(tc_journal::ChangeId, bool)> = self
            .read
            .heads(key)
            .into_iter()
            .map(|h| (h, false))
            .collect();
        while let Some((change, expanded)) = stack.pop() {
            if memo.contains_key(&change) {
                continue;
            }
            if base.contains(&change) {
                memo.insert(change, true);
                continue;
            }
            let parents = self
                .read
                .body(&change)
                .map(|body| body.parents)
                .unwrap_or_default();
            if expanded {
                let after = parents
                    .iter()
                    .any(|p| memo.get(p).copied().unwrap_or(false));
                memo.insert(change, after);
            } else {
                stack.push((change, true));
                stack.extend(parents.into_iter().map(|p| (p, false)));
            }
        }
        memo.into_iter()
            .filter(|(change, after)| *after && !base.contains(change))
            .filter_map(|(change, _)| self.read.body(&change))
            .collect()
    }
}

/// One field's candidates, before the machine judges them.
struct Field {
    kind: DraftKind,
    place: FieldPlace,
    key: Option<RecordKey>,
    comment_target: Option<tc_journal::record::CommentTarget>,
    candidates: Vec<Candidate<Vec<ShownToken>>>,
}

/// The offer of `session` for the buffered lines the UI sent (see the module documentation).
/// Each text the machine offers is taken into `drafts.json` before this returns.
pub fn offer(session: &ProjectSession, buffered: &[String], mirror: bool) -> OfferedDrafts {
    let handle = session.handle();
    if matches!(handle.facts().mode, SessionMode::ReadOnly(_)) {
        return OfferedDrafts {
            offers: Vec::new(),
            settle: Vec::new(),
        };
    }
    let read = session.read();
    let mut fields: BTreeMap<String, Field> = BTreeMap::new();
    for draft in handle.offered_drafts() {
        let (id, field) = stored_field(handle.as_ref(), &draft);
        fields.entry(id).or_insert(field.0).candidates.push(field.1);
    }
    for line in buffered {
        let Ok(BufferLine::Changed { key, entry }) = serde_json::from_str::<BufferLine>(line)
        else {
            continue;
        };
        if key.project != session.scope().project || key.id != *session.id() {
            continue;
        }
        let Some(kind) = kind_of_field(key.field.kind) else {
            continue;
        };
        let record = parse_tokens(handle.as_ref(), &entry.base)
            .and_then(|tokens| tokens.first().map(|(token, _)| token.key().clone()));
        let place = key.field.place.clone();
        let comment_target = if kind == DraftKind::Comment && entry.base.is_empty() {
            entry
                .draft
                .as_ref()
                .filter(|call| call.path == "editor/draft")
                .and_then(|call| serde_json::from_str::<serde_json::Value>(&call.body).ok())
                .and_then(|body| body.get("field").cloned())
                .and_then(|field| serde_json::from_value::<tc_api::editor::DraftField>(field).ok())
                .and_then(|field| match field {
                    tc_api::editor::DraftField::UserComment { target } => {
                        crate::comments::target(target).ok()
                    }
                    _ => None,
                })
                .filter(|target| place_of(&comment_place(target)) == place.as_str())
        } else {
            None
        };
        let candidate = Candidate {
            text: entry.text.clone(),
            at: entry.at.0,
            base: base_of(handle.as_ref(), kind, entry.base, comment_target.is_some()),
            sent: entry.sent.map(|sent| sent.text),
            run: Some(entry.instance),
            home: Home::Buffer(BufferSlot {
                key,
                instance: entry.instance,
                text: entry.text,
                at: entry.at,
            }),
        };
        fields
            .entry(identity(kind, record.as_ref(), &place))
            .or_insert(Field {
                kind,
                place,
                key: record,
                comment_target,
                candidates: Vec::new(),
            })
            .candidates
            .push(candidate);
    }
    let mut offers: Vec<(i64, DraftOffer)> = Vec::new();
    let mut settle = Vec::new();
    for (_, mut field) in fields {
        let record = FieldRecord {
            handle: handle.as_ref(),
            read: &read,
            kind: field.kind,
            key: field.key.clone(),
            comment_target: field.comment_target.as_ref(),
        };
        let (mut state, actions) = machine::next(
            FieldDraft::default(),
            Event::Offer {
                candidates: std::mem::take(&mut field.candidates),
                record: &record,
                mirror,
            },
        );
        for action in actions {
            match action {
                Action::Absorb {
                    text,
                    at,
                    over,
                    keep,
                    then,
                    older,
                } => {
                    let taken = absorb(handle.as_ref(), &read, &field, &over, &text, keep, at);
                    if let Some(id) = taken
                        && let Some(offer) = state.offers.iter_mut().find(|o| o.text == text)
                    {
                        machine::absorbed(offer, id);
                        settle.extend(then);
                        for copy in older {
                            handle.discard_draft(copy.0);
                        }
                    }
                }
                Action::Unstore(id) => {
                    handle.discard_draft(id.0);
                }
                // An open's plan writes nothing else.
                other => debug_assert!(false, "an open planned {other:?}"),
            }
        }
        settle.append(&mut state.settle);
        offers.extend(state.offers.into_iter().map(|offer| {
            (
                offer.at,
                DraftOffer {
                    kind: field.kind,
                    place: field.place.clone(),
                    settles: Settles {
                        stored: offer.stored().collect(),
                        buffered: offer.slots().cloned().collect(),
                    },
                    text: offer.text,
                    keep: offer.keep,
                    shown: offer.over.unwrap_or_default(),
                },
            )
        }));
    }
    offers.sort_by(|(a_at, a), (b_at, b)| {
        (a.place.as_str(), std::cmp::Reverse(*a_at))
            .cmp(&(b.place.as_str(), std::cmp::Reverse(*b_at)))
    });
    OfferedDrafts {
        offers: offers.into_iter().map(|(_, offer)| offer).collect(),
        settle,
    }
}

/// A stored draft of `drafts.json` as the machine's candidate, with its field.
fn stored_field(
    handle: &dyn ProjectHandle,
    draft: &OfferedDraft,
) -> (String, (Field, Candidate<Vec<ShownToken>>)) {
    let kind = kind_of_draft(&draft.field);
    let first = draft.keys().next().cloned();
    let place = field_place(&match (&first, &draft.field) {
        (_, DraftField::UserComment { target }) => place_of(&comment_place(target)),
        (Some(key), _) => place_of(key),
        (None, _) => String::new(),
    });
    // A token tc-api cannot carry is still offered, as text.
    let shown = draft.wire_tokens().unwrap_or_default();
    let comment_target = match &draft.field {
        DraftField::UserComment { target } => Some(target.clone()),
        _ => None,
    };
    let candidate = Candidate {
        text: draft.text.clone(),
        at: draft.at.0,
        base: base_of(handle, kind, shown, comment_target.is_some()),
        sent: None,
        run: draft.run,
        home: Home::Stored(StoredDraftId(draft.id)),
    };
    (
        identity(kind, first.as_ref(), &place),
        (
            Field {
                kind,
                place,
                key: first,
                comment_target,
                candidates: Vec::new(),
            },
            candidate,
        ),
    )
}

/// Writes an offered entry of `drafts.json` for a text the buffer held: its number, or `None`
/// when it could not be written, and the slots stay.
fn absorb(
    handle: &dyn ProjectHandle,
    read: &CountingSnapshot,
    field: &Field,
    over: &[ShownToken],
    text: &str,
    keep: Keep,
    at: i64,
) -> Option<StoredDraftId> {
    let tokens: Vec<DraftToken> = parse_tokens(handle, over)?
        .into_iter()
        .map(|(token, _)| token)
        .collect();
    let field = draft_field(field.kind, &tokens, read, field.comment_target.as_ref()).ok()?;
    handle
        .offer_draft(
            field,
            tokens,
            text.to_owned(),
            keep == Keep::Save,
            WallTime(at),
        )
        .map(StoredDraftId)
}

impl Handle<Drafts> for App {
    async fn handle(&self, req: Drafts) -> Result<OfferedDrafts, ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        Ok(offer(&session, &req.buffered, req.mirror))
    }
}

/// The store's field of a kept draft. A comment's target is read from the comment its token
/// names, as the store keeps a revision on it (STORAGE §5.5).
fn draft_field(
    kind: DraftKind,
    shown: &[DraftToken],
    read: &CountingSnapshot,
    comment_target: Option<&tc_journal::record::CommentTarget>,
) -> Result<DraftField, ApiError> {
    let unkept = |why: &str| ApiError::BadRequest {
        field: "kind".to_owned(),
        reason: format!("{kind:?}: {why}"),
    };
    Ok(match kind {
        DraftKind::Words => DraftField::Words,
        DraftKind::Layout => DraftField::Layout,
        DraftKind::Frame => DraftField::Frame,
        DraftKind::StoryRef => DraftField::StoryRef,
        DraftKind::CheckComment => DraftField::CheckComment,
        DraftKind::Alignment => DraftField::Alignment,
        DraftKind::Comment => {
            if shown.is_empty() {
                return comment_target
                    .cloned()
                    .map(|target| DraftField::UserComment { target })
                    .ok_or_else(|| unkept("a new comment needs its offered target"));
            }
            let key = shown
                .first()
                .ok_or_else(|| unkept("a comment needs its displayed record"))?
                .key();
            match read.projected(key) {
                Some(Record::Comment { value, .. }) => DraftField::UserComment {
                    target: value.target,
                },
                _ => return Err(unkept("the comment it revises is not in the project")),
            }
        }
        DraftKind::Section => return Err(unkept("a section has no Keep yet (B4b, R6)")),
    })
}

/// The one question a *Keep* or a *Discard* answers, as the machine holds it: the request
/// carries what the offer said, so the server keeps nothing between the two.
fn answered(
    text: &str,
    shown: &[ShownToken],
    stored: &[StoredDraftId],
) -> FieldDraft<Vec<ShownToken>> {
    let mut homes: Vec<Home> = stored.iter().copied().map(Home::Stored).collect();
    homes.dedup();
    FieldDraft {
        offers: vec![Offer {
            text: text.to_owned(),
            at: 0,
            // The store decides from the tokens whether Keep writes over them or with no
            // parents (R20).
            keep: Keep::Save,
            over: Some(shown.to_vec()),
            homes,
        }],
        ..FieldDraft::default()
    }
}

impl Handle<KeepDraft> for App {
    /// The offer's text, saved as its field's save over the offer's tokens (STORAGE §5.14),
    /// then the offer's entries of `drafts.json` gone, as the machine's *Keep* says. No lock
    /// is held across the wait: the write's lease keeps the session open.
    async fn handle(&self, req: KeepDraft) -> Result<KeptDraft, ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let (_, actions) = machine::next(
            answered(&req.text, &req.shown, &req.discard),
            Event::Keep(0),
        );
        let mut kept = None;
        for action in actions {
            match action {
                Action::Keep { text, over, .. } => {
                    kept = Some(
                        self.keep(&session, req.kind, req.request, &over, text, &req.discard)
                            .await?,
                    );
                }
                Action::Unstore(id) => {
                    handle.discard_draft(id.0);
                }
                other => debug_assert!(false, "Keep planned {other:?}"),
            }
        }
        kept.ok_or_else(|| ApiError::BadRequest {
            field: "shown".to_owned(),
            reason: "Keep names the record of its text".to_owned(),
        })
    }
}

impl App {
    async fn keep(
        &self,
        session: &Arc<ProjectSession>,
        kind: DraftKind,
        request: tc_api::RequestId,
        over: &[ShownToken],
        text: String,
        stored_ids: &[StoredDraftId],
    ) -> Result<KeptDraft, ApiError> {
        let handle = session.handle().clone();
        let shown = over
            .iter()
            .map(|token| DraftToken::parse(handle.as_ref(), token).map(|(token, _)| token))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApiError::NotSaved(NotSaved::BadToken))?;
        // A new comment has no record identity until it is committed. Its durable offer
        // supplies the typed target, never an arbitrary request-side place string.
        let target = if kind == DraftKind::Comment && shown.is_empty() {
            handle.offered_drafts().into_iter().find_map(|draft| {
                if !stored_ids.contains(&StoredDraftId(draft.id))
                    || draft.text != text
                    || draft.keys().next().is_some()
                {
                    return None;
                }
                match draft.field {
                    DraftField::UserComment { target } => Some(target),
                    _ => None,
                }
            })
        } else {
            None
        };
        let record = shown
            .first()
            .and_then(|token| wire::scope::record_ref(token.key()));
        let field = draft_field(kind, &shown, &session.read(), target.as_ref())?;
        // A story draft holds the text as typed; it is kept in the form a save stores.
        let line = match (kind, shown.first().map(DraftToken::key)) {
            (DraftKind::StoryRef, _) => Some(tc_api::story::StoryLine::Reference),
            (DraftKind::Frame, Some(RecordKey::Frame(_, tc_journal::FrameNo::TITLE))) => {
                Some(tc_api::story::StoryLine::Title)
            }
            (DraftKind::Frame, _) => Some(tc_api::story::StoryLine::Frame),
            _ => None,
        };
        let text = match line {
            Some(line) => tc_api::story::stored_text(line, &text),
            None if kind == DraftKind::Words => {
                // A verse draft holds the card's text (poem lines as tabs, tC4 #810): it goes
                // back to the words by the version it was typed over, as a save does (REF1b).
                let read = session.read();
                let opened = shown.first().and_then(|token| match token.heads() {
                    [head] => read.body(head).map(|body| body.record),
                    _ => read.projected(token.key()),
                });
                let opened = match opened {
                    Some(Record::Verse {
                        value: VerseValue::Text { words, .. },
                        ..
                    }) => words,
                    _ => String::new(),
                };
                tc_usfm::poem::words_from_editor_text(&text, &opened)
            }
            None => text,
        };
        let me = handle.facts().device;
        let first_comment = match (kind, over) {
            (DraftKind::CheckComment, [token]) => wire::token::edit_token(handle.as_ref(), token)
                .ok()
                .filter(|token| token.heads().is_empty()),
            _ => None,
        };
        let command = match first_comment {
            Some(token) => self.first_comment(session, token, text).await?,
            None => StoreCommand::keep_draft(field, shown, text),
        }
        .requested_by(request);
        let reply =
            handle.submit(command).await.map_err(|error| {
                match wire::error::command_error(error) {
                    Ok(api) => api,
                    Err(unworded) => self.internal(
                        "the store refused to keep a draft",
                        facts(json!({"reason": unworded.0})),
                    ),
                }
            })?;
        let read = session.read();
        let (stored, _) = wire::error::stored(&reply.stored);
        let record = record
            .or_else(|| {
                reply
                    .tokens
                    .iter()
                    .find_map(|(key, _)| wire::scope::record_ref(key))
            })
            .ok_or_else(|| ApiError::BadRequest {
                field: "shown".into(),
                reason: "the kept text has no record".into(),
            })?;
        let new_conflicts =
            wire::write::new_conflicts(&reply, &read, &record, me).map_err(|e| {
                self.internal(
                    "a kept draft's reply could not be written",
                    facts(json!({"reason": e.to_string()})),
                )
            })?;
        Ok(KeptDraft {
            view: wire::scope::view(reply.seq),
            stored,
            new_conflicts,
        })
    }
}

impl Handle<DiscardDraft> for App {
    /// The offer's entries of `drafts.json` go, each written before the answer, so a
    /// discarded text never comes back after a crash.
    async fn handle(&self, req: DiscardDraft) -> Result<(), ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let (_, actions) = machine::next(answered("", &[], &req.stored), Event::Discard(0));
        for action in actions {
            match action {
                Action::Unstore(id) => {
                    session.handle().discard_draft(id.0);
                }
                other => debug_assert!(false, "Discard planned {other:?}"),
            }
        }
        Ok(())
    }
}

/// The names a copy of `folder` tries, as the file managers name a copy: "Name - Copy", then
/// "Name - Copy (2)" and on.
fn copy_names(folder: &ProjectRef) -> impl Iterator<Item = ProjectRef> + '_ {
    (1..=99u32).filter_map(move |n| {
        let name = match n {
            1 => format!("{folder} - Copy"),
            n => format!("{folder} - Copy ({n})"),
        };
        ProjectRef::parse(&name).ok()
    })
}

impl Handle<CopyProject> for App {
    /// Copies the project to a new folder (R7's `Store::copy_project`): every save and the
    /// folder's `drafts.json` go with it, so the copy's first open offers them (§6.4, A-10).
    async fn handle(&self, req: CopyProject) -> Result<ProjectCopy, ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let folder = session.scope().project.clone();
        let id = session.id().clone();
        let store = self.sessions().store().clone();
        let copied = tokio::task::spawn_blocking(move || {
            for name in copy_names(&folder) {
                match store.copy(&folder, &name) {
                    Ok(()) => return Ok(name),
                    Err(CopyFailure::NameExists) => continue,
                    Err(CopyFailure::Unsafe(reason)) => {
                        return Err(CreateRefusal::CopyUnsafe { reason });
                    }
                    Err(CopyFailure::Failed(reason)) => {
                        return Err(CreateRefusal::Failed { reason });
                    }
                }
            }
            Err(CreateRefusal::Failed {
                reason: "every copy name is taken".to_owned(),
            })
        })
        .await
        .unwrap_or_else(|e| {
            Err(CreateRefusal::Failed {
                reason: e.to_string(),
            })
        });
        match copied {
            Ok(project) => Ok(ProjectCopy { project, id }),
            Err(refusal) => Err(ApiError::Refused(Refusal::Create(refusal))),
        }
    }
}

impl Handle<OtherComputerClosed> for App {
    /// The person said that the computer the project's lock names is closed (STORAGE §5.0):
    /// the next open of the folder takes the lock and opens writable. The UI goes Home and
    /// opens the project again, so the open runs with its progress as every open does (B3).
    async fn handle(&self, req: OtherComputerClosed) -> Result<(), ApiError> {
        self.sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        self.sessions().store().trust_lock(&req.at.project);
        Ok(())
    }
}
