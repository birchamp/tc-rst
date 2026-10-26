//! Strategies for the tc-core types inside wire types, which have no `Arbitrary` of their
//! own.

use proptest::prelude::*;
use tc_core::BookId;

pub fn book() -> impl Strategy<Value = BookId> {
    proptest::sample::select(&BookId::ALL[..])
}

/// A few values. The helps and the source panes nest lists four deep (a chapter's notes, each
/// note's panes, each pane's lit verses, their tokens), and proptest's default of up to 100 at
/// each level makes millions of values for one case.
pub fn few<T: Arbitrary>() -> impl Strategy<Value = Vec<T>> {
    proptest::collection::vec(any::<T>(), 0..3)
}

/// A few lists of a few values.
pub fn few_lists<T: Arbitrary>() -> impl Strategy<Value = Vec<Vec<T>>> {
    proptest::collection::vec(few::<T>(), 0..3)
}

/// No list, or a few values.
pub fn maybe_few<T: Arbitrary>() -> impl Strategy<Value = Option<Vec<T>>> {
    proptest::option::of(few::<T>())
}
