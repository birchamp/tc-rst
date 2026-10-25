//! Aligned and plain USFM of one book (tC4 `src/data/export/usfm.ts`, J7).
//!
//! Aligned USFM is the book as the journal stores it with each verse's alignment woven in as
//! `\zaln` and `\w` markup by the one weave, [`tc_align::usfm::weave_verse`], in tC4's bytes.
//! A verse is woven only when [`tc_align::needs_review`] is false: the rule of the Align
//! rail, so the export never writes links the translator is told to review (tC4 e2bfd8c3,
//! #796). Every other verse keeps its stored bytes, and a verse with a record is named in
//! the facts: [`SkipReason::InlineAlignment`] when it already holds `\zaln` markup (its
//! bytes are its alignment, #553), [`SkipReason::NeedsReview`], or
//! [`SkipReason::WeaveRefused`]. A record that places no word has nothing to weave and is
//! not named (tC4 F20 wrapped every word in `\w`). Nothing writes `x-ref` (#409).
//!
//! Plain USFM is the aligned book with its alignment markup removed
//! ([`tc_usfm::strip_alignment`]) piece by piece: the stored text, and a verse added from an
//! aligned file without its `\zaln` markup. A verse whose markup will not strip (an unclosed
//! `\zaln-s`, a `\w` with another attribute) keeps its stored bytes and is named
//! ([`SkipReason::MarkupKept`]), so one such verse never fails the whole export (tC4 exports
//! the stored file).

use std::collections::BTreeMap;

use tc_align::{AlignmentValue, SourceVerse, TargetVerse, needs_review};
use tc_core::{BookId, GitOid, Reference, VerseSpan};
use tc_journal::alignment::AlignedRun;
use tc_journal::ids::{ChapterNo, VerseNo};
use tc_usfm::codec::{BookParts, VersePart, join_verse, recompose};

use crate::{ExportFacts, SkipReason, SkippedVerse};

/// One book to export.
#[derive(Debug, Clone)]
pub struct BookInput {
    pub book: BookId,
    /// The book as the journal stores it (`book_parts`).
    pub parts: BookParts,
    /// Each verse's alignment, by the place of the verse (or the first verse of the span) in
    /// the book now.
    pub alignments: BTreeMap<(ChapterNo, VerseNo), VerseAlignment>,
    /// The sha of the book's original-language pin now, if pinned.
    pub source_sha: Option<GitOid>,
}

/// A verse's stored alignment and what it is judged against.
#[derive(Debug, Clone)]
pub struct VerseAlignment {
    /// The verse of the record's key: the verse's number when its text was aligned, which a
    /// renumber leaves behind (STORAGE §7.7).
    pub anchor: VerseNo,
    pub value: AlignmentValue,
    /// The pinned original-language verse, when it is available (tC4 #790).
    pub pin: Option<SourceVerse>,
}

/// The aligned book and the verses whose alignment it left out.
pub fn aligned(input: &BookInput) -> (String, ExportFacts) {
    let (parts, facts) = aligned_parts(input);
    (recompose(&parts).usfm, facts)
}

fn aligned_parts(input: &BookInput) -> (BookParts, ExportFacts) {
    let mut parts = input.parts.clone();
    let mut facts = ExportFacts::default();
    for (&c, chapter) in &mut parts.chapters {
        for (&v, part) in &mut chapter.verses {
            let VersePart::Text(text) = part else {
                continue;
            };
            let (Some(chapter_no), Some(verse_no)) = (ChapterNo::new(c), VerseNo::new(v)) else {
                continue;
            };
            let Some(alignment) = input.alignments.get(&(chapter_no, verse_no)) else {
                continue;
            };
            let last = text.last.max(v);
            let stored = join_verse(&text.words, &text.layout);
            match weave(&stored, v, last, alignment, input.source_sha) {
                Woven::Unchanged => {}
                Woven::Skipped(why) => facts.skip(SkippedVerse {
                    reference: Reference {
                        book: input.book,
                        chapter: c,
                        verses: VerseSpan::range(v, last).unwrap_or(VerseSpan::single(v)),
                    },
                    why,
                }),
                Woven::Aligned(woven) => {
                    text.words = woven;
                    text.layout.clear();
                }
            }
        }
    }
    (parts, facts)
}

/// The book with no alignment markup. A plain export leaves out every alignment by design,
/// so its facts name only the verses it kept as stored because their markup would not strip.
pub fn plain(input: &BookInput) -> (String, ExportFacts) {
    let (mut parts, _) = aligned_parts(input);
    let mut facts = ExportFacts::default();
    // A header or chapter front that will not strip keeps its bytes: no verse to name, and
    // nothing of the translator's is dropped.
    if let Ok(header) = tc_usfm::strip_alignment(&parts.header) {
        parts.header = header;
    }
    for (&c, chapter) in &mut parts.chapters {
        if let Some(front) = &mut chapter.front
            && let Ok(plain) = tc_usfm::strip_alignment(front)
        {
            *front = plain;
        }
        for (&v, part) in &mut chapter.verses {
            let VersePart::Text(text) = part else {
                continue;
            };
            let stored = join_verse(&text.words, &text.layout);
            match tc_usfm::strip_alignment(&stored) {
                Ok(plain) if plain == stored => {}
                Ok(plain) => {
                    text.words = plain;
                    text.layout.clear();
                }
                Err(_) => {
                    let last = text.last.max(v);
                    facts.skip(SkippedVerse {
                        reference: Reference {
                            book: input.book,
                            chapter: c,
                            verses: VerseSpan::range(v, last).unwrap_or(VerseSpan::single(v)),
                        },
                        why: SkipReason::MarkupKept,
                    });
                }
            }
        }
    }
    (recompose(&parts).usfm, facts)
}

enum Woven {
    Unchanged,
    Skipped(SkipReason),
    Aligned(String),
}

fn weave(
    stored: &str,
    run_start: u16,
    run_end: u16,
    alignment: &VerseAlignment,
    source_sha: Option<GitOid>,
) -> Woven {
    let value = &alignment.value;
    if value.usfm_groups().is_empty() {
        return Woven::Unchanged;
    }
    let verse = TargetVerse::new(stored);
    if verse.holds_alignment_markup() {
        return Woven::Skipped(SkipReason::InlineAlignment);
    }
    let run = AlignedRun {
        anchor: alignment.anchor.get(),
        run_start,
        run_end,
        text_md5: verse.md5(),
        source_sha,
    };
    if needs_review(value, &run, &verse, alignment.pin.as_ref()) {
        return Woven::Skipped(SkipReason::NeedsReview);
    }
    match tc_align::usfm::weave_verse(stored, value) {
        Ok(woven) => Woven::Aligned(woven),
        Err(_) => Woven::Skipped(SkipReason::WeaveRefused),
    }
}
