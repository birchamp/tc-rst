//! Pure conversions between the seam's types and tc-api's (APP-DESIGN AD2, §4.1): tokens,
//! records, scopes of a change, people and the seam's errors. Each later unit adds the views
//! of its screens here.
//!
//! A conversion that cannot be exact errs toward the larger answer: a change it cannot
//! describe part by part is `ChangeScope::Everything`, which costs a refresh but never leaves
//! a teammate's work off a screen.

pub mod error;
pub mod scope;
pub mod token;

/// A value the server made that tc-api cannot carry: a bug. The handler that meets one
/// reports it and answers `ApiError::Internal`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("a token cannot be written for the wire: {0}")]
    Token(#[from] tc_api::token::TokenTextError),
    #[error("a record key cannot be written for the wire: {0}")]
    Record(String),
}
