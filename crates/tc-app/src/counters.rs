//! What the app's reads cost, counted (APP-DESIGN §8.1, PERFORMANCE rule 2).
//!
//! [`AppCounters`] holds one counter per kind of work the app's own code does. Every read a
//! query makes of a project goes through a [`CountingSnapshot`], which counts the records it
//! reads and the keys its range reads return, and counts each whole-project walk a screen
//! that shows the whole thing may make (the conflict list, a book's files or text). A count
//! test runs an action on the Psalms corpus and on one ten times larger and fails when a
//! count grows.
//!
//! The walks that cost the whole project (`record_keys`, `frontier`, `device_stats`,
//! `derived_files`, `batch_changes`, …) are not here at all: `crates/tc-app/clippy.toml` refuses
//! them, and the fold reads counted here, anywhere in tc-app's code but this file.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_journal::fold::ConflictId;
use tc_journal::fold::{
    BookView, ChapterKeys, Conflict, ConflictCounts, Counters, DerivedFile, KeyState, PersonLabel,
};
use tc_journal::token::TokenError;
use tc_journal::{BookId, ChangeBody, ChangeId, DeviceId, Record, RecordKey, ToolId};

use crate::backend::{EditToken, ReviewToken, Snapshot};

macro_rules! counters {
    ($( $(#[$doc:meta])* $variant:ident => $field:ident, )*) => {
        /// A kind of work the app counts.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum AppCounter {
            $( $(#[$doc])* $variant, )*
        }

        const COUNT: usize = [$( AppCounter::$variant ),*].len();

        /// The counts at one moment, one field per [`AppCounter`].
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct AppCounts {
            $( $(#[$doc])* pub $field: u64, )*
        }

        impl AppCounts {
            fn from_cells(cells: &[AtomicU64; COUNT]) -> Self {
                Self {
                    $( $field: cells[AppCounter::$variant as usize].load(Ordering::Relaxed), )*
                }
            }

            /// The counts' change from `earlier` to `self`, field by field.
            pub fn since(&self, earlier: &Self) -> Self {
                Self {
                    $( $field: self.$field.saturating_sub(earlier.$field), )*
                }
            }

            /// Every field by name, in declaration order.
            pub fn fields(&self) -> [(&'static str, u64); COUNT] {
                [$( (stringify!($field), self.$field) ),*]
            }
        }
    };
}

counters! {
    /// Records read one by one: a key's state, value, heads, writer or token.
    RecordsRead => records_read,
    /// Values decoded from stored bytes by the app's own reads.
    ValuesDecoded => values_decoded,
    /// Items derived from resources: checks, rows, groups.
    ItemsDerived => items_derived,
    /// Rows written into a reply.
    RowsSerialized => rows_serialized,
    /// Chapters projected as USFM.
    ChaptersProjected => chapters_projected,
    /// A book's resources loaded from the resource store.
    BookResourceLoads => book_resource_loads,
    /// Home summaries computed.
    SummariesComputed => summaries_computed,
    /// Keys returned by the fold's range reads (R1).
    KeysProbed => keys_probed,
    /// Saves that waited behind a draft write (A-20; 0 once R22 holds).
    SavesWaitingBehindADraftWrite => saves_waiting_behind_a_draft_write,
    /// Walks of the whole conflict list.
    ConflictWalks => conflict_walks,
    /// Books whose every derived file was rendered.
    BooksRendered => books_rendered,
    /// Whole books projected as USFM.
    BookUsfm => book_usfm,
    /// Derived files rendered one at a time, other than a book's text and the conflict list,
    /// which count as `book_usfm` and `conflict_walks`.
    FilesRendered => files_rendered,
}

/// The app's counters, shared by every reader. Clones of the `Arc` count into one set.
#[derive(Debug, Default)]
pub struct AppCounters {
    cells: [AtomicU64; COUNT],
}

impl AppCounters {
    pub fn add(&self, counter: AppCounter, n: u64) {
        self.cells[counter as usize].fetch_add(n, Ordering::Relaxed);
    }

    pub fn read(&self) -> AppCounts {
        AppCounts::from_cells(&self.cells)
    }
}

impl AppCounts {
    /// The fields of `self` larger than the same field of `other`: (name, self's, other's).
    pub fn exceeding(&self, other: &Self) -> Vec<(&'static str, u64, u64)> {
        self.fields()
            .into_iter()
            .zip(other.fields())
            .filter(|((_, mine), (_, theirs))| mine > theirs)
            .map(|((name, mine), (_, theirs))| (name, mine, theirs))
            .collect()
    }
}

/// A snapshot whose reads are counted. Every read a query makes of a project goes through
/// one; a later unit that needs another read adds its wrapper here, never a read of the fold
/// elsewhere.
#[derive(Debug, Clone)]
pub struct CountingSnapshot {
    snapshot: Snapshot,
    counters: Arc<AppCounters>,
}

impl CountingSnapshot {
    pub fn new(snapshot: Snapshot, counters: Arc<AppCounters>) -> Self {
        Self { snapshot, counters }
    }

    fn count(&self, counter: AppCounter, n: usize) {
        self.counters
            .add(counter, u64::try_from(n).unwrap_or(u64::MAX));
    }

    pub fn seq(&self) -> u64 {
        self.snapshot.seq()
    }

    /// The fold's own work counters, which this snapshot shares with the state it was taken
    /// from.
    pub fn fold_counters(&self) -> Counters {
        self.snapshot.fold().counters()
    }

    // ---- one record -------------------------------------------------------------------

    pub fn key_state(&self, key: &RecordKey) -> Option<KeyState> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.fold().key_state(key)
    }

    pub fn projected(&self, key: &RecordKey) -> Option<Record> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.fold().projected(key)
    }

    pub fn heads(&self, key: &RecordKey) -> Vec<ChangeId> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.fold().heads(key)
    }

    /// The device whose change the record shows, when it shows one change.
    pub fn writer_of(&self, key: &RecordKey) -> Option<DeviceId> {
        self.count(AppCounter::RecordsRead, 1);
        let fold = self.snapshot.fold();
        let change = fold.projected_change(key)?;
        fold.change(&change).map(|view| view.device)
    }

    /// One change's body: who wrote it, when, and the record it holds.
    pub fn body(&self, change: &ChangeId) -> Option<ChangeBody> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.fold().body(change)
    }

    /// One conflict, by its id.
    pub fn conflict(&self, id: &ConflictId) -> Option<Conflict> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.fold().conflict(id).cloned()
    }

    /// An edit token for `key` as this snapshot shows it ([`Snapshot::mint`]).
    pub fn mint(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<EditToken, TokenError> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.mint(key, displayed)
    }

    pub fn mint_review(&self, key: &RecordKey) -> Result<ReviewToken, TokenError> {
        self.count(AppCounter::RecordsRead, 1);
        self.snapshot.mint_review(key)
    }

    // ---- range reads (R1): each costs what it returns ---------------------------------

    /// The chapters of `book` with a verse or a front. Only numbers: not counted. A record
    /// anchored in a chapter with neither (an alignment or comment after a renumber) is in
    /// no chapter listed here, so a book-wide read uses [`Self::alignments_of`] or
    /// [`Self::comments_of`], never a loop over these.
    pub fn chapters_of(&self, book: BookId) -> Vec<u16> {
        self.snapshot.fold().chapters_of(book)
    }

    pub fn chapter_keys(&self, book: BookId, chapter: u16) -> ChapterKeys {
        let keys = self.snapshot.fold().chapter_keys(book, chapter);
        let n = usize::from(keys.front.is_some())
            + keys.verses.len()
            + keys.alignments.len()
            + keys.comments.len()
            + keys.checks.len();
        self.count(AppCounter::KeysProbed, n);
        keys
    }

    pub fn decisions_of(&self, tool: ToolId, book: BookId) -> Vec<RecordKey> {
        let keys = self.snapshot.fold().decisions_of(tool, book);
        self.count(AppCounter::KeysProbed, keys.len());
        keys
    }

    pub fn alignments_of(&self, book: BookId) -> Vec<RecordKey> {
        let keys = self.snapshot.fold().alignments_of(book);
        self.count(AppCounter::KeysProbed, keys.len());
        keys
    }

    pub fn comments_of(&self, book: BookId) -> Vec<RecordKey> {
        let keys = self.snapshot.fold().comments_of(book);
        self.count(AppCounter::KeysProbed, keys.len());
        keys
    }

    /// Records with no place on any screen (§7.7).
    pub fn detached(&self) -> Vec<RecordKey> {
        let keys: Vec<RecordKey> = self.snapshot.fold().detached().cloned().collect();
        self.count(AppCounter::KeysProbed, keys.len());
        keys
    }

    // ---- projection -------------------------------------------------------------------

    /// One chapter's USFM, from its marker to the byte before the next chapter's.
    pub fn chapter_usfm(&self, book: BookId, chapter: u16) -> Option<String> {
        self.count(AppCounter::ChaptersProjected, 1);
        self.snapshot.fold().chapter_usfm(book, chapter)
    }

    pub fn book(&self, book: BookId) -> BookView {
        self.snapshot.fold().book(book)
    }

    pub fn projected_books(&self) -> Vec<BookId> {
        self.snapshot.fold().projected_books()
    }

    /// Kept up to date by every fold step: per book and kind, never a walk.
    pub fn conflict_counts(&self) -> &ConflictCounts {
        self.snapshot.fold().conflict_counts()
    }

    pub fn person_labels(&self, me: DeviceId) -> BTreeMap<DeviceId, PersonLabel> {
        self.snapshot.fold().person_labels(me)
    }

    // ---- whole-project walks, counted: only a screen that shows the whole thing --------

    /// The whole conflict list.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one counted walk of the conflict list"
    )]
    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.count(AppCounter::ConflictWalks, 1);
        self.snapshot.fold().conflicts()
    }

    /// Every derived file of `book`, with its bytes.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one counted rendering of a whole book"
    )]
    pub fn render_book(&self, book: BookId) -> Vec<(DerivedFile, Vec<u8>)> {
        self.count(AppCounter::BooksRendered, 1);
        self.snapshot.fold().render_book(book)
    }

    /// A whole book's USFM.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one counted projection of a whole book"
    )]
    pub fn book_usfm(&self, book: BookId) -> Option<String> {
        self.count(AppCounter::BookUsfm, 1);
        self.snapshot.fold().book_usfm(book)
    }

    /// One derived file's bytes. A book's text projects the whole book and the conflict list
    /// walks every conflict, so they count as [`Self::book_usfm`] and [`Self::conflicts`] do.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one counted rendering of a single file"
    )]
    pub fn render(&self, file: &DerivedFile) -> Option<Vec<u8>> {
        let counter = match file {
            DerivedFile::Usfm(_) => AppCounter::BookUsfm,
            DerivedFile::Conflicts => AppCounter::ConflictWalks,
            DerivedFile::Alignments(_)
            | DerivedFile::Decisions(..)
            | DerivedFile::Comments(_)
            | DerivedFile::Resources
            | DerivedFile::Settings
            | DerivedFile::Versification
            | DerivedFile::Story(_)
            | DerivedFile::StoryDecisions(_)
            | DerivedFile::StoryComments => AppCounter::FilesRendered,
        };
        self.count(counter, 1);
        self.snapshot.fold().render(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_counts_since_subtracts_field_by_field() {
        let counters = AppCounters::default();
        counters.add(AppCounter::RecordsRead, 3);
        counters.add(AppCounter::KeysProbed, 10);
        let before = counters.read();
        counters.add(AppCounter::RecordsRead, 2);
        counters.add(AppCounter::BookUsfm, 1);
        let delta = counters.read().since(&before);
        assert_eq!(
            delta,
            AppCounts {
                records_read: 2,
                book_usfm: 1,
                ..AppCounts::default()
            }
        );
        // Every counter has its own cell: each one moves only its own field.
        let fields = AppCounts::default().fields();
        assert_eq!(fields.len(), COUNT);
        let names: std::collections::BTreeSet<&str> = fields.iter().map(|f| f.0).collect();
        assert_eq!(names.len(), COUNT, "names are distinct");
    }

    #[test]
    fn exceeding_names_every_field_that_grew() {
        let small = AppCounts {
            records_read: 5,
            keys_probed: 7,
            chapters_projected: 1,
            ..AppCounts::default()
        };
        let large = AppCounts {
            records_read: 5,
            keys_probed: 70,
            chapters_projected: 2,
            conflict_walks: 1,
            ..AppCounts::default()
        };
        assert_eq!(
            large.exceeding(&small),
            vec![
                ("chapters_projected", 2, 1),
                ("keys_probed", 70, 7),
                ("conflict_walks", 1, 0),
            ]
        );
        assert!(small.exceeding(&large).is_empty());
        assert!(large.exceeding(&large).is_empty());
    }
}
