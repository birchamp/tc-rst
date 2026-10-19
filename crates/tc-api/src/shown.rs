//! Values as the UI shows them (APP-DESIGN §3.4).

use serde::{Deserialize, Serialize};

use crate::ids::ConflictRef;
use crate::token::ShownToken;

/// An editable value with the token of the version it shows. An editor keeps the two as one
/// pair from the moment it opens (AD37), so a save never claims a version it did not show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Shown<T> {
    pub value: T,
    pub token: ShownToken,
    /// Present while the record is in conflict: "2 versions · Review".
    pub marker: Option<ConflictMarker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ConflictMarker {
    pub conflict: ConflictRef,
    pub versions: u8,
}

/// Who made a change, as the review and the notices name them (STORAGE:2686-2691).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PersonLabel {
    pub who: Who,
    /// The computer's name, when the same person works on more than one.
    pub computer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Who {
    /// The person at this computer.
    Mine,
    /// A Door43 username.
    Username(String),
    /// A name the teammate gave, with no Door43 account.
    DisplayName(String),
    /// "Teammate 2": a device with no name.
    Teammate(u16),
}

/// A count or value the server could not compute. The UI shows "—", never a false 0
/// (`tC4:src/views/Home.jsx:259-264`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Known<T> {
    Known(T),
    Unknown,
}

/// The direction a text reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Ltr,
    Rtl,
}
