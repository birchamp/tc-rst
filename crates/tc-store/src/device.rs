//! Device identity (`docs/STORAGE.md` §6.3): one Local device per installation writing in
//! one project folder on one machine. `.tc/device.json` names it with a fingerprint of the
//! installation secret, the machine id and the folder's canonical path, so a copied or moved
//! folder, or application data copied to another machine, gets a new device before it
//! writes anything, and two folders never write as one device.

use crate::appdata::rfc3339;
use crate::atomic::{SyncDir, atomic_create_dirs, atomic_write};
use crate::clock::Clock;
use crate::error::StoreError;
use crate::layout::{DEVICE, Folder, TC};
use crate::random;
use crate::vfs::{Vfs, is_not_found};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tc_journal::DeviceId;
use tc_journal::fold::RotationReason;

#[derive(Debug, Serialize, Deserialize)]
struct DeviceFile {
    device: String,
    fingerprint: String,
    created: String,
}

/// `sha256(installation secret ‖ machine id ‖ canonical folder path)`.
pub fn fingerprint(secret: &[u8; 32], machine_id: &str, folder: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(secret);
    hash.update(machine_id.as_bytes());
    hash.update(folder.as_bytes());
    hash.finalize().into()
}

/// The folder's device after the fingerprint check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    Kept(DeviceId),
    /// A new device: `.tc/device.json` was missing (no `old`) or its fingerprint differs.
    Minted {
        device: DeviceId,
        old: Option<DeviceId>,
        reason: RotationReason,
    },
}

impl Identity {
    pub fn device(&self) -> DeviceId {
        match self {
            Self::Kept(d) | Self::Minted { device: d, .. } => *d,
        }
    }
}

/// A new Local device.
pub fn mint() -> DeviceId {
    DeviceId::local(random::bytes::<15>())
}

/// Reads `.tc/device.json`; keeps its device when the fingerprint is this folder's here,
/// otherwise mints one and writes the file (§6.3, step O6). `read_only` never writes: a
/// read-only open keeps whatever the file names, or a device of its own in memory.
pub fn identify(
    vfs: &dyn Vfs,
    folder: &Folder,
    fingerprint: [u8; 32],
    clock: &dyn Clock,
    read_only: bool,
) -> Result<Identity, StoreError> {
    let path = folder.path(DEVICE);
    let stored = match vfs.read(&path) {
        Ok(bytes) => serde_json::from_slice::<DeviceFile>(&bytes)
            .ok()
            .and_then(|f| Some((DeviceId::parse(&f.device)?, f.fingerprint))),
        Err(e) if is_not_found(&e) => None,
        Err(e) => return Err(StoreError::io("read", &path, e)),
    };
    let wanted = random::to_hex(&fingerprint);
    if let Some((device, print)) = &stored
        && *print == wanted
    {
        return Ok(Identity::Kept(*device));
    }
    let device = mint();
    if !read_only {
        write(vfs, folder, device, &wanted, clock)?;
    }
    Ok(Identity::Minted {
        device,
        old: stored.map(|(d, _)| d),
        reason: RotationReason::FolderCopied,
    })
}

/// Writes `.tc/device.json` for `device`.
pub fn write(
    vfs: &dyn Vfs,
    folder: &Folder,
    device: DeviceId,
    fingerprint_hex: &str,
    clock: &dyn Clock,
) -> Result<(), StoreError> {
    let file = DeviceFile {
        device: device.to_string(),
        fingerprint: fingerprint_hex.to_owned(),
        created: rfc3339(clock.now()),
    };
    let bytes = serde_json::to_vec(&file).map_err(|e| StoreError::Other(e.to_string()))?;
    let dir = folder.path(TC);
    atomic_create_dirs(vfs, &dir)?;
    atomic_write(vfs, &dir, "device.json", &bytes, SyncDir::Now).map_err(|e| e.into_inner())
}
