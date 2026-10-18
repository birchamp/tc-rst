//! Translation resources for tC: installing, pinning, verifying, upgrading and loading
//! them, from Door43 (DCS) and from Aquifer, all usable offline once installed.
//!
//! - **Pins.** A project pins Door43 resources by commit ([`ResourcePin`]; the version tag
//!   is a label, D58) in tC4's §5.3 shape ([`ProjectPins`]: two language sets, the
//!   original-language texts, the lexicons and the source panes). [`ProjectPins`] reads
//!   and writes each STORAGE `Pins` record's value and tC4's `resources.json`. Aquifer
//!   assets are pinned by repository, tag, commit, language and SHA-256 ([`AquiferPin`]).
//! - **Gateway languages.** [`gateways`] holds tC4's registry of 18 (language, owner)
//!   pairs and their naming quirks, and builds a language set from what is installed;
//!   [`suite`] holds the bundled English pins; [`gateway_change`] makes the pin edits of a
//!   change of gateway language (what it would cost is `tc-check`'s count).
//! - **Resolution.** The D30 ladder is the journal's (`tc_journal::pins::Ladder`, which
//!   [`ProjectPins`] implements), the one the fold and the Check screens resolve with.
//!   [`resolve`] reads it on this computer: which rung answers a (tool, book), and what
//!   opening it requires (ready, fetch, unavailable, unpinned, not covered, an unusable
//!   pin). It also counts the pinned versions this computer lacks, and names the one a
//!   download of a repository must fetch.
//! - **Store.** [`ResourceStore`] keeps installed resources by source, owner, repository
//!   and commit, each installed atomically and never modified after, with a receipt that
//!   lets it be verified. A bundled store seeds a first run that is offline; each copy is
//!   checked once, so a repeat start opens no resource file.
//!   [`ResourceStore::seed_plan`] says from receipts alone what a first run must copy, and
//!   [`ResourceStore::seed`] does it with a [`SeedWatch`] that hears each file and can stop
//!   it.
//! - **Installs.** [`install_dcs`] downloads an exact commit through tc-door43 and
//!   [`install_aquifer`] an exact release asset from GitHub; nothing is kept unless it is
//!   the pinned bytes. A Door43 download streams to a file, and [`install_dcs_watched`]
//!   reports its bytes and can be cancelled. Zips at hand install with
//!   [`install_dcs_zip`] and [`install_aquifer_zip`]. [`FetchedLock`] holds the Door43
//!   facts recorded once for the bundled and packaged archives, so they are checked
//!   offline.
//! - **Lexicons.** A lexicon is installed as one JSON file, never as its thousands of
//!   files: [`repackage_lexicon`] turns its verified archive into that file, both when the
//!   app is packaged ([`install_lexicon_zip`], checked against [`FetchedLock`]) and when a
//!   lexicon is downloaded later ([`install_dcs`]). [`Lexicons`] reads a project's two
//!   lexicons once per session, and looks a word's `strong` attribute up in memory by
//!   tC4's rule ([`strongs`]).
//! - **Loading.** [`load_book`] and [`load_obs`] open what is installed as tc-helps
//!   [`tc_helps::Helps`] and USFM texts, and say why each missing part is missing.
//!   [`HelpsChoice`] picks Door43 or Aquifer for the helps tabs; [`check_sources`] says
//!   what a check reads, which is always Door43.
//! - **Updates.** [`updates`] finds newer releases and describes offers per set and per
//!   text, installing nothing.
//!
//! The network is reached only through tc-door43's [`tc_door43::Door43Client`] and this
//! crate's [`AquiferClient`], and only while the app's [`tc_door43::InternetSwitch`] is on
//! (D95). Loading and everything else here works offline.

mod aquifer;
mod choice;
mod dcs;
mod error;
mod fetched;
pub mod gateway_change;
pub mod gateways;
mod layout;
mod lexicon;
mod load;
mod pin;
pub mod resolve;
mod sets;
mod store;
pub mod strongs;
pub mod suite;
pub mod updates;

pub use aquifer::{
    AquiferClient, AquiferLimits, AquiferRelease, ReleaseAsset, install_aquifer,
    install_aquifer_zip, pin_aquifer_release,
};
pub use choice::{
    AquiferHelps, CheckBlocker, CheckSources, HELPS_SOURCE_SETTING, HelpsChoice, check_sources,
};
pub use dcs::{
    LexiconInstall, install_all, install_dcs, install_dcs_as, install_dcs_watched, install_dcs_zip,
    install_lexicon_zip, pin_release,
};
pub use error::{
    AquiferError, InstallError, LexiconError, LexiconUnavailable, ParseError, Refusal, StoreError,
    UnwritablePin,
};
pub use fetched::{FetchedLock, LOCK_FORMAT, LOCK_VERSION, LockEntry, TreeCheck};
pub use lexicon::{
    LEXICON_FILE, LEXICON_FORMAT, LEXICON_VERSION, Lexicon, LexiconEntry, LexiconSource, Lexicons,
    PartEntry, PartLookup, lexicon_entries_compared, open_lexicon, repackage_lexicon,
};
pub use load::{
    BookResources, Missing, ObsResources, PartStatus, SourceText, TextRole, TextStatus, load_book,
    load_obs,
};
pub use pin::{
    AQUIFER_OWNER, AquiferLanguage, AquiferPin, AquiferRepo, AssetName, BookCoverage, DCS_HOST,
    GithubName, RepoPath, ResourcePin, Sha256,
};
pub use sets::{
    GatewayLanguage, GroupError, LanguageSet, PaneId, PinGroup, ProjectKind, ProjectPins, Rung,
    ScripturePane, SetGateway, SetSlot, Testaments, UnknownKeys,
};
pub use store::{
    FileRecord, Installed, Inventory, Origin, Packaging, Receipt, ResourceStore, SeedPlan,
    SeedReport, SeedWatch, Seeding, content_files_opened, content_files_written,
};
pub use tc_journal::keys::ToolId;
pub use tc_journal::pins::{Collection, Ladder};
