//! The states of a Home card (APP-DESIGN §3.7, A-30). Unit B2 adds the card.

use serde::{Deserialize, Serialize};

/// How the project came to this computer: an imported project carries the "Imported"
/// badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ProjectOrigin {
    Created,
    Imported,
}

/// Whether the project is on Door43. A shared project, Bible or OBS, offers "Sync with
/// Door43" (PLAN, Q2); one that is not offers Share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Door43Card {
    Local,
    Shared { place: RepoPlace },
}

/// A Door43 repository: `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RepoPlace {
    pub owner: String,
    pub name: String,
}
