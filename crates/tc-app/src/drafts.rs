//! The draft offer at an open (APP-DESIGN §6.6, C1b): `session/drafts`, `session/keepDraft`,
//! `session/discardDraft`, and the copy offer of a read-only project, `session/copy` (§6.4).
//!
//! Unsaved text of an earlier run is in up to three places: the project's `.tc/drafts.json`
//! (the store offers the drafts whose text differs from what their records hold, O12), the
//! page's draft buffer and the shell's mirror (the UI sends those as `buffered`). [`offer`]
//! merges them:
//!
//! - Only entries of this folder and project id count (A-9), and never a test field.
//! - An entry whose text the record holds now, or whose text is the value it was editing,
//!   holds nothing unsaved: it is `saved`, and the UI removes it.
//! - A stored draft gives way to a buffered entry of its field at least as new that edits the
//!   same version (the same tokens): the buffer takes every keystroke, `drafts.json` at most
//!   one write a second, so it is the older copy of the same typing. Otherwise every distinct
//!   text of a field is offered, the newest first: two runs' entries of one field (a restart
//!   behind a reused port, §6.6) are two texts, and so is a draft of a folder copied from
//!   another computer.
//! - An entry whose `sent` text the record holds now is kept over the record's own token: the
//!   save it waited for landed before the crash, and the newer text continues from it (A4b).
//!   Any other is kept over its own tokens, so a record changed since becomes a conflict.
//! - A token whose MAC fails is offered as text, kept as a separate version (R20).
//!
//! A read-only project offers nothing (STORAGE §5.14): nothing can be kept there, and the
//! entries wait for the copy or a writable open.

use std::collections::BTreeMap;

use serde_json::json;
use tc_api::buffer::{BufferEntry, BufferKey, BufferLine, FieldKind, FieldPlace};
use tc_api::event::SessionMode;
use tc_api::ids::ProjectRef;
use tc_api::refusal::CreateRefusal;
use tc_api::session::{
    BufferSlot, CopyProject, DiscardDraft, DraftKind, DraftOffer, Drafts, Keep, KeepDraft,
    KeptDraft, OfferedDrafts, OtherComputerClosed, ProjectCopy, Settles, StoredDraftId,
};
use tc_api::{ApiError, NotSaved, Refusal, ShownToken};
use tc_journal::command::DraftField;
use tc_journal::keys::CheckTarget;
use tc_journal::record::{CheckValue, VerseValue};
use tc_journal::{Record, RecordKey};
use unicode_normalization::UnicodeNormalization;

use crate::app::App;
use crate::backend::{CopyFailure, DraftToken, OfferedDraft, ProjectHandle, StoreCommand};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::reports::facts;
use crate::session::ProjectSession;
use crate::wire;

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

fn same_text(a: &str, b: &str) -> bool {
    nfc(a) == nfc(b)
}

/// One unsaved text before the merge.
struct Candidate {
    /// The field: its kind and record, or its place for a field with no token.
    identity: String,
    kind: DraftKind,
    place: FieldPlace,
    text: String,
    /// Milliseconds since the Unix epoch.
    at: i64,
    keep: Keep,
    shown: Vec<ShownToken>,
    /// The tokens of the version the editor showed, as the editor held them.
    base: Vec<ShownToken>,
    stored: Option<StoredDraftId>,
    slot: Option<BufferSlot>,
}

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
        ) => Some(words),
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
        RecordKey::Frame(story, frame) => format!("{:02}-{:02}", story.get(), frame.0),
        RecordKey::StoryRef(story) => format!("{:02}", story.get()),
        RecordKey::Check(check) => match &check.target {
            CheckTarget::Verse { book, .. } => {
                let (chapter, verse) = check.target.parts();
                format!("{} {chapter}:{verse}", book.code())
            }
            CheckTarget::Frame { story, frame } => format!("{:02}-{:02}", story.get(), frame.0),
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

/// A stored draft of `drafts.json`, as the store offers it.
fn stored_candidate(draft: &OfferedDraft) -> Candidate {
    let kind = kind_of_draft(&draft.field);
    let first = draft.keys().next().cloned();
    let place = field_place(&match (&first, &draft.field) {
        (Some(key), _) => place_of(key),
        (None, DraftField::UserComment { target }) => place_of(&comment_place(target)),
        (None, _) => String::new(),
    });
    let shown = draft.wire_tokens().unwrap_or_default();
    let keep = if shown.is_empty() && draft.keys().next().is_some() {
        // A token tc-api cannot carry: the text is still offered.
        Keep::CopyText
    } else {
        keep_for(kind, first.is_some(), draft.verified)
    };
    Candidate {
        identity: identity(kind, first.as_ref(), &place),
        kind,
        place,
        text: draft.text.clone(),
        at: draft.at.0,
        keep,
        base: shown.clone(),
        shown,
        stored: Some(StoredDraftId(draft.id)),
        slot: None,
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

/// What *Keep* can do for a field: a section's save and a new comment's need the units that
/// build their editors (B4b with R6, C3 with R15), so until then the offer is "Copy text".
fn keep_for(kind: DraftKind, has_token: bool, verified: bool) -> Keep {
    match (kind, has_token) {
        (DraftKind::Section, _) | (DraftKind::Comment, false) => Keep::CopyText,
        (_, false) => Keep::CopyText,
        (_, true) if verified => Keep::Save,
        (_, true) => Keep::AsVersion,
    }
}

/// What one buffered entry offers: `Ok(None)` when it holds nothing unsaved, `Err(slot)`
/// when its text is already stored.
fn buffered_candidate(
    handle: &dyn ProjectHandle,
    read: &CountingSnapshot,
    key: BufferKey,
    entry: BufferEntry,
) -> Result<Option<Candidate>, BufferSlot> {
    let slot = BufferSlot {
        key: key.clone(),
        instance: entry.instance,
        text: entry.text.clone(),
    };
    let Some(kind) = kind_of_field(key.field.kind) else {
        return Ok(None);
    };
    let place = key.field.place.clone();
    let parsed: Option<Vec<(DraftToken, bool)>> = entry
        .base
        .iter()
        .map(|token| DraftToken::parse(handle, token).ok())
        .collect();
    let first = parsed
        .as_ref()
        .and_then(|tokens| tokens.first())
        .map(|(token, _)| token.key().clone());
    let verified = parsed
        .as_ref()
        .is_some_and(|tokens| tokens.iter().all(|(_, verified)| *verified));
    let current = first
        .as_ref()
        .and_then(|key| field_text(kind, read.projected(key)));
    if current
        .as_deref()
        .is_some_and(|now| same_text(now, &entry.text))
    {
        return Err(slot);
    }
    // The value the person started from: an entry back at it was typed and undone.
    let base = parsed
        .as_ref()
        .filter(|_| verified)
        .and_then(|tokens| tokens.first())
        .and_then(|(token, _)| token.heads().first().copied())
        .and_then(|head| read.body(&head))
        .and_then(|body| field_text(kind, Some(body.record)));
    if base
        .as_deref()
        .is_some_and(|then| same_text(then, &entry.text))
    {
        return Err(slot);
    }
    let mut shown = match &parsed {
        Some(_) => entry.base.clone(),
        None => Vec::new(),
    };
    let landed = verified
        && entry.sent.as_ref().is_some_and(|sent| {
            current
                .as_deref()
                .is_some_and(|now| same_text(now, &sent.text))
        });
    if landed
        && let Some(key) = &first
        && let Some(token) = read
            .mint(key, None)
            .ok()
            .and_then(|token| wire::token::shown(&token).ok())
        && let Some(slot) = shown.first_mut()
    {
        *slot = token;
    }
    let keep = if parsed.is_none() {
        Keep::CopyText
    } else {
        keep_for(kind, first.is_some(), verified)
    };
    Ok(Some(Candidate {
        identity: identity(kind, first.as_ref(), &place),
        kind,
        place,
        text: entry.text,
        at: entry.at.0,
        keep,
        shown,
        base: entry.base,
        stored: None,
        slot: Some(slot),
    }))
}

/// The offer of `session` for the buffered lines the UI sent (see the module documentation).
pub fn offer(session: &ProjectSession, buffered: &[String]) -> OfferedDrafts {
    let handle = session.handle();
    if matches!(handle.facts().mode, SessionMode::ReadOnly(_)) {
        return OfferedDrafts {
            offers: Vec::new(),
            saved: Vec::new(),
        };
    }
    let read = session.read();
    let mut saved = Vec::new();
    let mut groups: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for draft in handle.offered_drafts() {
        let candidate = stored_candidate(&draft);
        groups
            .entry(candidate.identity.clone())
            .or_default()
            .push(candidate);
    }
    for line in buffered {
        let Ok(BufferLine::Changed { key, entry }) = serde_json::from_str::<BufferLine>(line)
        else {
            continue;
        };
        if key.project != session.scope().project || key.id != *session.id() {
            continue;
        }
        match buffered_candidate(handle.as_ref(), &read, key, entry) {
            Ok(Some(candidate)) => groups
                .entry(candidate.identity.clone())
                .or_default()
                .push(candidate),
            Ok(None) => {}
            Err(slot) => saved.push(slot),
        }
    }
    let mut offers: Vec<(i64, DraftOffer)> = Vec::new();
    for (_, mut group) in groups {
        // A stored draft over the very version a buffered entry of the field edits, and no
        // newer than it, is the older copy of that typing. One over another version (a folder
        // copied from another computer, an edit begun elsewhere) is other typing and is offered.
        let lagging: Vec<StoredDraftId> = group
            .iter()
            .filter(|stored| {
                stored.slot.is_none()
                    && group.iter().any(|buffered| {
                        buffered.slot.is_some()
                            && buffered.at >= stored.at
                            && buffered.base == stored.base
                    })
            })
            .filter_map(|c| c.stored)
            .collect();
        group.retain(|c| !(c.stored.is_some_and(|id| lagging.contains(&id))));
        group.sort_by_key(|c| std::cmp::Reverse(c.at));
        let mut kept: Vec<(i64, DraftOffer)> = Vec::new();
        for candidate in group {
            let settles = Settles {
                stored: candidate.stored.into_iter().collect(),
                buffered: candidate.slot.into_iter().collect(),
            };
            match kept
                .iter_mut()
                .find(|(_, offer)| same_text(&offer.text, &candidate.text))
            {
                Some((_, offer)) => {
                    offer.settles.stored.extend(settles.stored);
                    offer.settles.buffered.extend(settles.buffered);
                }
                None => kept.push((
                    candidate.at,
                    DraftOffer {
                        kind: candidate.kind,
                        place: candidate.place,
                        text: candidate.text,
                        keep: candidate.keep,
                        shown: candidate.shown,
                        settles,
                    },
                )),
            }
        }
        if let Some((_, newest)) = kept.first_mut() {
            newest.settles.stored.extend(lagging);
        }
        offers.extend(kept);
    }
    offers
        .sort_by(|(a_at, a), (b_at, b)| (a.place.as_str(), *a_at).cmp(&(b.place.as_str(), *b_at)));
    OfferedDrafts {
        offers: offers.into_iter().map(|(_, offer)| offer).collect(),
        saved,
    }
}

impl Handle<Drafts> for App {
    async fn handle(&self, req: Drafts) -> Result<OfferedDrafts, ApiError> {
        let session = self.read(&req.at)?;
        Ok(offer(&session, &req.buffered))
    }
}

/// The store's field of a kept draft. A comment's target is read from the comment its token
/// names, as the store keeps a revision on it (STORAGE §5.5).
fn draft_field(
    kind: DraftKind,
    shown: &[DraftToken],
    read: &CountingSnapshot,
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
            let key = shown
                .first()
                .ok_or_else(|| unkept("a new comment has no Keep yet (C3)"))?
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

impl Handle<KeepDraft> for App {
    /// The offer's text, saved as its field's save over the offer's tokens (STORAGE §5.14),
    /// then the offer's other stored drafts discarded. No lock is held across the wait: the
    /// write's lease keeps the session open.
    async fn handle(&self, req: KeepDraft) -> Result<KeptDraft, ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let shown = req
            .shown
            .iter()
            .map(|token| DraftToken::parse(handle.as_ref(), token).map(|(token, _)| token))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApiError::NotSaved(NotSaved::BadToken))?;
        let record = shown
            .first()
            .and_then(|token| wire::scope::record_ref(token.key()))
            .ok_or_else(|| ApiError::BadRequest {
                field: "shown".to_owned(),
                reason: "Keep names the record of its text".to_owned(),
            })?;
        let field = draft_field(req.kind, &shown, &session.read())?;
        let me = handle.facts().device;
        let command = StoreCommand::keep_draft(field, shown, req.text).requested_by(req.request);
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
        for id in req.discard {
            handle.discard_draft(id.0);
        }
        let read = session.read();
        let (stored, _) = wire::error::stored(&reply.stored);
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
    async fn handle(&self, req: DiscardDraft) -> Result<(), ApiError> {
        let session = self
            .sessions()
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        for id in req.stored {
            session.handle().discard_draft(id.0);
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
                    Err(CopyFailure::Failed(reason)) => return Err(reason),
                }
            }
            Err("every copy name is taken".to_owned())
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|copied| copied);
        match copied {
            Ok(project) => Ok(ProjectCopy { project, id }),
            Err(reason) => Err(ApiError::Refused(Refusal::Create(CreateRefusal::Failed {
                reason,
            }))),
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
