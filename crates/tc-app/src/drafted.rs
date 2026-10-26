//! How much of a book is drafted (tC4 #572): one rule for Home's book tiles and the book rail.
//!
//! A verse is drafted when its words, trimmed as JavaScript trims, are neither empty nor the
//! stub `___`. A span such as `\v 2-3` counts every verse it covers, drafted or not, so joining
//! two drafted verses keeps the percentage (#572). The percentage is `Math.round` of drafted
//! over total, and 0 for a book with no verse (`draftPercent`, `tC4:src/data/usfm/indexer.ts`).
//!
//! A declared difference from tC4: tC4 reads a verse's body only up to its first paragraph
//! marker, so a poem verse whose words are on `\q` lines is undrafted there and drafted here.

use serde::{Deserialize, Serialize};
use tc_usfm::codec::{BookParts, DecomposeError, VersePart, decompose};

use crate::js;

/// The verses of a book, and how many of them are drafted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookDraft {
    pub drafted: u32,
    pub total: u32,
}

impl BookDraft {
    /// The drafted percentage, rounded as `Math.round` rounds; 0 when the book has no verse.
    pub fn percent(self) -> u8 {
        if self.total == 0 {
            return 0;
        }
        let percent = js::round(f64::from(self.drafted) / f64::from(self.total) * 100.0);
        // drafted ≤ total, so the percentage is 0 to 100.
        percent.clamp(0.0, 100.0) as u8
    }
}

/// The drafted count of a book's verse parts.
pub fn book_draft(parts: &BookParts) -> BookDraft {
    let mut draft = BookDraft {
        drafted: 0,
        total: 0,
    };
    for chapter in parts.chapters.values() {
        for (verse, part) in &chapter.verses {
            // A covered verse is counted by the span that covers it.
            let VersePart::Text(text) = part else {
                continue;
            };
            let covered = u32::from(text.last.max(*verse) - verse) + 1;
            draft.total += covered;
            if words_drafted(&text.words) {
                draft.drafted += covered;
            }
        }
    }
    draft
}

/// Counts one verse record into `draft`: a span counts every verse it covers, a covered or
/// absent verse nothing (the span that covers it is counted).
pub fn count_verse(draft: &mut BookDraft, verse: u16, value: &tc_journal::record::VerseValue) {
    let tc_journal::record::VerseValue::Text { words, last, .. } = value else {
        return;
    };
    let covered = u32::from(last.get().max(verse) - verse) + 1;
    draft.total += covered;
    if words_drafted(words) {
        draft.drafted += covered;
    }
}

/// The drafted count of a book's USFM text.
pub fn usfm_draft(usfm: &str) -> Result<BookDraft, DecomposeError> {
    decompose(usfm).map(|parts| book_draft(&parts))
}

pub(crate) use tc_usfm::codec::words_drafted;
