//! Builds the bundled store an installer ships (APP-DESIGN §10, unit A9; DELTA §3.6 item 3):
//! the pinned English suite of `bundle/en`, each export checked against `SHA256SUMS` and
//! installed with tc-resources' own installers, each lexicon against `fetched.lock`. `tc4
//! serve --bundle <dir>` copies this store into the machine's at a first launch.
//!
//! The example's `main` and the tests of tc-app and tc4 share this file by path, so it holds no
//! `main` and reads nothing but the folders it is given.

use std::path::Path;

use tc_app::bundle::BundleManifest;
use tc_resources::{ResourcePin, ResourceStore, install_bundle};

/// What a build of the bundled store did.
#[derive(Debug, Default)]
pub struct Built {
    /// The pins installed, by name.
    pub installed: Vec<String>,
    /// The pins left out, each with why.
    pub skipped: Vec<(String, String)>,
    /// The content files of the installs made.
    pub files: u64,
}

/// Builds the bundled store in `out` from the archives in `from` (`bundle/en`), for each
/// bundled pin that `wanted` keeps (`tc_resources::install_bundle`). A pin whose file is not
/// in `from` (the OBS pictures, fetched when the app is packaged) is skipped and named.
/// Installing is idempotent: an install already in `out` is kept. The build ends by writing
/// `BUNDLE-MANIFEST.json`, naming the installs this run made or kept: the list a start
/// compares the folders with.
pub fn build_bundle(
    from: &Path,
    out: &Path,
    wanted: &dyn Fn(&ResourcePin) -> bool,
) -> Result<Built, String> {
    let store = ResourceStore::open(out).map_err(|e| e.to_string())?;
    let installed = install_bundle(from, &store, wanted).map_err(|e| e.to_string())?;
    BundleManifest::new(installed.pins)
        .write(out)
        .map_err(|e| e.to_string())?;
    Ok(Built {
        installed: installed.installed,
        skipped: installed.skipped,
        files: installed.files,
    })
}
