//! Tokens on the wire (AD3): an [`EditToken`] only ever becomes tc-api's `ShownToken`
//! (`st1.`), a [`ReviewToken`] only a `ReviewToken` (`rt1.`), and each is read back only as its
//! own kind, the MAC first.

use tc_api::write::TokenUpdate;
use tc_api::{ApiError, NotSaved};
use tc_journal::RecordKey;

use super::WireError;
use super::scope::record_ref;
use crate::backend::{EditToken, ProjectHandle, ReviewToken};

pub fn shown(token: &EditToken) -> Result<tc_api::ShownToken, WireError> {
    Ok(token.to_wire()?)
}

pub fn review(token: &ReviewToken) -> Result<tc_api::ReviewToken, WireError> {
    Ok(token.to_wire()?)
}

/// The edit token a write carried. A forged one, one of another project, or a review token
/// relabelled through JSON is `NotSaved(BadToken)`: a bug, never a save.
pub fn edit_token(
    handle: &dyn ProjectHandle,
    token: &tc_api::ShownToken,
) -> Result<EditToken, ApiError> {
    handle
        .parse_edit(token)
        .map_err(|_| ApiError::NotSaved(NotSaved::BadToken))
}

/// The review token a resolution carried.
pub fn review_token(
    handle: &dyn ProjectHandle,
    token: &tc_api::ReviewToken,
) -> Result<ReviewToken, ApiError> {
    handle
        .parse_review(token)
        .map_err(|_| ApiError::NotSaved(NotSaved::BadToken))
}

/// The token the next save of `key` uses, as an `Ack` carries it.
pub fn token_update(key: &RecordKey, token: &EditToken) -> Result<TokenUpdate, WireError> {
    Ok(TokenUpdate {
        record: record_ref(key).ok_or_else(|| WireError::Record(format!("{key:?}")))?,
        token: shown(token)?,
    })
}
