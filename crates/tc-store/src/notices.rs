//! The notices of an open project (`docs/STORAGE.md` §2.6): what the files and this machine
//! show that the fold does not list (a damaged or unreadable file, a held path, a second
//! writer, a disk error after a save). The person sees them with the fold's anomalies.

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
