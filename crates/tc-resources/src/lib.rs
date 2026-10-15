//! Translation resources for tC: installing, pinning, verifying, upgrading and loading
//! them, from Door43 (DCS) and from Aquifer, all usable offline once installed.

mod dcs;
mod error;
pub mod gateways;
mod layout;
mod pin;
pub mod resolve;
mod sets;
mod store;
pub mod suite;

pub use dcs::{install_all, install_dcs, install_dcs_as, pin_release};
pub use error::{AquiferError, InstallError, ParseError, Refusal, StoreError};
pub use pin::{
    AQUIFER_OWNER, AquiferLanguage, AquiferPin, AquiferRepo, AssetName, BookCoverage, DCS_HOST,
    GithubName, RepoPath, ResourcePin, Sha256,
};
pub use sets::{
    GatewayLanguage, LanguageSet, PaneId, PinGroup, ProjectKind, ProjectPins, Rung, ScripturePane,
    SetSlot, Testaments, Tool,
};
pub use store::{FileRecord, Installed, Origin, Packaging, Receipt, ResourceStore, SeedReport};
