//! The question about books another program changed, and the kept copies (D1, owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.11): the words, decided on the host.

use tc_api::conflicts::{ReviewTab, TabCount};
use tc_api::session::{KeptCopy, KeptImported, WaitingBook};
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
        keys::outside::IMPORTED,
        &[
            ("book", &name_of(&report.file, report.book)),
            ("imported", &report.imported),
            ("review", &report.to_review),
        ],
    )
}

/// A kept copy's line in the conflict review's notices.
pub fn kept_line(kept: &KeptCopy) -> String {
    t(
        keys::outside::KEPT,
        &[("name", &name_of(&kept.file, book_of_file(&kept.file)))],
    )
}

/// *Dismiss* is refused while the copy's book still waits for the question's answer.
pub fn can_dismiss(kept: &KeptCopy, waiting: &[WaitingBook]) -> bool {
    !waiting.iter().any(|w| w.file == kept.file)
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
