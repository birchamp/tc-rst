//! Translate (unit B4a): the translation's text of the open book, every chapter in one read,
//! so a chapter switch reads nothing of the translation (AD16). Each verse comes with the
//! words its box edits and their token (AD37), and the paragraph marks the grid draws, by the
//! rules the source panes follow ([`crate::sources`]). C2 writes use the displayed tokens and acknowledge only the store's result.

use crate::backend::{EditToken, ProjectHandle, StoreCommand};
use crate::session::ProjectSession;
use serde_json::json;
use tc_api::ApiError;
use tc_api::editor::{
    ChapterText, Chapters, ChaptersView, DiscardDraft, Draft, DraftField, SaveLayout, SaveVerse,
    VerseRow,
};
use tc_api::ids::WallMs;
use tc_api::ids::{ConflictRef, RefPart};
use tc_api::model::section::ParaFormat;
use tc_api::shown::{ConflictMarker, Shown};
use tc_api::write::{Ack, NotSaved};
use tc_journal::fold::Resolved;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use crate::app::App;
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::sources::{is_para_mark, level};
use crate::wire;

/// The highest order of a verse conflict kind (`ConflictKind::is_verse`: verse text, verse
/// structure, a passage, a structure issue).
const LAST_VERSE_KIND: u8 = 3;

/// One verse of a chapter as the store holds it.
pub(crate) struct Text {
    key: RecordKey,
    pub(crate) verse: u16,
    pub(crate) last: u16,
    pub(crate) words: String,
    pub(crate) layout: String,
    /// The records of the verses a span covers, in order.
    covered: Vec<RecordKey>,
}

/// The paragraph marks of a verse's content: the levels of those before its first text and
/// after its last, in order.
fn marks(body: &str) -> (Vec<u8>, Vec<u8>) {
    let (lead, tail) = tc_usfm::edge_markers(body);
    let levels = |names: Vec<&str>| {
        names
            .into_iter()
            .filter(|m| is_para_mark(m))
            .map(level)
            .collect::<Vec<u8>>()
    };
    (levels(lead), levels(tail))
}

pub(crate) fn texts(read: &CountingSnapshot, book: tc_core::BookId, chapter: u16) -> Vec<Text> {
    let mut texts: Vec<Text> = Vec::new();
    // The verses a span covers, with the verse that covers them.
    let mut covered: Vec<(u16, u16, RecordKey)> = Vec::new();
    for key in read.chapter_keys(book, chapter).verses {
        match read.projected(&key) {
            Some(Record::Verse {
                verse,
                value:
                    VerseValue::Text {
                        words,
                        layout,
                        last,
                        ..
                    },
                ..
            }) => texts.push(Text {
                key,
                verse: verse.get(),
                last: last.get().max(verse.get()),
                words,
                layout,
                covered: Vec::new(),
            }),
            Some(Record::Verse {
                verse,
                value: VerseValue::Covered { by },
                ..
            }) => covered.push((by.get(), verse.get(), key)),
            _ => {}
        }
    }
    texts.sort_unstable_by_key(|t| t.verse);
    covered.sort_unstable_by_key(|(_, verse, _)| *verse);
    for (by, _, key) in covered {
        if let Some(text) = texts.iter_mut().find(|t| t.verse == by) {
            text.covered.push(key);
        }
    }
    texts
}

/// The conflict a verse is in, with how many versions it has.
fn marker(read: &CountingSnapshot, key: &RecordKey) -> Option<ConflictMarker> {
    let state = read.key_state(key)?;
    let Resolved::Conflict(id) = state.state else {
        return None;
    };
    Some(ConflictMarker {
        conflict: ConflictRef::parse(&id.to_hex()).ok()?,
        versions: u8::try_from(state.heads.len()).unwrap_or(u8::MAX),
    })
}

impl App {
    fn chapter_text(
        &self,
        read: &CountingSnapshot,
        book: tc_core::BookId,
        chapter: u16,
    ) -> Result<ChapterText, ApiError> {
        let mut sections = Vec::new();
        let mut verses = Vec::new();
        // The marks after the previous verse's text, and whether a `\ts\*` came in it.
        let mut trailing: Vec<u8> = Vec::new();
        let mut section_mark = false;
        // The `\p`, `\q1` or `\q2` line the previous verse's layout ends with.
        let mut gap: Option<ParaFormat> = None;
        for text in texts(read, book, chapter) {
            let body = format!("{}{}", text.words, text.layout);
            let (lead, tail) = marks(&body);
            let label = if text.last > text.verse {
                format!("{}-{}", text.verse, text.last)
            } else {
                text.verse.to_string()
            };
            let key = RefPart::parse(&label).map_err(|e| {
                self.internal(
                    "a verse's number could not be written",
                    crate::reports::facts(
                        serde_json::json!({"verse": label, "reason": e.to_string()}),
                    ),
                )
            })?;
            if verses.is_empty() || section_mark {
                sections.push(text.verse);
            }
            let minted = read
                .mint(&text.key, None)
                .map_err(|e| e.to_string())
                .and_then(|t| wire::token::shown(&t).map_err(|e| e.to_string()))
                .map_err(|reason| {
                    self.internal(
                        "a verse's token could not be made",
                        crate::reports::facts(
                            serde_json::json!({"verse": label, "reason": reason}),
                        ),
                    )
                })?;
            let record = wire::scope::record_ref(&text.key).ok_or_else(|| {
                self.internal(
                    "a verse's record could not be named",
                    crate::reports::facts(serde_json::json!({"verse": label})),
                )
            })?;
            let drafted = crate::drafted::words_drafted(&text.words);
            let covered = text
                .covered
                .iter()
                .map(|key| {
                    read.mint(key, None)
                        .map_err(|e| e.to_string())
                        .and_then(|t| wire::token::shown(&t).map_err(|e| e.to_string()))
                        .map_err(|reason| {
                            self.internal(
                                "a covered verse's token could not be made",
                                crate::reports::facts(
                                    serde_json::json!({"verse": label, "reason": reason}),
                                ),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            verses.push(VerseRow {
                key,
                record,
                drafted,
                text: if drafted {
                    tc_usfm::display_text(&text.words)
                } else {
                    String::new()
                },
                lead: lead.first().copied(),
                before: trailing.last().copied(),
                words: Shown {
                    marker: marker(read, &text.key),
                    value: text.words,
                    token: minted,
                },
                format: gap,
                covered,
            });
            gap = crate::section::gap_marker(&text.layout);
            trailing = tail;
            section_mark = body.contains("\\ts\\*");
        }
        Ok(ChapterText {
            chapter,
            sections,
            verses,
        })
    }
}

impl Handle<Chapters> for App {
    async fn handle(&self, req: Chapters) -> Result<ChaptersView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let numbers = read.chapters_of(req.book);
        if numbers.is_empty() {
            return Err(ApiError::BadRequest {
                field: "book".to_owned(),
                reason: format!("{} is not in the project", req.book.code()),
            });
        }
        let chapters = numbers
            .into_iter()
            .map(|chapter| self.chapter_text(&read, req.book, chapter))
            .collect::<Result<Vec<_>, _>>()?;
        let conflicts = read
            .conflict_counts()
            .iter()
            .filter(|((_, kind), _)| *kind <= LAST_VERSE_KIND)
            .map(|(_, n)| *n)
            .sum();
        Ok(ChaptersView {
            view: wire::scope::view(read.seq()),
            chapters,
            conflicts,
        })
    }
}

impl Handle<SaveVerse> for App {
    /// The words over the verse the token shows, through the screen's session and the store,
    /// announced as this request's own save (AD37). No lock is held across the wait: the
    /// write's lease keeps the session open.
    async fn handle(&self, req: SaveVerse) -> Result<Ack, ApiError> {
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let shown = wire::token::edit_token(handle.as_ref(), &req.shown)?;
        let me = handle.facts().device;
        let command =
            StoreCommand::save_verse(shown.clone(), req.words.clone()).requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => {
                self.record_translate_edit(&session, &shown, &req.words, self.inner.clock.now());
                wire::write::ack(&reply, &session.read(), &shown, &req.words, me).map_err(|e| {
                    self.internal(
                        "a save's reply could not be written",
                        crate::reports::facts(json!({"reason": e.to_string()})),
                    )
                })
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(unworded) => self.internal(
                    "the store refused a save no screen has words for yet",
                    crate::reports::facts(json!({"reason": unworded.0})),
                ),
            }),
        }
    }
}

impl Handle<SaveLayout> for App {
    async fn handle(&self, req: SaveLayout) -> Result<Ack, ApiError> {
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let shown = wire::token::edit_token(handle.as_ref(), &req.shown)?;
        let me = handle.facts().device;
        let command =
            StoreCommand::save_layout(shown.clone(), req.layout.clone()).requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => {
                let read = session.read();
                let words = match read.projected(shown.key()) {
                    Some(Record::Verse {
                        value: VerseValue::Text { words, .. },
                        ..
                    }) => words,
                    _ => String::new(),
                };
                self.record_translate_edit(&session, &shown, &words, self.inner.clock.now());
                wire::write::layout_ack(&reply, &read, &shown, &req.layout, me).map_err(|error| {
                    self.internal(
                        "a layout save's reply could not be written",
                        crate::reports::facts(json!({"reason": error.to_string()})),
                    )
                })
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(error) => self.internal(
                    "the store refused a layout save no screen has words for yet",
                    crate::reports::facts(json!({"reason": error.0})),
                ),
            }),
        }
    }
}

impl App {
    /// The token identifies where typing happened. The caller supplies the keystroke's
    /// original timestamp for a draft, or the completed store time for a confirmed save.
    pub(crate) fn record_translate_edit(
        &self,
        session: &ProjectSession,
        shown: &EditToken,
        text: &str,
        at: WallMs,
    ) {
        if let RecordKey::Frame(story, _) | RecordKey::StoryRef(story) = shown.key() {
            let frame = match shown.key() {
                RecordKey::Frame(_, frame) => frame.0,
                _ => 0,
            };
            self.record_story_edit(session, *story, frame, text, at);
            return;
        }
        let (book, chapter, verse) = match shown.key() {
            RecordKey::Verse(book, chapter, verse) => {
                (*book, chapter.get(), verse.get().to_string())
            }
            RecordKey::ChapterFront(book, chapter) => (*book, chapter.get(), "front".to_owned()),
            _ => return,
        };
        let Ok(verse) = RefPart::parse(&verse) else {
            return;
        };
        let folder = session.scope().project.clone();
        // A Resume preview is bounded by this field, never by the project or its history.
        let snippet = tc_usfm::plain_text_v1(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect();
        self.inner.prefs.record_last_used(folder.clone(), at);
        self.inner.prefs.record_last_edit(
            session.id().clone(),
            crate::prefs::LastEdit {
                project: folder,
                unit: crate::prefs::EditUnit::Book {
                    book,
                    chapter,
                    verse,
                },
                mode: tc_api::prefs::PlaceMode::Draft,
                tool: None,
                snippet,
                at,
            },
        );
    }
}

impl Handle<Draft> for App {
    async fn handle(&self, req: Draft) -> Result<(), ApiError> {
        if req.instance != self.instance() {
            return Err(ApiError::BadRequest {
                field: "instance".into(),
                reason: "a draft of another server run must be offered through session/drafts"
                    .into(),
            });
        }
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let (field, shown) = draft_target(handle.as_ref(), &session.read(), req.field, &req.shown)?;
        let first = shown.first().cloned();
        let command = StoreCommand::draft(
            field.clone(),
            shown,
            req.text.clone(),
            tc_journal::WallTime(req.changed.0),
        )
        .requested_by(req.request);
        handle
            .submit(command)
            .await
            .map_err(|error| match wire::error::command_error(error) {
                Ok(api) => api,
                Err(error) => self.internal(
                    "the store refused an editor draft no screen has words for yet",
                    crate::reports::facts(json!({"reason": error.0})),
                ),
            })?;
        match &field {
            tc_journal::command::DraftField::UserComment { target } => {
                self.record_comment_edit(&session, target, &req.text, req.changed)
            }
            _ => {
                if let Some(first) = first {
                    self.record_translate_edit(&session, &first, &req.text, req.changed);
                }
            }
        }
        Ok(())
    }
}

/// Every draft validates its displayed parents; a new user comment has none.
fn draft_target(
    handle: &dyn ProjectHandle,
    read: &CountingSnapshot,
    kind: DraftField,
    over: &[tc_api::ShownToken],
) -> Result<(tc_journal::command::DraftField, Vec<EditToken>), ApiError> {
    let shown = over
        .iter()
        .map(|token| wire::token::edit_token(handle, token))
        .collect::<Result<Vec<_>, _>>()?;
    if let DraftField::UserComment { target } = kind {
        let target = crate::comments::target(target)?;
        if shown.len() > 1
            || shown
                .first()
                .is_some_and(|token| !crate::comments::matches_target(read, token, &target))
        {
            return Err(ApiError::NotSaved(NotSaved::BadToken));
        }
        return Ok((
            tc_journal::command::DraftField::UserComment { target },
            shown,
        ));
    }
    let first = shown
        .first()
        .ok_or(ApiError::NotSaved(NotSaved::BadToken))?;
    let field = match kind {
        DraftField::Frame | DraftField::StoryRef => {
            if shown.len() != 1
                || !matches!(
                    (kind, first.key()),
                    (DraftField::Frame, RecordKey::Frame(..))
                        | (DraftField::StoryRef, RecordKey::StoryRef(_))
                )
            {
                return Err(ApiError::NotSaved(NotSaved::BadToken));
            }
            if matches!(kind, DraftField::Frame) {
                tc_journal::command::DraftField::Frame
            } else {
                tc_journal::command::DraftField::StoryRef
            }
        }
        DraftField::UserComment { .. } => return Err(ApiError::NotSaved(NotSaved::BadToken)),
        DraftField::Words | DraftField::Layout => {
            if shown.len() != 1 || !matches!(first.key(), RecordKey::Verse(..)) {
                return Err(ApiError::NotSaved(NotSaved::BadToken));
            }
            match kind {
                DraftField::Words => tc_journal::command::DraftField::Words,
                _ => tc_journal::command::DraftField::Layout,
            }
        }
        DraftField::Section { book } => {
            let mut unique = std::collections::BTreeSet::new();
            let chapter = match first.key() {
                RecordKey::Verse(of, chapter, _) | RecordKey::ChapterFront(of, chapter)
                    if *of == book =>
                {
                    *chapter
                }
                _ => return Err(ApiError::NotSaved(NotSaved::BadToken)),
            };
            if shown.iter().any(|token| {
                    !unique.insert(token.key().clone()) || !matches!(token.key(),
                        RecordKey::Verse(of, at, _) | RecordKey::ChapterFront(of, at) if *of == book && *at == chapter)
                }) { return Err(ApiError::NotSaved(NotSaved::BadToken)); }
            tc_journal::command::DraftField::Section { book }
        }
        // The decision's token: the comment is a field of the decision record (STORAGE §5.4).
        DraftField::CheckComment => {
            if shown.len() != 1 || !matches!(first.key(), RecordKey::Check(_)) {
                return Err(ApiError::NotSaved(NotSaved::BadToken));
            }
            tc_journal::command::DraftField::CheckComment
        }
    };
    Ok((field, shown))
}

impl Handle<DiscardDraft> for App {
    async fn handle(&self, req: DiscardDraft) -> Result<(), ApiError> {
        if req.instance != self.instance() {
            return Err(ApiError::BadRequest {
                field: "instance".into(),
                reason: "Cancel cannot discard another server run's draft".into(),
            });
        }
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        if let tc_api::event::SessionMode::ReadOnly(reason) = handle.facts().mode {
            return Err(ApiError::NotSaved(NotSaved::ReadOnly(reason)));
        }
        let (field, shown) = draft_target(handle.as_ref(), &session.read(), req.field, &req.shown)?;
        let discarded = tokio::task::spawn_blocking(move || {
            handle.discard_current_draft(
                field,
                shown,
                req.text,
                tc_journal::WallTime(req.changed.0),
            )
        })
        .await
        .map_err(|error| {
            self.internal(
                "the editor draft's Cancel stopped",
                crate::reports::facts(json!({"reason": error.to_string()})),
            )
        })?;
        if !discarded {
            return Err(ApiError::NotSaved(NotSaved::Disk(
                "the unsaved draft could not be discarded".into(),
            )));
        }
        Ok(())
    }
}
