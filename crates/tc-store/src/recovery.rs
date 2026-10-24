//! Recovery copies never replace another image, even when a cloud client reuses a name.

use crate::atomic::{SyncDir, WriteError, atomic_create, atomic_create_dirs};
use crate::error::StoreError;
use crate::vfs::{Vfs, is_not_found};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The full image identity includes damaged/unverified tails, not just verified lines.
pub(crate) fn fingerprint(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    crate::random::to_hex(&digest)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Source,
    Missing,
    Identical,
    Different,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Skip,
    Create,
    Flush,
}

fn next(target: Target) -> Action {
    match target {
        Target::Source | Target::Different | Target::Unreadable => Action::Skip,
        Target::Missing => Action::Create,
        Target::Identical => Action::Flush,
    }
}

/// Returns a durable name for these exact bytes. The excluded source cannot stand for
/// its own independent replacement. Every occupied different/unreadable name stays intact.
pub(crate) fn preserve_bytes(
    vfs: &dyn Vfs,
    dir: &Path,
    base: &str,
    bytes: &[u8],
    excluded: Option<&Path>,
) -> Result<String, StoreError> {
    atomic_create_dirs(vfs, dir)?;
    for number in 1..1000 {
        let name = format!("{base}~{number}");
        let path = dir.join(&name);
        let target = if excluded == Some(path.as_path()) {
            Target::Source
        } else {
            match vfs.read(&path) {
                Ok(found) if found == bytes => Target::Identical,
                Ok(_) => Target::Different,
                Err(e) if is_not_found(&e) => Target::Missing,
                Err(_) => Target::Unreadable,
            }
        };
        match next(target) {
            Action::Skip => continue,
            Action::Create => match atomic_create(vfs, dir, &name, bytes, SyncDir::Now) {
                Ok(()) => return Ok(name),
                Err(WriteError::NotWritten(e))
                    if e.kind() == Some(std::io::ErrorKind::AlreadyExists) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into_inner()),
            },
            Action::Flush => {
                vfs.sync_file(&path)
                    .and_then(|()| vfs.sync_dir(dir))
                    .and_then(|()| vfs.sync_file(&path))
                    .map_err(|e| StoreError::io("flush the recovery copy", &path, e))?;
                return Ok(name);
            }
        }
    }
    Err(StoreError::Other(format!(
        "could not preserve {base}: all recovery copy names are occupied"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::FaultFs;

    #[test]
    fn every_target_kind_preserves_the_independent_image_rule() {
        for (target, expected) in [
            (Target::Source, Action::Skip),
            (Target::Missing, Action::Create),
            (Target::Identical, Action::Flush),
            (Target::Different, Action::Skip),
            (Target::Unreadable, Action::Skip),
        ] {
            assert_eq!(next(target), expected);
        }
    }

    #[test]
    fn occupied_and_excluded_images_are_kept_and_equal_copies_are_durable() {
        let fs = FaultFs::new();
        let dir = Path::new("/p/copies");
        fs.put(&dir.join("save~1"), b"damaged original", None);
        fs.put(&dir.join("save~2"), b"another person's image", None);
        let name = preserve_bytes(
            &fs,
            dir,
            "save",
            b"verified lines",
            Some(&dir.join("save~1")),
        )
        .unwrap();
        assert_eq!(name, "save~3");
        assert_eq!(fs.peek(&dir.join("save~1")).unwrap(), b"damaged original");
        assert_eq!(
            fs.peek(&dir.join("save~2")).unwrap(),
            b"another person's image"
        );
        assert_eq!(
            preserve_bytes(&fs, dir, "save", b"verified lines", None).unwrap(),
            name
        );
        assert_eq!(
            fs.crash(true).peek(&dir.join(name)).unwrap(),
            b"verified lines"
        );
    }
}
