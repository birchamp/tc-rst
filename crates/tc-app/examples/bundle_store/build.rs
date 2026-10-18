//! Builds the bundled store an installer ships (APP-DESIGN §10, unit A9; DELTA §3.6 item 3):
//! the pinned English suite of `bundle/en`, each export checked against `SHA256SUMS` and
//! installed with tc-resources' own installers, each lexicon against `fetched.lock`. `tc4
//! serve --bundle <dir>` copies this store into the machine's at a first launch.
//!
//! The example's `main` and the tests of tc-app and tc4 share this file by path, so it holds no
//! `main` and reads nothing but the folders it is given.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use tc_app::bundle::BundleManifest;
use tc_resources::{
    FetchedLock, LexiconInstall, ResourcePin, ResourceStore, Sha256, install_dcs_zip,
    install_lexicon_zip, suite,
};

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
/// bundled pin that `wanted` keeps. A pin whose file is not in `from` (the OBS pictures,
/// fetched when the app is packaged) is skipped and named. Installing is idempotent: an
/// install already in `out` is kept. The build ends by writing `BUNDLE-MANIFEST.json`, naming
/// the installs this run made or kept: the list a start compares the folders with.
pub fn build_bundle(
    from: &Path,
    out: &Path,
    wanted: &dyn Fn(&ResourcePin) -> bool,
) -> Result<Built, String> {
    let max_unpacked = tc_door43::Limits::default().max_unpacked_bytes;
    let sums = read_sums(&from.join("SHA256SUMS"))?;
    let lock_file = from.join("fetched.lock");
    let lock_bytes = fs::read(&lock_file).map_err(|e| format!("{}: {e}", lock_file.display()))?;
    let lock = FetchedLock::parse(&lock_bytes).map_err(|e| format!("fetched.lock: {e}"))?;
    let store = ResourceStore::open(out).map_err(|e| e.to_string())?;
    let mut built = Built::default();
    let mut promised = Vec::new();

    for pin in suite::bundled_pins().iter().filter(|pin| wanted(pin)) {
        let name = pin.to_string();
        let installed = if pin.version.is_some() {
            let Some(version) = &pin.version else {
                continue;
            };
            let file = format!("{}_{}.sb.zip", pin.repo.name().as_str(), version.as_str());
            let Some(zip) = read_archive(from, &file, &mut built, &name)? else {
                continue;
            };
            let found = Sha256::of(&zip).to_string();
            match sums.get(&file) {
                Some(listed) if *listed == found => {}
                Some(listed) => {
                    return Err(format!(
                        "{file}: its SHA-256 is {found}, SHA256SUMS says {listed}"
                    ));
                }
                None => return Err(format!("{file}: SHA256SUMS does not list it")),
            }
            install_dcs_zip(&store, pin, &zip, max_unpacked).map_err(|e| format!("{name}: {e}"))?
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
            match install_lexicon_zip(&store, pin, &lock, &zip, max_unpacked)
                .map_err(|e| format!("{name}: {e}"))?
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
        promised.push(pin.clone());
    }
    // The list the start compares the folders with, so an install that goes missing later
    // stops the start.
    BundleManifest::new(promised)
        .write(out)
        .map_err(|e| e.to_string())?;
    Ok(built)
}

/// The archive `file` of `from`, or `None` (and a skip) when `from` does not hold it.
fn read_archive(
    from: &Path,
    file: &str,
    built: &mut Built,
    name: &str,
) -> Result<Option<Vec<u8>>, String> {
    match fs::read(from.join(file)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            built.skipped.push((
                name.to_owned(),
                format!("{file} is not in {}", from.display()),
            ));
            Ok(None)
        }
        Err(e) => Err(format!("{file}: {e}")),
    }
}

/// `SHA256SUMS`: `<hex>  <file>` per line.
fn read_sums(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (hex, file) = line
            .split_once("  ")
            .ok_or_else(|| format!("SHA256SUMS: a line is not `<sha256>  <file>`: {line:?}"))?;
        sums.insert(file.to_owned(), hex.to_owned());
    }
    Ok(sums)
}
