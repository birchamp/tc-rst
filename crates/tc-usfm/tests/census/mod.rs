//! The census of a property test (`docs/STORAGE.md` §16.1): each run counts the states its
//! generator reached and fails when one was never reached, because a generator that never
//! builds a state tests nothing about it.

#![allow(clippy::panic, reason = "test code: a failed unwrap is a failed test")]

use proptest::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub struct Census {
    expected: &'static [&'static str],
    counts: RefCell<BTreeMap<&'static str, u64>>,
}

impl Census {
    /// Counts one case that reached `state`, which must be one of the expected states.
    pub fn saw(&self, state: &'static str) {
        assert!(
            self.expected.contains(&state),
            "{state:?} is not a state of this census"
        );
        *self.counts.borrow_mut().entry(state).or_default() += 1;
    }
}

/// Runs a property over `cases` generated values, then fails if an expected state was never
/// reached.
pub fn run_with_census<S: Strategy>(
    cases: u32,
    strategy: S,
    expected: &'static [&'static str],
    test: impl Fn(S::Value, &Census) -> Result<(), TestCaseError>,
) {
    let census = Census {
        expected,
        counts: RefCell::default(),
    };
    let config = ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    let mut runner = proptest::test_runner::TestRunner::new(config);
    if let Err(error) = runner.run(&strategy, |value| test(value, &census)) {
        panic!("{error}");
    }
    let counts = census.counts.borrow();
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|state| !counts.contains_key(state))
        .collect();
    assert!(
        missing.is_empty(),
        "the generator never reached {missing:?}; it reached {counts:?}"
    );
}
