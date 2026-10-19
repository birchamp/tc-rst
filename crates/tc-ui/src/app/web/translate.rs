//! Translate's actions (unit B4a): tC4's `startVerse` and `blurVerse`, the verse card's lock
//! (`Editing::Verse`), and the verse field's writer. The card's field is a save-machine field
//! of kind `Verse` (6.5): it saves on blur or 2 s after the last keystroke.
//!
//! `editor/saveVerse` is C2's. Until it exists, a build with the test hooks saves a verse
//! through `test/saveVerse`, the real save through the store (C0), so the journeys prove the
//! card on real outcomes; any other build has no verse writer, and a save is `Unavailable`:
//! "Not saved" with Retry, the text kept in the buffer.

use leptos::prelude::*;

use super::app_ctx::Actions;
use crate::app::shell::Editing;
use crate::app::translate::verse_key;

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
}

#[cfg(feature = "test-hooks")]
mod writer {
    use std::rc::Rc;

    use tc_api::buffer::{BufferKey, DraftCall, FieldKind, SentSave};
    use tc_api::test::SaveVerse;
    use tc_api::token::ShownToken;
    use tc_api::write::Ack;

    use crate::app::client::{CallError, Transport};
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

    pub(in crate::app::web) fn register(saves: &Saves) {
        saves.register(FieldKind::Verse, Rc::new(VerseWriter));
    }
}

#[cfg(feature = "test-hooks")]
pub(super) use writer::register;
