//! The notices of an open project (`docs/STORAGE.md` §2.6): what the files and this machine
//! show that the fold does not list (a damaged or unreadable file, a held path, a second
//! writer, a disk error after a save). The person sees them with the fold's anomalies.

use crate::layout::Folder;
use crate::vfs::{EntryKind, Vfs};
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;
use tc_journal::fold::Notice;

/// One listed notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreNotice {
    pub id: u64,
    pub notice: Notice,
    /// The byte copy the notice keeps (`.tc/external/…`, `.tc/quarantine/…`), removed when
    /// the person dismisses it (§3.6).
    pub copy: Option<String>,
}

/// A listed notice with the identity of the copy it was raised for. Only that identity
/// authorizes removing the copy: a name alone could now hold someone else's bytes.
#[derive(Debug, Clone)]
pub(crate) struct RetainedNotice {
    pub visible: StoreNotice,
    pub identity: CopyIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyIdentity {
    NoCopy,
    /// The copy could not be read when the notice was raised. A later successful read
    /// cannot stand for the bytes the notice was about, so it never authorizes removal.
    Unavailable,
    Verified(CopyDigest),
}

impl CopyIdentity {
    pub(crate) fn capture(vfs: &dyn Vfs, folder: &Folder, copy: Option<&str>) -> Self {
        match copy {
            None => Self::NoCopy,
            Some(copy) => match CopyDigest::read(vfs, &folder.path(copy)) {
                Ok(digest) => Self::Verified(digest),
                Err(_) => Self::Unavailable,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CopyDigest {
    size: u64,
    sha256: [u8; 32],
}

impl CopyDigest {
    /// A copy may be a large carried file: only one chunk is in memory at a time.
    pub(crate) fn read(vfs: &dyn Vfs, path: &Path) -> io::Result<Self> {
        let before = vfs.metadata(path)?;
        if before.kind != EntryKind::File {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the copy is no longer a regular file",
            ));
        }
        let file = vfs.open(path)?;
        let size = file.len();
        if size != before.len {
            return Err(io::Error::other("the copy changed while it was opened"));
        }
        let mut digest = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut at = 0u64;
        while at < size {
            let n =
                usize::try_from((size - at).min(buffer.len() as u64)).map_err(io::Error::other)?;
            file.read_at(&mut buffer[..n], at)?;
            digest.update(&buffer[..n]);
            at += n as u64;
        }
        if vfs.metadata(path)? != before {
            return Err(io::Error::other("the copy changed while it was read"));
        }
        Ok(Self {
            size,
            sha256: digest.finalize().into(),
        })
    }
}
