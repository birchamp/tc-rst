//! Installing the bundled English suite from the archives of `bundle/en` into a
//! [`ResourceStore`] (APP-DESIGN §10, unit A9; `docs/TC4-DELTA-2026-10-16.md` §3.6): each
//! pinned export checked against `SHA256SUMS` and installed with [`install_dcs_zip`], each
//! lexicon against `fetched.lock` with [`install_lexicon_zip`]. The packager builds the store
//! an installer ships with it; the benchmarks and the tests of the ruling on the real exports
//! build one the same way, so there is one code path.

use crate::{
    FetchedLock, LexiconInstall, ResourcePin, ResourceStore, Sha256, install_dcs_zip,
    install_lexicon_zip, suite,
};
use std::collections::BTreeMap;
use std::path::Path;

/// What an installation of the bundle did.
#[derive(Debug, Default)]
pub struct BundleInstalled {
    /// The pins installed, by name.
    pub installed: Vec<String>,
    /// The pins left out, each with why.
    pub skipped: Vec<(String, String)>,
    /// The content files of the installs made.
    pub files: u64,
    /// The pins installed, in the order of the suite.
    pub pins: Vec<ResourcePin>,
}

/// Why the bundle did not install.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BundleError(pub String);

/// Installs into `store` the archives of `from` (`bundle/en`) for each bundled pin that
/// `wanted` keeps. A pin whose file is not in `from` (the OBS pictures, fetched when the app
/// is packaged) is skipped and named. Installing is idempotent: an install already in the
/// store is kept.
pub fn install_bundle(
    from: &Path,
    store: &ResourceStore,
    wanted: &dyn Fn(&ResourcePin) -> bool,
) -> Result<BundleInstalled, BundleError> {
    let max_unpacked = tc_door43::Limits::default().max_unpacked_bytes;
    let sums = read_sums(&from.join("SHA256SUMS"))?;
    let lock_file = from.join("fetched.lock");
    let lock_bytes = std::fs::read(&lock_file)
        .map_err(|e| BundleError(format!("{}: {e}", lock_file.display())))?;
    let lock =
        FetchedLock::parse(&lock_bytes).map_err(|e| BundleError(format!("fetched.lock: {e}")))?;
    let mut built = BundleInstalled::default();
    for pin in suite::bundled_pins().iter().filter(|pin| wanted(pin)) {
        let name = pin.to_string();
        let installed = if let Some(version) = &pin.version {
            let file = format!("{}_{}.sb.zip", pin.repo.name().as_str(), version.as_str());
            let Some(zip) = read_archive(from, &file, &mut built, &name)? else {
                continue;
            };
            let found = Sha256::of(&zip).to_string();
            match sums.get(&file) {
                Some(listed) if *listed == found => {}
                Some(listed) => {
                    return Err(BundleError(format!(
                        "{file}: its SHA-256 is {found}, SHA256SUMS says {listed}"
                    )));
                }
                None => {
                    return Err(BundleError(format!("{file}: SHA256SUMS does not list it")));
                }
            }
            install_dcs_zip(store, pin, &zip, max_unpacked)
                .map_err(|e| BundleError(format!("{name}: {e}")))?
        } else {
            // A commit-only pin: the lexicons ship as the archive `fetched.lock` names, the
            // picture pack is downloaded when the app is packaged.
            let file = lock
                .entry(pin.repo.repo(), &pin.sha)
                .and_then(|entry| entry.file.clone());
            let Some(file) = file else {
                built.skipped.push((
                    name,
                    "fetched.lock names no file: it is downloaded when the app is packaged"
                        .to_owned(),
                ));
                continue;
            };
            let Some(zip) = read_archive(from, &file, &mut built, &name)? else {
                continue;
            };
            match install_lexicon_zip(store, pin, &lock, &zip, max_unpacked)
                .map_err(|e| BundleError(format!("{name}: {e}")))?
            {
                LexiconInstall::Installed(installed) => installed,
                LexiconInstall::Unconfirmed => {
                    built.skipped.push((
                        name,
                        "fetched.lock holds no entry that confirms the archive".to_owned(),
                    ));
                    continue;
                }
            }
        };
        built.files += installed.files().count() as u64;
        built.installed.push(name);
        built.pins.push(pin.clone());
    }
    Ok(built)
}

/// The archive `file` of `from`, or `None` (and a skip) when `from` does not hold it.
fn read_archive(
    from: &Path,
    file: &str,
    built: &mut BundleInstalled,
    name: &str,
) -> Result<Option<Vec<u8>>, BundleError> {
    match std::fs::read(from.join(file)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            built.skipped.push((
                name.to_owned(),
                format!("{file} is not in {}", from.display()),
            ));
            Ok(None)
        }
        Err(e) => Err(BundleError(format!("{file}: {e}"))),
    }
}

/// `SHA256SUMS`: `<hex>  <file>` per line.
fn read_sums(path: &Path) -> Result<BTreeMap<String, String>, BundleError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| BundleError(format!("{}: {e}", path.display())))?;
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (hex, file) = line.split_once("  ").ok_or_else(|| {
            BundleError(format!(
                "SHA256SUMS: a line is not `<sha256>  <file>`: {line:?}"
            ))
        })?;
        sums.insert(file.to_owned(), hex.to_owned());
    }
    Ok(sums)
}
