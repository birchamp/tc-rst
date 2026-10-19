//! The `net` service: the internet switch (APP-DESIGN §5.6). It lives only in the server,
//! is off at every launch and is never stored.

use serde::{Deserialize, Serialize};

use crate::ids::RequestId;

/// `net/state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct State {}

/// `net/set`: "Turn on internet" in the question, or the switch in the account menu.
/// Turning it off stops the very next Door43 request and download chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Set {
    pub request: RequestId,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct NetView {
    pub on: bool,
}
