//! Translate's actions (units B4a and B4b): tC4's `startVerse`, `startSection` and `blurVerse`,
//! the cards' lock (`Editing::Verse`, `Editing::Section`), and the writers of their fields. A
//! verse card's field is a save-machine field of kind `Verse` (6.5): it saves on blur or 2 s
//! after the last keystroke. A section card's is of kind `Section`: it saves only by its Save
//! button (A-1), and Cancel discards it.
//!
//! The production writers use the same save machine as the editor and restored buffers:
//! request ids and sent text stay unchanged across retries, and server drafts carry the
//! buffer entry's own timestamp and displayed tokens.

use leptos::prelude::*;

use super::app_ctx::Actions;
use crate::app::shell::Editing;
use crate::app::translate::{section_key, verse_key};

impl Actions {
    pub fn start_story_unit(&self, key: tc_api::buffer::BufferKey) {
        let Some(project) = self.s().project.get_untracked() else {
            return;
        };
        if self.runtime().saves().write_cap(&project.at).is_some() {
            self.s().editing.try_set(Editing::Frame(key));
        }
    }

    pub fn close_story_unit(&self) {
        self.s().editing.try_set(Editing::None);
    }

    /// A drafted verse or its pill (tC4 `startVerse`): the verse's card opens, when the open
    /// is writable (6.4).
    pub fn start_verse(&self, chapter: u16, verse: &str) {
        let s = self.s();
        let (Some(project), Some(book)) = (s.project.get_untracked(), s.u.book.get_untracked())
        else {
            return;
        };
        if self.runtime().saves().write_cap(&project.at).is_none() {
            return;
        }
        if let Some(key) = verse_key(&project.at.project, &project.id, book, chapter, verse) {
            s.editing.try_set(Editing::Verse(key));
        }
    }

    /// The text of verse `chapter`:`verse` while its save has not landed, for the grid to show
    /// in place of the stored text once its card closed (INV-NEVER-HIDE-UNSAVED-TEXT). Tracks
    /// every run of the save machine.
    pub fn unsaved_verse(
        &self,
        book: tc_core::BookId,
        chapter: u16,
        verse: &str,
    ) -> Option<String> {
        let s = self.s();
        let saves = self.runtime().saves();
        saves.changes().track();
        let project = s.project.get_untracked()?;
        let key = verse_key(&project.at.project, &project.id, book, chapter, verse)?;
        saves.unsaved_text(&key)
    }

    /// The card closes (tC4 `blurVerse`): its field saves as on a blur when the card goes.
    pub fn close_verse(&self) {
        self.s().editing.try_set(Editing::None);
    }

    /// A section's "Draft section" button (tC4 `startSection`): its card opens over `keys`, when
    /// the open is writable (6.4). Opening writes nothing.
    pub fn start_section(&self, chapter: u16, keys: &[String]) {
        let s = self.s();
        let (Some(project), Some(book)) = (s.project.get_untracked(), s.u.book.get_untracked())
        else {
            return;
        };
        if self.runtime().saves().write_cap(&project.at).is_none() {
            return;
        }
        if let Some(key) = section_key(&project.at.project, &project.id, book, chapter, keys) {
            s.editing.try_set(Editing::Section {
                key,
                keys: keys.to_vec(),
            });
        }
    }

    /// The section card closes: after its Save stored the text, or after Cancel discarded it.
    pub fn close_section(&self) {
        self.s().editing.try_set(Editing::None);
    }
}

mod writer {
    use std::rc::Rc;

    use tc_api::buffer::{BufferEntry, BufferKey, DraftCall, FieldKind, SentSave};
    use tc_api::editor::{AddComment, Draft, DraftField, SaveLayout, SaveSection, SaveVerse};
    use tc_api::ids::Scope;
    use tc_api::token::ShownToken;
    use tc_api::write::Ack;

    use crate::app::client::{CallError, Transport};
    use crate::app::translate::{comment_target, parse_verse_place};
    use crate::app::web::{Api, FieldWriter, LocalBoxFuture, Saves};

    /// Verse words and layout share timing and buffer semantics, with distinct typed saves.
    struct VerseWriter;
    struct LayoutWriter;
    struct CommentWriter;
    struct StoryWriter;

    impl FieldWriter for CommentWriter {
        fn save(
            &self,
            api: &Api,
            key: &BufferKey,
            sent: &SentSave,
            base: &[ShownToken],
        ) -> LocalBoxFuture<Result<Ack, CallError>> {
            let api = api.clone();
            let request = sent
                .scope
                .clone()
                .zip(comment_target(key.field.place.as_str()))
                .map(|(at, target)| AddComment {
                    at,
                    request: sent.request,
                    target,
                    shown: base.first().cloned(),
                    text: sent.text.clone(),
                });
            Box::pin(async move {
                match request {
                    Some(request) => api.call(&request).await,
                    None => Err(CallError::Transport(Transport::Unreadable(
                        "a comment save with no open or target".to_owned(),
                    ))),
                }
            })
        }

        fn draft_request(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<Draft> {
            draft_request(key, scope, entry)
        }

        fn draft_call_entry(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<DraftCall> {
            draft_call(key, scope, entry)
        }
    }

    impl FieldWriter for StoryWriter {
        fn save(
            &self,
            api: &Api,
            key: &BufferKey,
            sent: &SentSave,
            base: &[ShownToken],
        ) -> LocalBoxFuture<Result<Ack, CallError>> {
            let api = api.clone();
            let context = sent.scope.clone().zip(base.first().cloned());
            let request = sent.request;
            let text = sent.text.clone();
            let reference = key.field.kind == FieldKind::StoryRef;
            Box::pin(async move {
                let Some((at, shown)) = context else {
                    return Err(CallError::Transport(Transport::Unreadable(
                        "a story save with no open or token".to_owned(),
                    )));
                };
                if reference {
                    api.call(&tc_api::story::SaveRef {
                        at,
                        request,
                        shown,
                        text,
                    })
                    .await
                } else {
                    api.call(&tc_api::story::SaveFrame {
                        at,
                        request,
                        shown,
                        text,
                    })
                    .await
                }
            })
        }

        fn draft_request(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<Draft> {
            draft_request(key, scope, entry)
        }

        fn draft_call_entry(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<DraftCall> {
            draft_call(key, scope, entry)
        }
    }

    impl FieldWriter for VerseWriter {
        fn save(
            &self,
            api: &Api,
            _key: &BufferKey,
            sent: &SentSave,
            base: &[ShownToken],
        ) -> LocalBoxFuture<Result<Ack, CallError>> {
            let api = api.clone();
            let request = sent
                .scope
                .clone()
                .zip(base.first().cloned())
                .map(|(at, shown)| SaveVerse {
                    at,
                    request: sent.request,
                    shown,
                    words: sent.text.clone(),
                });
            Box::pin(async move {
                match request {
                    Some(request) => api.call(&request).await,
                    None => Err(CallError::Transport(Transport::Unreadable(
                        "a verse save with no open or no token".to_owned(),
                    ))),
                }
            })
        }

        fn draft_request(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<Draft> {
            draft_request(key, scope, entry)
        }

        fn draft_entry(
            &self,
            api: &Api,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
            draft(api, key, scope, entry)
        }

        fn draft_call_entry(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<DraftCall> {
            draft_call(key, scope, entry)
        }
    }

    /// A section's one string and every displayed record, including after a reload.
    struct SectionWriter;

    impl FieldWriter for SectionWriter {
        fn save(
            &self,
            api: &Api,
            key: &BufferKey,
            sent: &SentSave,
            base: &[ShownToken],
        ) -> LocalBoxFuture<Result<Ack, CallError>> {
            let api = api.clone();
            let book = parse_verse_place(key.field.place.as_str()).map(|(book, _, _)| book);
            let request = sent.scope.clone().zip(book).map(|(at, book)| SaveSection {
                at,
                request: sent.request,
                book,
                displayed: base.to_vec(),
                text: sent.text.clone(),
            });
            Box::pin(async move {
                match request {
                    Some(request) => api.call(&request).await,
                    None => Err(CallError::Transport(Transport::Unreadable(
                        "a section save with no open or no book".to_owned(),
                    ))),
                }
            })
        }

        fn draft_request(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<Draft> {
            draft_request(key, scope, entry)
        }

        fn draft_entry(
            &self,
            api: &Api,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
            draft(api, key, scope, entry)
        }

        fn draft_call_entry(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<DraftCall> {
            draft_call(key, scope, entry)
        }
    }

    impl FieldWriter for LayoutWriter {
        fn save(
            &self,
            api: &Api,
            _key: &BufferKey,
            sent: &SentSave,
            base: &[ShownToken],
        ) -> LocalBoxFuture<Result<Ack, CallError>> {
            let api = api.clone();
            let request = sent
                .scope
                .clone()
                .zip(base.first().cloned())
                .map(|(at, shown)| SaveLayout {
                    at,
                    request: sent.request,
                    shown,
                    layout: sent.text.clone(),
                });
            Box::pin(async move {
                match request {
                    Some(request) => api.call(&request).await,
                    None => Err(CallError::Transport(Transport::Unreadable(
                        "a layout save with no open or no token".to_owned(),
                    ))),
                }
            })
        }

        fn draft_request(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<Draft> {
            draft_request(key, scope, entry)
        }

        fn draft_entry(
            &self,
            api: &Api,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
            draft(api, key, scope, entry)
        }

        fn draft_call_entry(
            &self,
            key: &BufferKey,
            scope: Option<Scope>,
            entry: &BufferEntry,
        ) -> Option<DraftCall> {
            draft_call(key, scope, entry)
        }
    }

    fn draft_request(key: &BufferKey, scope: Option<Scope>, entry: &BufferEntry) -> Option<Draft> {
        let field = match key.field.kind {
            FieldKind::Verse => DraftField::Words,
            FieldKind::Layout => DraftField::Layout,
            FieldKind::Section => DraftField::Section {
                book: parse_verse_place(key.field.place.as_str())?.0,
            },
            FieldKind::Comment => DraftField::UserComment {
                target: comment_target(key.field.place.as_str())?,
            },
            FieldKind::Frame | FieldKind::StoryTitle => DraftField::Frame,
            FieldKind::StoryRef => DraftField::StoryRef,
            _ => return None,
        };
        Some(Draft {
            at: scope?,
            request: super::super::js::random_request_id(),
            instance: entry.instance,
            field,
            shown: entry.base.clone(),
            text: entry.text.clone(),
            changed: entry.at,
        })
    }

    fn draft(
        api: &Api,
        key: &BufferKey,
        scope: Option<Scope>,
        entry: &BufferEntry,
    ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
        let api = api.clone();
        let request = draft_request(key, scope, entry)?;
        Some(Box::pin(async move { api.call(&request).await }))
    }

    fn draft_call(key: &BufferKey, scope: Option<Scope>, entry: &BufferEntry) -> Option<DraftCall> {
        let mut body = serde_json::to_value(draft_request(key, scope, entry)?).ok()?;
        let body = body.as_object_mut()?;
        body.remove("request");
        body.remove("text");
        Some(DraftCall {
            path: "editor/draft".to_owned(),
            body: serde_json::to_string(body).ok()?,
        })
    }

    pub(in crate::app::web) fn register(saves: &Saves) {
        saves.register(FieldKind::Verse, Rc::new(VerseWriter));
        saves.register(FieldKind::Layout, Rc::new(LayoutWriter));
        saves.register(FieldKind::Section, Rc::new(SectionWriter));
        saves.register(FieldKind::Comment, Rc::new(CommentWriter));
        saves.register(FieldKind::Frame, Rc::new(StoryWriter));
        saves.register(FieldKind::StoryTitle, Rc::new(StoryWriter));
        saves.register(FieldKind::StoryRef, Rc::new(StoryWriter));
    }
}

pub(super) use writer::register;
