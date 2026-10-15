//! Installing an exact Door43 commit (tC4 `resourceFetch.ts`, D23b, D58, D59, D71).
//!
//! A pin with a version label is downloaded as the DCS burrito export of that tag, and
//! accepted only when the export declares the pin's commit as its revision and every
//! ingredient matches its checksum. A pin by commit only (the lexicons, the picture
//! pack) is downloaded as the git archive of that commit and accepted only when its files
//! hash to the commit's tree. Either way the expected commit comes from the pin, never
//! from the download, and nothing is installed unless every check passes. tc-door43 does
//! the checks; this module adds that an export names the pinned repository, and keeps
//! out what nothing verified.
//!
//! The pin's host is not compared with the client's server: reads go to the server the
//! client was made for (production, or a test server), and the commit is the identity.

use tc_door43::{Door43Client, FileMode, TagName, UnpackedFile};
use tc_helps::metadata::parse_burrito_metadata;

use crate::error::{InstallError, Refusal};
use crate::layout;
use crate::pin::{RepoPath, ResourcePin};
use crate::store::{Installed, Origin, Packaging, ResourceStore, Staged};

impl Packaging {
    /// tC4's choice: the burrito export of the pin's tag, or the archive of a pin by
    /// commit only.
    pub fn for_pin(pin: &ResourcePin) -> Self {
        if pin.version.is_some() {
            Packaging::BurritoExport
        } else {
            Packaging::Archive
        }
    }
}

/// Turns a release tag into a pin by one Door43 tag lookup (D59): the commit Door43's tags
/// API names for the tag. A first install verifies the download against this commit, so
/// the archive never certifies itself. The pin has no flavor yet, so it cannot be written
/// to a project; the installed resource's [`Installed::dcs_pin`] carries the flavor it
/// declares.
pub async fn pin_release(
    client: &Door43Client,
    repo: &RepoPath,
    tag: &TagName,
) -> Result<ResourcePin, InstallError> {
    let commit = client.tag_commit(repo.repo(), tag).await?;
    Ok(ResourcePin::new(repo.clone(), commit).with_version(tag.clone()))
}

/// Installs the pinned commit, or returns the install the store already has without
/// touching the network. See the module documentation for the checks.
pub async fn install_dcs(
    store: &ResourceStore,
    client: &Door43Client,
    pin: &ResourcePin,
) -> Result<Installed, InstallError> {
    install_dcs_as(store, client, pin, Packaging::for_pin(pin)).await
}

/// [`install_dcs`] with the packaging chosen by the caller. [`Packaging::Archive`] gives
/// the same bytes on every download; [`Packaging::BurritoExport`] needs the pin's tag.
pub async fn install_dcs_as(
    store: &ResourceStore,
    client: &Door43Client,
    pin: &ResourcePin,
    packaging: Packaging,
) -> Result<Installed, InstallError> {
    if let Some(installed) = store.dcs(pin)? {
        return Ok(installed);
    }
    let staged = match (packaging, &pin.version) {
        (Packaging::BurritoExport, Some(tag)) => {
            let export = client
                .download_sb_export(pin.repo.repo(), tag, &pin.sha)
                .await?;
            from_export(
                pin,
                export.files,
                export.unlisted,
                export.missing_ingredients,
            )?
        }
        (Packaging::BurritoExport, None) => {
            return Err(InstallError::NoTag {
                pin: pin.to_string(),
            });
        }
        (Packaging::Archive, _) => {
            let archive = client.download_archive(pin.repo.repo(), &pin.sha).await?;
            from_archive(pin, archive.files)
        }
    };
    store.install(staged)
}

/// Installs a burrito export already at hand: a zip a person picked from a USB drive, or
/// one an installer building the bundled store downloaded earlier (tC4 `verifySideload`).
/// It is checked as a download is: its metadata must declare the pin's commit as its DCS
/// revision, every ingredient must match its checksum, and it must name the pinned
/// repository.
pub fn install_dcs_zip(
    store: &ResourceStore,
    pin: &ResourcePin,
    zip: &[u8],
    max_unpacked: u64,
) -> Result<Installed, InstallError> {
    if let Some(installed) = store.dcs(pin)? {
        return Ok(installed);
    }
    let export = tc_door43::verify_sb_export(zip, &pin.sha, max_unpacked).map_err(|e| {
        InstallError::Refused {
            what: pin.to_string(),
            refusal: Refusal::Door43(e),
        }
    })?;
    let staged = from_export(
        pin,
        export.files,
        export.unlisted,
        export.missing_ingredients,
    )?;
    store.install(staged)
}

/// Installs each pin in order, stopping at the first failure, which names its pin. A
/// release moves a project's pins only after every resource of it is installed and
/// verified (D72), so a failed download changes nothing.
pub async fn install_all(
    store: &ResourceStore,
    client: &Door43Client,
    pins: &[ResourcePin],
) -> Result<Vec<Installed>, (ResourcePin, InstallError)> {
    let mut out = Vec::new();
    for pin in pins {
        match install_dcs(store, client, pin).await {
            Ok(installed) => out.push(installed),
            Err(e) => return Err((pin.clone(), e)),
        }
    }
    Ok(out)
}

fn from_export(
    pin: &ResourcePin,
    files: Vec<UnpackedFile>,
    unlisted: Vec<UnpackedFile>,
    missing: Vec<String>,
) -> Result<Staged, InstallError> {
    let refused = |refusal| InstallError::Refused {
        what: pin.to_string(),
        refusal,
    };
    let metadata = files
        .iter()
        .find(|f| f.path == "metadata.json")
        .ok_or_else(|| {
            refused(Refusal::Unreadable(
                "the export has no metadata.json".to_owned(),
            ))
        })?;
    let text = std::str::from_utf8(&metadata.bytes)
        .map_err(|_| refused(Refusal::Unreadable("metadata.json is not UTF-8".to_owned())))?;
    let identity = parse_burrito_metadata(text)
        .map_err(|e| refused(Refusal::Unreadable(e.to_string())))?
        .identity;
    // The owner is not compared: an export names the organization at export time, which a
    // rename leaves stale (`Idiomas-Puentes/es-419_tn`).
    if let Some(repo) = &identity.repo
        && !repo.eq_ignore_ascii_case(pin.repo.name().as_str())
    {
        return Err(refused(Refusal::WrongRepository {
            expected: pin.repo.name().to_string(),
            found: repo.clone(),
        }));
    }
    let mut pin = pin.clone();
    if pin.flavor.is_none() {
        pin.flavor = identity.flavor.clone();
    }
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let books = layout::coverage_of(
        paths.iter().copied(),
        pin.flavor.as_deref(),
        pin.repo.repo(),
    );
    Ok(Staged {
        origin: Origin::Dcs {
            pin,
            packaging: Packaging::BurritoExport,
            left_out: unlisted.into_iter().map(|f| f.path).collect(),
            missing_ingredients: missing,
        },
        books,
        files: files.into_iter().map(|f| (f.path, f.bytes)).collect(),
    })
}

fn from_archive(pin: &ResourcePin, files: Vec<UnpackedFile>) -> Staged {
    let mut pin = pin.clone();
    if pin.flavor.is_none() {
        pin.flavor = files
            .iter()
            .find(|f| f.path == "metadata.json")
            .and_then(|f| std::str::from_utf8(&f.bytes).ok())
            .and_then(|text| parse_burrito_metadata(text).ok())
            .and_then(|m| m.identity.flavor);
    }
    // A symbolic link's target can point anywhere; it is never written (tc-door43
    // `FileMode::Symlink`).
    let (links, files): (Vec<UnpackedFile>, Vec<UnpackedFile>) =
        files.into_iter().partition(|f| f.mode == FileMode::Symlink);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let books = layout::coverage_of(
        paths.iter().copied(),
        pin.flavor.as_deref(),
        pin.repo.repo(),
    );
    Staged {
        origin: Origin::Dcs {
            pin,
            packaging: Packaging::Archive,
            left_out: links.into_iter().map(|f| f.path).collect(),
            missing_ingredients: Vec::new(),
        },
        books,
        files: files.into_iter().map(|f| (f.path, f.bytes)).collect(),
    }
}
