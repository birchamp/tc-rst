//! Strategies for the tc-core types inside wire types, which have no `Arbitrary` of their
//! own.

use proptest::prelude::*;
use tc_core::BookId;

pub fn book() -> impl Strategy<Value = BookId> {
    proptest::sample::select(&BookId::ALL[..])
}
