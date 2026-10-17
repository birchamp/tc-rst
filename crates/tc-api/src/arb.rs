//! Strategies for the tc-core types inside wire types, which have no `Arbitrary` of their
//! own.

use proptest::prelude::*;
use tc_core::{BookId, Reference, VerseSpan};

pub fn book() -> impl Strategy<Value = BookId> {
    proptest::sample::select(&BookId::ALL[..])
}

pub fn reference() -> impl Strategy<Value = Reference> {
    (book(), 1u16..=150, 1u16..=176, 0u16..3).prop_map(|(book, chapter, start, extra)| {
        // `range` refuses only an end before its start, which `start + extra` never is.
        let verses = VerseSpan::range(start, start + extra).unwrap_or(VerseSpan::single(start));
        Reference {
            book,
            chapter,
            verses,
        }
    })
}
