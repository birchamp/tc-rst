//! What an aligned export reads of a book (D2): the book's parts and, for each verse with an
//! alignment record, that record keyed by the verse's place now, the anchor it was stored
//! under (STORAGE §7.7), and the pinned original-language verse when this computer has it, so
//! tc-export judges each record by the rule the rail uses (`tc_align::needs_review`).

use std::collections::BTreeMap;
use std::sync::Arc;

use tc_align::frame::source_reference;
use tc_api::ApiError;
use tc_core::{BookId, Reference, VerseSpan};
use tc_export::{BookInput, VerseAlignment};
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use super::open::{Original, anchor, original_language};
use crate::app::App;
use crate::session::ProjectSession;

/// The export input of `book`, or `BadRequest` when the project does not hold it. Costs the
/// book: one projection of it and one probe per verse.
pub(crate) async fn book_input(
    app: &App,
    session: &Arc<ProjectSession>,
    book: BookId,
) -> Result<BookInput, ApiError> {
    // The original-language text is optional: without it each record is judged on the
    // target's words alone, as the rail judges a verse whose pin is not here.
    let loaded = app.loaded_book(session, book).await.ok();
    let original = match &loaded {
        Some(loaded) => match session.align().original(loaded, book).await {
            Original::Ready { book } => Some(book),
            Original::Blocked(_) => None,
        },
        None => None,
    };
    let read = session.read();
    let Some(parts) = read.book_parts(book) else {
        return Err(ApiError::BadRequest {
            field: "subject".to_owned(),
            reason: format!("{} is not in the project", book.code()),
        });
    };
    let source_sha = original_language(&read, book).0.map(|r| r.sha);
    let mut alignments = BTreeMap::new();
    for chapter in read.chapters_of(book) {
        for key in read.chapter_keys(book, chapter).verses {
            let Some(Record::Verse {
                chapter: c,
                verse,
                value: VerseValue::Text { last, from, .. },
                ..
            }) = read.projected(&key)
            else {
                continue;
            };
            let at = anchor(c, verse, from);
            let Some(Record::Alignment { value, .. }) =
                read.projected(&RecordKey::Alignment(book, at.chapter, at.verse))
            else {
                continue;
            };
            let pin = match (&loaded, &original) {
                (Some(loaded), Some(original)) => {
                    VerseSpan::range(verse.get(), last.get().max(verse.get()))
                        .and_then(|verses| {
                            source_reference(
                                loaded.numbering.project_scheme(),
                                Reference::new(book, c, verses),
                            )
                            .ok()
                        })
                        .and_then(|source| original.verse(source.chapter, source.verses).ok())
                }
                _ => None,
            };
            alignments.insert(
                (c, verse),
                VerseAlignment {
                    anchor: at.verse,
                    value,
                    pin,
                },
            );
        }
    }
    Ok(BookInput {
        book,
        parts,
        alignments,
        source_sha,
    })
}

/// The direction a print is set in: the project language's (tC4 `scriptDirection`).
pub(crate) fn print_dir(read: &crate::counters::CountingSnapshot) -> tc_export::Direction {
    match crate::direction::of_fold(read) {
        tc_api::shown::Direction::Ltr => tc_export::Direction::Ltr,
        tc_api::shown::Direction::Rtl => tc_export::Direction::Rtl,
    }
}
