//! Translate's actions (units B4a and B4b): tC4's `startVerse`, `startSection` and `blurVerse`,
//! the cards' lock (`Editing::Verse`, `Editing::Section`), and the writers of their fields. A
//! verse card's field is a save-machine field of kind `Verse` (6.5): it saves on blur or 2 s
//! after the last keystroke. A section card's is of kind `Section`: it saves only by its Save
//! button (A-1), and Cancel discards it.
//!
//! `editor/saveVerse` and `editor/saveSection` are C2's. Until they exist, a build with the test
//! hooks saves through `test/saveVerse` and `test/saveSection`, the real saves through the store
//! (C0, B4b), so the journeys prove the cards on real outcomes; any other build has no writer,
//! and a save is `Unavailable`: "Not saved" with Retry, the text kept in the buffer.

use leptos::prelude::*;

use super::app_ctx::Actions;
use crate::app::shell::Editing;
use crate::app::translate::{section_key, verse_key};

impl Actions {
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

#[cfg(feature = "test-hooks")]
mod writer {
    use std::rc::Rc;

    use tc_api::buffer::{BufferKey, DraftCall, FieldKind, SentSave};
    use tc_api::test::{SaveSection, SaveVerse};
    use tc_api::token::ShownToken;
    use tc_api::write::Ack;

    use crate::app::client::{CallError, Transport};
    use crate::app::translate::parse_verse_place;
    use crate::app::web::{Api, FieldWriter, LocalBoxFuture, Saves};

    /// A verse saved through `test/saveVerse`, the real save through the store, until C2's
    /// `editor/saveVerse`.
    struct VerseWriter;

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

        /// `editor/draft` comes with C2.
        fn draft(
            &self,
            _api: &Api,
            _key: &BufferKey,
            _text: &str,
            _base: &[ShownToken],
        ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
            None
        }

        fn draft_call(&self, _key: &BufferKey, _base: &[ShownToken]) -> Option<DraftCall> {
            None
        }
    }

    /// A section saved through `test/saveSection`, the real save through the store, until C2's
    /// `editor/saveSection`: the card's text over the tokens of every verse it displayed.
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

        /// `editor/draft` comes with C2.
        fn draft(
            &self,
            _api: &Api,
            _key: &BufferKey,
            _text: &str,
            _base: &[ShownToken],
        ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
            None
        }

        fn draft_call(&self, _key: &BufferKey, _base: &[ShownToken]) -> Option<DraftCall> {
            None
        }
    }

    pub(in crate::app::web) fn register(saves: &Saves) {
        saves.register(FieldKind::Verse, Rc::new(VerseWriter));
        saves.register(FieldKind::Section, Rc::new(SectionWriter));
    }
}

#[cfg(feature = "test-hooks")]
pub(super) use writer::register;
