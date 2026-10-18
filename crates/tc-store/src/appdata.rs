//! Application data (`docs/STORAGE.md` §3.7): per installation, never in a project.
//!
//! - `installation.json`: the installation secret, written once. Shown tokens are signed
//!   with a key derived from it (§2.7) and a device's fingerprint hashes it (§6.3).
//! - `ledger.json`: per Local device, the project, the folder, and the last sequence number
//!   and time this installation sealed (§6.3), so an older backup restored in place is
//!   noticed.

use crate::atomic::{SyncDir, atomic_create_dirs, atomic_write};
use crate::clock::Clock;
use crate::error::{IoContext, StoreError};
use crate::random;
use crate::vfs::{Vfs, is_not_found};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tc_journal::{DeviceId, ProjectId, WallTime};

const INSTALLATION: &str = "installation.json";
const LEDGER: &str = "ledger.json";

#[derive(Debug, Serialize, Deserialize)]
struct InstallationFile {
    secret: String,
    created: String,
}

/// One Local device's line in the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub project: String,
    pub folder: String,
    pub last_seq: u64,
    pub last_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LedgerFile {
    devices: BTreeMap<String, LedgerEntry>,
}

/// The application data folder of this installation.
#[derive(Debug)]
pub struct AppData {
    vfs: Arc<dyn Vfs>,
    root: PathBuf,
    secret: [u8; 32],
    /// One read-modify-write of the ledger at a time: several projects may be open.
    ledger: Mutex<()>,
}

/// RFC 3339 with milliseconds and `Z`, as `metadata.json` and `project.json` write times.
pub fn rfc3339(at: WallTime) -> String {
    let ms = at.0.rem_euclid(1000);
    let secs = at.0.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A compact UTC stamp for file names: `20261017T120000Z`.
pub fn utc_stamp(at: WallTime) -> String {
    let text = rfc3339(at);
    text.chars()
        .filter(|c| c.is_ascii_digit() || *c == 'T')
        .take(15)
        .collect::<String>()
        + "Z"
}

impl AppData {
    /// Opens the application data at `root`, making the installation secret the first time.
    /// A damaged `installation.json` is moved aside and a new secret made: a lost secret
    /// only means new devices and tokens that no longer verify, never lost work (§15.1
    /// L17).
    pub fn open(vfs: Arc<dyn Vfs>, root: &Path, clock: &dyn Clock) -> Result<Self, StoreError> {
        atomic_create_dirs(&*vfs, root)?;
        let path = root.join(INSTALLATION);
        let secret = match vfs.read(&path) {
            Ok(bytes) => match parse_installation(&bytes) {
                Some(secret) => Some(secret),
                None => {
                    let aside =
                        root.join(format!("{INSTALLATION}.damaged-{}", utc_stamp(clock.now())));
                    vfs.rename(&path, &aside).at("move aside", &path)?;
                    None
                }
            },
            Err(e) if is_not_found(&e) => None,
            Err(e) => return Err(StoreError::io("read", &path, e)),
        };
        let secret = match secret {
            Some(secret) => secret,
            None => {
                let secret = random::secret::<32>().map_err(StoreError::Other)?;
                let file = InstallationFile {
                    secret: random::to_hex(&secret),
                    created: rfc3339(clock.now()),
                };
                let bytes =
                    serde_json::to_vec(&file).map_err(|e| StoreError::Other(e.to_string()))?;
                atomic_write(&*vfs, root, INSTALLATION, &bytes, SyncDir::Now)
                    .map_err(|e| e.into_inner())?;
                secret
            }
        };
        Ok(Self {
            vfs,
            root: root.to_owned(),
            secret,
            ledger: Mutex::new(()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn secret(&self) -> &[u8; 32] {
        &self.secret
    }

    fn read_ledger(&self) -> LedgerFile {
        self.vfs
            .read(&self.root.join(LEDGER))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// What the ledger says this installation sealed as `device`.
    pub fn ledger_entry(&self, device: DeviceId) -> Option<LedgerEntry> {
        let _guard = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        self.read_ledger().devices.get(&device.to_string()).cloned()
    }

    /// Records the last sequence number and time `device` sealed (§5.11 K5).
    pub fn record_ledger(
        &self,
        device: DeviceId,
        project: ProjectId,
        folder: &Path,
        last_seq: u64,
        last_at: WallTime,
    ) -> Result<(), StoreError> {
        let _guard = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let mut ledger = self.read_ledger();
        let entry = ledger
            .devices
            .entry(device.to_string())
            .or_insert_with(|| LedgerEntry {
                project: project.to_string(),
                folder: folder.to_string_lossy().into_owned(),
                last_seq: 0,
                last_at: 0,
            });
        if entry.last_seq >= last_seq && entry.last_at >= last_at.0 {
            return Ok(());
        }
        entry.last_seq = entry.last_seq.max(last_seq);
        entry.last_at = entry.last_at.max(last_at.0);
        entry.folder = folder.to_string_lossy().into_owned();
        let bytes = serde_json::to_vec(&ledger).map_err(|e| StoreError::Other(e.to_string()))?;
        atomic_write(&*self.vfs, &self.root, LEDGER, &bytes, SyncDir::Now)
            .map_err(|e| e.into_inner())
    }
}

fn parse_installation(bytes: &[u8]) -> Option<[u8; 32]> {
    let file: InstallationFile = serde_json::from_slice(bytes).ok()?;
    let text = file.secret.as_bytes();
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in text.chunks(2).enumerate() {
        let digit = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        out[i] = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::fault::FaultFs;

    #[test]
    fn the_secret_is_made_once() {
        let fs = Arc::new(FaultFs::new());
        let clock = ManualClock::at(1_760_000_000_000);
        let first = AppData::open(fs.clone(), Path::new("/app"), &clock).unwrap();
        let second = AppData::open(fs.clone(), Path::new("/app"), &clock).unwrap();
        assert_eq!(first.secret(), second.secret());
        fs.put(Path::new("/app/installation.json"), b"{broken", None);
        let third = AppData::open(fs.clone(), Path::new("/app"), &clock).unwrap();
        assert_ne!(third.secret(), first.secret());
        assert!(
            fs.files_under(Path::new("/app"))
                .keys()
                .any(|p| p.to_string_lossy().contains("damaged"))
        );
    }

    #[test]
    fn times_are_rfc3339_utc() {
        assert_eq!(rfc3339(WallTime(0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            rfc3339(WallTime(1_760_520_000_123)),
            "2025-10-15T09:20:00.123Z"
        );
        assert_eq!(utc_stamp(WallTime(1_760_520_000_123)), "20251015T092000Z");
    }
}
