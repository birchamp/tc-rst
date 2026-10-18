//! The aligner's training input (`docs/TC4-DELTA-2026-10-16.md` §3.10 item 3): each verse's
//! text and alignment from one snapshot, so training during a book switch never pairs one
//! book's alignments with another book's text. The editor's unsaved text replaces a verse's
//! text only when it belongs to that book.

use crate::handle::Snapshot;
use tc_journal::record::{AlignmentValue, VerseValue};
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};

/// Unsaved text of one verse in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorText {
    pub book: BookId,
    pub chapter: ChapterNo,
    pub verse: VerseNo,
    pub words: String,
}

/// One verse of the training input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingVerse {
    pub chapter: ChapterNo,
    pub verse: VerseNo,
    /// The verse's words (`words` of a text value).
    pub words: String,
    pub alignment: Option<AlignmentValue>,
}

impl Snapshot {
    /// The verses of `book` that have text, each with its alignment, from this snapshot
    /// alone.
    pub fn training_input(&self, book: BookId, editor: Option<&EditorText>) -> Vec<TrainingVerse> {
        let fold = self.fold();
        let mut out = Vec::new();
        for chapter in fold.chapters_of(book) {
            let keys = fold.chapter_keys(book, chapter);
            for key in keys.verses {
                let RecordKey::Verse(_, c, v) = key else {
                    continue;
                };
                let Some(Record::Verse {
                    value: VerseValue::Text { words, .. },
                    ..
                }) = fold.projected(&key)
                else {
                    continue;
                };
                let words = match editor {
                    Some(e) if e.book == book && e.chapter == c && e.verse == v => e.words.clone(),
                    _ => words,
                };
                let alignment = match fold.projected(&RecordKey::Alignment(book, c, v)) {
                    Some(Record::Alignment { value, .. }) => Some(value),
                    _ => None,
                };
                out.push(TrainingVerse {
                    chapter: c,
                    verse: v,
                    words,
                    alignment,
                });
            }
        }
        out
    }
}
