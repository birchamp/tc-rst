//! A folder of archives checked against its `fetched.lock`, offline: `bundle/en/` itself,
//! and the fixture folder `fixtures/door43/fetched-lock/` of the same shape.
// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use tc_door43::{GitOid, Limits, VerificationError};
use tc_resources::{FetchedLock, Refusal};
use zip::write::SimpleFileOptions;

const DEMO: &str = "demo-e8c6503832022a8fd38bb426781d6b6127dde664.zip";
const UGL: &str = "en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip";
const UHL: &str = "en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip";

fn fixture_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/door43/fetched-lock")
}

fn bundle_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en")
}

/// A git archive of one commit, named `<repository>-<40 hex>.zip`, as `bundle/en/` names
/// the lexicons.
fn is_commit_archive(name: &str) -> bool {
    name.strip_suffix(".zip")
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(_, sha)| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Check every archive `dir/fetched.lock` names, and that every commit archive in `dir`
/// has an entry. The error says what is wrong in words.
fn check_folder(dir: &Path) -> Result<FetchedLock, String> {
    let lock_path = dir.join("fetched.lock");
    let bytes = fs::read(&lock_path).map_err(|e| format!("{}: {e}", lock_path.display()))?;
    let lock = FetchedLock::parse(&bytes).map_err(|e| e.to_string())?;
    let max_unpacked = Limits::default().max_unpacked_bytes;
    for entry in &lock.entries {
        let Some(file) = &entry.file else { continue };
        let named = format!("{}-{}.zip", entry.repo.name, entry.commit);
        if *file != named {
            return Err(format!(
                "{file}: the entry for {} names {named}",
                entry.repo
            ));
        }
        let zip = fs::read(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let verified = entry
            .verify(&zip, max_unpacked)
            .map_err(|e| format!("{file}: {e}"))?;
        if verified.tree != entry.tree || verified.commit != entry.commit {
            return Err(format!("{file}: verified as {verified:?}"));
        }
    }
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .map(|item| item.map(|item| item.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    names.sort();
    for name in names.iter().filter(|name| is_commit_archive(name)) {
        if !lock.entries.iter().any(|e| e.file.as_deref() == Some(name)) {
            return Err(format!("{name} has no entry in fetched.lock"));
        }
    }
    Ok(lock)
}

/// A copy of the fixture folder that a test may change.
fn copy_of_fixture_folder() -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    for item in fs::read_dir(fixture_folder()).unwrap() {
        let item = item.unwrap();
        fs::copy(item.path(), copy.path().join(item.file_name())).unwrap();
    }
    copy
}

#[test]
fn the_fixture_folder_matches_its_lock() {
    let lock = check_folder(&fixture_folder()).unwrap();
    let demo = &lock.entries[0];
    assert_eq!(demo.file.as_deref(), Some(DEMO));
    assert_eq!(
        demo.tree,
        "6e5802a33cc85441708e9d4dc17c71c2b25b6cc0"
            .parse::<GitOid>()
            .unwrap(),
        "the tree git rev-parse gave when the fixture was made"
    );
    assert_eq!(lock.entries[1].file, None);
}

#[test]
fn the_bundled_lexicons_match_the_trees_door43_reported() {
    let lock = check_folder(&bundle_folder()).unwrap();
    let files: Vec<&str> = lock
        .entries
        .iter()
        .filter_map(|e| e.file.as_deref())
        .collect();
    assert_eq!(files, [UGL, UHL]);
}

#[test]
fn a_changed_byte_in_a_copy_of_a_bundled_lexicon_fails_the_check() {
    let copy = tempfile::tempdir().unwrap();
    for name in ["fetched.lock", UGL, UHL] {
        fs::copy(bundle_folder().join(name), copy.path().join(name)).unwrap();
    }
    check_folder(copy.path()).unwrap();
    let path = copy.path().join(UHL);
    let mut zip = fs::read(&path).unwrap();
    let middle = zip.len() / 2;
    zip[middle] ^= 1;
    fs::write(&path, &zip).unwrap();
    let error = check_folder(copy.path()).unwrap_err();
    assert!(
        error.starts_with(UHL) && error.contains("SHA-256"),
        "{error}"
    );
}

#[test]
fn a_changed_archive_in_the_folder_fails_the_check() {
    let copy = copy_of_fixture_folder();
    let path = copy.path().join(DEMO);
    let mut zip = fs::read(&path).unwrap();
    let at = zip.windows(6).position(|w| w == b"# demo").unwrap();
    zip[at + 2] = b'D';
    fs::write(&path, &zip).unwrap();
    let error = check_folder(copy.path()).unwrap_err();
    assert!(error.contains(DEMO) && error.contains("SHA-256"), "{error}");
}

/// The fixture archive zipped again by the zip crate, with the same entries, modes and
/// comment, and `README.md` replaced by `readme` when one is given.
fn rezipped(readme: Option<&[u8]>) -> Vec<u8> {
    let original = fs::read(fixture_folder().join(DEMO)).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(original)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().to_owned();
        let options = SimpleFileOptions::default();
        if file.is_dir() {
            writer.add_directory(name, options).unwrap();
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if let (Some(readme), "demo/README.md") = (readme, name.as_str()) {
            bytes = readme.to_vec();
        }
        let mode = file.unix_mode().unwrap_or(0o644);
        writer
            .start_file(name, options.unix_permissions(mode))
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer
        .set_comment(String::from_utf8(archive.comment().to_vec()).unwrap())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn the_tree_check_binds_the_files_not_the_zip_bytes() {
    let lock =
        FetchedLock::parse(&fs::read(fixture_folder().join("fetched.lock")).unwrap()).unwrap();
    let recorded = &lock.entries[0];
    let as_if_recorded = |zip: &[u8]| {
        let mut entry = recorded.clone();
        entry.size = zip.len() as u64;
        entry.sha256 = tc_resources::Sha256::of(zip);
        entry.verify(zip, 1 << 20)
    };

    let same_files = rezipped(None);
    assert!(matches!(
        recorded.verify(&same_files, 1 << 20),
        Err(Refusal::SizeMismatch { .. } | Refusal::Sha256Mismatch { .. })
    ));
    let verified = as_if_recorded(&same_files).unwrap();
    assert_eq!(verified.tree, recorded.tree);

    let other_files = rezipped(Some(b"# Demo\n"));
    assert!(
        matches!(
            as_if_recorded(&other_files),
            Err(Refusal::Door43(VerificationError::TreeMismatch { .. }))
        ),
        "a lock whose digest was taken from changed files still names the commit's tree"
    );
}

#[test]
fn a_commit_archive_without_an_entry_fails_the_check() {
    let copy = copy_of_fixture_folder();
    let stray = "en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip";
    fs::copy(fixture_folder().join(DEMO), copy.path().join(stray)).unwrap();
    let error = check_folder(copy.path()).unwrap_err();
    assert_eq!(error, format!("{stray} has no entry in fetched.lock"));
}

#[test]
fn an_archive_the_lock_names_but_the_folder_lacks_fails_the_check() {
    let copy = copy_of_fixture_folder();
    fs::remove_file(copy.path().join(DEMO)).unwrap();
    let error = check_folder(copy.path()).unwrap_err();
    assert!(error.starts_with(DEMO), "{error}");
}

#[test]
fn only_archives_named_by_a_full_commit_need_an_entry() {
    assert!(is_commit_archive(
        "en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip"
    ));
    for name in [
        "en_tn-v91.zip",
        "en_tn_v91.sb.zip",
        "el-x-koine_ugnt-v0.34.zip",
        "hbo_uhb-v3.0.0.zip",
        "SHA256SUMS",
        "fetched.lock",
    ] {
        assert!(!is_commit_archive(name), "{name}");
    }
}
