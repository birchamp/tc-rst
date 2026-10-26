//! USB exchange (unit E2; STORAGE §10, APP-DESIGN "E2 USB exchange"): "Save work for a
//! teammate" writes an exchange file into a folder the person chose and verifies it from the
//! device before it says "You can remove the stick"; "Receive work from a file" names the
//! file's project, asks to confirm, and takes the file's packs in whole or not at all. A
//! receive never runs beside a sync of the same project (A-31).

use serde::{Deserialize, Serialize};

use crate::door43::ProjectName;
use crate::ids::{ProjectRef, RequestId, UploadId};
use crate::shown::PersonLabel;

/// Where an exchange file is: a path the shell's `chooseFile()` gave, read in place, or an
/// upload a browser sent (`UploadFlow::Exchange`, up to 4 GiB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ExchangeFile {
    Path(String),
    Upload(UploadId),
}

/// `exchange/identify`: what project an exchange file holds, before the person confirms the
/// receive. Nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identify {
    pub file: ExchangeFile,
}

/// The file's project: its name as the file states it, and the project on this computer with
/// the same id, when there is one. A file of a project this computer lacks is received into
/// no project (STORAGE §10.2 R2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct IdentifyView {
    pub name: String,
    pub project: Option<ProjectRef>,
}

/// `exchange/export` (job `ExchangeExport`): checkpoints `project`, writes the exchange file
/// into `destination` and reads it back from the device (STORAGE §10.1 E1-E5). The job ends
/// with `JobOutcome::Exported` naming the file, or fails with `Refused(Exchange(..))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Export {
    pub request: RequestId,
    pub project: ProjectRef,
    pub destination: String,
}

/// The exchange file written and verified: only now may the stick be removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ExportedView {
    pub path: String,
}

/// `exchange/receive` (job `ExchangeReceive`): takes the file's packs into `project`, whole or
/// not at all (STORAGE §10.2 R1-R8). The job ends with `JobOutcome::Received`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receive {
    pub request: RequestId,
    pub project: ProjectRef,
    pub file: ExchangeFile,
}

/// What a receive brought and left, as the Received sheet shows it (the counts as
/// `SyncResultView` has them), and the files inside the exchange file that were edited by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ReceivedView {
    /// The teammates' changes this receive took in.
    pub received: u32,
    /// Who made them, each person once, named as the conflict review names them.
    pub from: Vec<PersonLabel>,
    /// Open conflicts of kinds other than a verse's text and alignments.
    pub kept_newest: u32,
    pub verse_conflicts: u32,
    pub alignments: u32,
    pub hand_edited: Vec<HandEdited>,
}

/// A derived file inside the exchange file that its own packs do not make: someone edited it
/// by hand. The app's own record is the project, so it was not taken in and nothing was kept;
/// the file on the stick still has it (`rulings/2026-10-22-H22.md` part 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HandEdited {
    pub path: String,
    pub outcome: HandEditOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum HandEditOutcome {
    NotTakenIn,
}

/// `exchange/receiveNew` (job `ExchangeReceive`): a file for a project this computer does not
/// have yet becomes the project `name`, built from the file's packs in a staging folder and
/// placed whole or not at all (STORAGE §10.2; a new teammate on USB starts here). The name is
/// checked against the projects on this computer first, without letter case, and a name in use
/// is refused before anything is written. The job ends with `JobOutcome::Created`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiveNew {
    pub request: RequestId,
    pub file: ExchangeFile,
    pub name: ProjectName,
}
