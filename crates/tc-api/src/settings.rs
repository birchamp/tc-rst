//! Project settings (M7, unit B9a; `tC4:src/views/modals/ProjectSettings.jsx`): what the dialog
//! shows when a Home card's Settings opens it. tC4 opens Settings only from Home (#505), so the
//! read is `settings/viewFromHome`, from the project's derived files with no open (AD14).
//! Saving is unit C6's: `settings/save`, below.

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectRef, RequestId};
use crate::resources::{FromInstalls, GatewayPackage, GatewayRef};
use crate::shown::Direction;

/// `settings/viewFromHome`: a project's settings and checking language from its files, with no
/// open and no fold (AD14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewFromHome {
    pub project: ProjectRef,
}

/// What Project settings shows beside the card's own facts (name, code, books). A value the
/// project does not hold is tC4's: no `languageName` is an empty field, no `textFont` the first
/// font, anything but `rtl` left to right. A `checkingLanguage` an early project carries is
/// kept in its file and never read: the pins name the checking language (tC4 #529).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub language_name: String,
    pub direction: Direction,
    pub font: Option<String>,
    pub gateway: GatewayCard,
}

/// The Checking language card (tC4 `GatewayCard`, #412).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct GatewayCard {
    /// The primary set's gateway language from the project's pins; `None`: the project has no
    /// checking language yet, never English by default.
    pub current: Option<CurrentGateway>,
    /// The packages installed on this computer that cover the project's books.
    pub options: FromInstalls<GatewayPackage>,
}

/// The current checking language, named by its autonym (tC4 #579, D94).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CurrentGateway {
    pub gateway: GatewayRef,
    /// The language's own name; the tag when the registry does not name it.
    pub autonym: String,
}

/// The font Project settings and New Bible show first, and the one a project that holds none
/// shows (tC4 `SCRIPT_FONTS[0]`).
pub const DEFAULT_FONT: &str = "Noto Sans (default)";

/// The values the dialog showed, which a save is made from (AD14, tC4's compare-and-swap of
/// `readSettingsWithMd5`): the project must still hold them, or nothing is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShownValues {
    pub direction: Direction,
    /// `None` when the project holds no font; the dialog showed the first one.
    pub font: Option<String>,
}

/// `settings/save`: Save changes from a Home card's Project settings. It opens the project
/// through the registry's one open of the folder, so a card clicked meanwhile shares the
/// session (AD38), checks that the project still holds `shown`, and saves the direction and
/// the font that changed (R18's fallback: two batches).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Save {
    pub request: RequestId,
    pub project: ProjectRef,
    pub shown: ShownValues,
    pub direction: Direction,
    pub font: String,
}
