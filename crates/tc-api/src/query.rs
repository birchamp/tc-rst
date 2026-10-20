//! Which cached reads a change makes stale (APP-DESIGN §3.6, §6.2). The UI and the
//! server's invalidation tests use this one rule.
//!
//! A query names the parts of the project it shows as one or more keys; it is fetched
//! again when a `Changed` event's scope touches any of them. A key that misses a change
//! would leave a teammate's work off the screen, so each rule errs toward a refresh.

use tc_core::BookId;

use crate::check::{CheckListRef, CheckUnit};
use crate::event::{Aspect, ChangeScope, ChapterRef};
use crate::ids::{RecordRef, StoryNo};

/// A part of the project that a query shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryKey {
    /// One chapter's verses, front and sections.
    Chapter(ChapterRef),
    /// A whole book: its head, its chapter list, and every chapter in it.
    Book(BookId),
    /// One OBS story.
    Story(StoryNo),
    /// One tool's check list for one book, or for the stories.
    CheckList(CheckListRef),
    /// One record, such as a check's decision.
    Record(RecordRef),
    /// Project-wide data: pins, settings, details or the conflict list.
    Aspect(Aspect),
}

impl QueryKey {
    pub fn depends_on(&self, scope: &ChangeScope) -> bool {
        let parts = match scope {
            ChangeScope::Everything => return true,
            ChangeScope::Parts(parts) => parts,
        };
        let book_changed = |book: BookId| parts.books.contains(&book);
        match self {
            Self::Chapter(chapter) => {
                book_changed(chapter.book) || parts.chapters.contains(chapter)
            }
            Self::Book(book) => {
                book_changed(*book) || parts.chapters.iter().any(|c| c.book == *book)
            }
            Self::Story(story) => parts.stories.contains(story),
            // A check list is derived from the pinned resources and from the presence of its
            // book. `ChangeParts` names a story whether it was added, removed or edited, and
            // a story's presence cannot be told from its text here, so any change of its own
            // story counts; a book's text edit does not, because the server names the lists
            // it re-assessed in `checks`.
            Self::CheckList(list) => {
                parts.checks.contains(list)
                    || parts.aspects.contains(&Aspect::Pins)
                    || match list.unit {
                        CheckUnit::Book(book) => book_changed(book),
                        CheckUnit::Story(story) => parts.stories.contains(&story),
                    }
            }
            Self::Record(record) => parts.records.iter().any(|r| &r.record == record),
            Self::Aspect(aspect) => parts.aspects.contains(aspect),
        }
    }
}
