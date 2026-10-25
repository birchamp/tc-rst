//! The question about books another program changed, and the kept copies (D1, owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.11): the words, decided on the host.

use tc_api::conflicts::{ReviewTab, TabCount};
use tc_api::event::SessionMode;
use tc_api::session::{KeptCopy, KeptImported, KeptWhy, WaitingBook};
use tc_core::BookId;

use crate::i18n::{keys, t};

/// A file by the name the question uses: the book's name, else the file's own name.
pub fn name_of(file: &str, book: Option<BookId>) -> String {
    match book {
        Some(book) => book.english_name().to_owned(),
        None => file_name(file).to_owned(),
    }
}

fn file_name(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

/// The one question for every book found together; `None` when no book waits.
pub fn question(waiting: &[WaitingBook]) -> Option<String> {
    use keys::outside::question as q;
    match waiting {
        [] => None,
        [one] if one.added => Some(t(q::ADDED, &[("file", &file_name(&one.file))])),
        [_] => Some(t(q::ONE, &[])),
        many => {
            let books = many
                .iter()
                .map(|w| name_of(&w.file, w.book))
                .collect::<Vec<_>>()
                .join(", ");
            Some(t(q::MANY, &[("books", &books)]))
        }
    }
}

/// *Import* is offered while a waiting book can be read; one that cannot offers only *Not now*
/// (delta §2.11 Q2).
pub fn can_import(waiting: &[WaitingBook]) -> bool {
    waiting.iter().any(|w| w.unreadable.is_none())
}

/// The report of one *Import*: "Titus: 12 verses imported, 2 to review."
pub fn report_line(report: &KeptImported) -> String {
    t(
        if report.imported == 1 {
            keys::outside::IMPORTED_ONE
        } else {
            keys::outside::IMPORTED
        },
        &[
            ("book", &name_of(&report.file, report.book)),
            ("imported", &report.imported),
            ("review", &report.to_review),
        ],
    )
}

/// A kept copy's line in the conflict review's notices; lost work in H22 part 1's words.
pub fn kept_line(kept: &KeptCopy) -> String {
    match kept.why {
        KeptWhy::Changed => t(
            keys::outside::KEPT,
            &[("name", &name_of(&kept.file, book_of_file(&kept.file)))],
        ),
        KeptWhy::Lost => t(keys::outside::LOST, &[]),
    }
}

/// The row's button that takes the copy in: *Import*, or *Bring it back* for lost work; both
/// run the same import and show the same report.
pub fn kept_import_label(kept: &KeptCopy) -> String {
    match kept.why {
        KeptWhy::Changed => t(keys::outside::IMPORT, &[]),
        KeptWhy::Lost => t(keys::outside::BRING_BACK, &[]),
    }
}

/// What a kept copy's row offers besides its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeptOffer {
    /// The project is read-only: the store refuses both.
    Nothing,
    /// Its book still waits for the question's answer, so *Dismiss* is refused.
    Import,
    ImportOrDismiss,
}

pub fn kept_offer(kept: &KeptCopy, waiting: &[WaitingBook], mode: &SessionMode) -> KeptOffer {
    if matches!(mode, SessionMode::ReadOnly(_)) {
        KeptOffer::Nothing
    } else if waiting.iter().any(|w| w.file == kept.file) {
        KeptOffer::Import
    } else {
        KeptOffer::ImportOrDismiss
    }
}

/// The book an `ingredients/<CODE>.usfm` path is.
pub fn book_of_file(file: &str) -> Option<BookId> {
    file_name(file).strip_suffix(".usfm")?.parse().ok()
}

/// The review's tab counts with the kept copies among the notices, so the Notices tab shows
/// while a copy waits there.
pub fn with_kept(tabs: &[TabCount], kept: usize) -> Vec<TabCount> {
    let kept = u32::try_from(kept).unwrap_or(u32::MAX);
    tabs.iter()
        .map(|tab| TabCount {
            tab: tab.tab,
            count: if tab.tab == ReviewTab::Notices {
                tab.count.saturating_add(kept)
            } else {
                tab.count
            },
        })
        .collect()
}

/// The toolbar's way into the conflict review (INV-A-KEPT-COPY-STAYS-ON-A-SCREEN): tC4's button
/// counts the open verse conflicts; here it counts the kept copies too, so a copy left by *Not
/// now* stays one click away. It opens at the conflicts, else at the notices that list the
/// copies. `None` when there is neither.
pub fn review_entry(conflicts: u32, kept: usize) -> Option<(u32, ReviewTab)> {
    let kept = u32::try_from(kept).unwrap_or(u32::MAX);
    match (conflicts, kept) {
        (0, 0) => None,
        (0, kept) => Some((kept, ReviewTab::Notices)),
        (conflicts, kept) => Some((conflicts.saturating_add(kept), ReviewTab::Text)),
    }
}
