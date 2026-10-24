//! Pending batch files (`docs/STORAGE.md` §4.2): `.tc/pending/<device>-<hash32>.batch`,
//! one atomic file per save, outside the burrito.
//!
//! ```text
//! {"device":"d…","pending":1,"project":"6f1c…","purpose":"save"}<TAB><sha256>
//! <change line>
//! …
//! ```
//!
//! Every file in `.tc/pending/` is read, whatever its name: a file is identified by its
//! content, never by its name.

use crate::canon::{HeaderReader, header_object, to_json};
use crate::change::Verified;
use crate::ids::{ChangeId, DeviceId, DeviceKind, Md5, ProjectId, RepoPath};
use crate::json::{self, Json};
use crate::line::{RawLine, Span, encode_line, file_hash, frame, split_lines, verify_line};
use crate::pack::{
    NotOurs, PackError, VerifiedLine, not_ours, parse_entries, salvage, sorted_ids, verified_lines,
};
use crate::text::hex;

/// The pending-file framing version this app reads and writes.
pub const PENDING_FORMAT: u64 = 1;

/// Why the changes of a pending file were written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// A command's batch (`commit_batch`).
    Save,
    /// Changes recorded by an intake, such as ingested Door43 edits (`commit_received`).
    Intake,
    /// Changes recovered from a damaged pack or pending file (`commit_received`).
    Salvage,
}

impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Intake => "intake",
            Self::Salvage => "salvage",
        }
    }
}

/// The outside edit an intake batch records, so recording the same file twice is a no-op
/// (§11.1 X4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reconciles {
    pub path: RepoPath,
    pub seen_md5: Md5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingHeader {
    /// The Local device that wrote the file.
    pub device: DeviceId,
    pub project: ProjectId,
    pub purpose: Purpose,
    pub reconciles: Option<Reconciles>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingStatus {
    /// The header and every line verify: fold and seal it.
    Valid,
    /// Quarantine the file and salvage its verified lines into a new pending file.
    Damaged(PackError),
    /// A pending file from a newer format: kept, not read, never removed.
    Newer { pending: u64 },
    /// The header names another project (a `.tc/` copied between projects): moved to
    /// quarantine unread and reported.
    Foreign { project: ProjectId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRead {
    pub header: Option<PendingHeader>,
    pub status: PendingStatus,
    /// Every line of a valid file, or the salvaged lines of a damaged one.
    pub entries: Vec<Verified>,
    /// Where each entry's JSON lies in the file: `spans[i]` is `entries[i]`'s.
    pub spans: Vec<Span>,
}

/// What the open's rules (§6.1 O7: a pending file all of whose changes a valid pack holds is
/// removed; §11.1 X4) and notices read of a pending file: everything a read gives except the
/// changes themselves. Made from a read ([`PendingFacts::of`]) or from the fold cache's
/// section of the same bytes ([`crate::index::Section::load`]), which give the same facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFacts {
    pub header: Option<PendingHeader>,
    pub status: PendingStatus,
    /// The id of every line read (a damaged file's salvaged lines), sorted, without repeats.
    pub ids: Vec<ChangeId>,
}

impl PendingFacts {
    pub fn of(read: &PendingRead) -> Self {
        Self::with_ids(read, read.entries.iter().map(Verified::id))
    }

    /// The facts of `read`, whose entries have the ids `ids`.
    pub(crate) fn with_ids(read: &PendingRead, ids: impl Iterator<Item = ChangeId>) -> Self {
        Self {
            header: read.header.clone(),
            status: read.status.clone(),
            ids: sorted_ids(ids),
        }
    }
}

/// Reads a pending file of project `project`. Never fails.
pub fn read_pending(bytes: &[u8], project: ProjectId) -> PendingRead {
    let (mut read, lines) = read_pending_lines(bytes, project);
    (read.entries, read.spans) = parse_entries(lines);
    read
}

/// [`read_pending`] without its entries: the read, and its verified (or salvaged) lines in
/// file order, still to parse (see [`crate::pack::read_pack_lines`]).
pub(crate) fn read_pending_lines(
    bytes: &[u8],
    project: ProjectId,
) -> (PendingRead, Vec<VerifiedLine<'_>>) {
    let lines = split_lines(bytes);
    let mut read = PendingRead {
        header: None,
        status: PendingStatus::Valid,
        entries: Vec::new(),
        spans: Vec::new(),
    };
    match not_ours(lines.first(), "pending", PENDING_FORMAT, project) {
        Some(NotOurs::Newer(pending)) => {
            read.status = PendingStatus::Newer { pending };
            return (read, Vec::new());
        }
        Some(NotOurs::Foreign(owner)) => {
            read.status = PendingStatus::Foreign { project: owner };
            return (read, Vec::new());
        }
        None => {}
    }
    let body = match lines.first().map(parse_header) {
        None => Err(PackError::Empty),
        Some(Err(error)) => Err(error),
        Some(Ok(header)) => {
            read.header = Some(header);
            verified_lines(&lines[1..])
        }
    };
    let entries = match body {
        Ok(entries) => entries,
        Err(error) => {
            read.status = PendingStatus::Damaged(error);
            salvage(&lines)
        }
    };
    (read, entries)
}

fn parse_header(line: &RawLine<'_>) -> Result<PendingHeader, PackError> {
    let (_, json) = verify_line(line.bytes).ok_or(PackError::HeaderUnverified)?;
    if !line.terminated {
        return Err(PackError::Truncated);
    }
    let value = json::parse(json).map_err(|e| PackError::HeaderMalformed(e.to_string().into()))?;
    let malformed = |text: String| PackError::HeaderMalformed(text.into());
    let mut o = HeaderReader::new(&value).map_err(malformed)?;
    let version: u64 = o.req("pending").map_err(malformed)?;
    if version != PENDING_FORMAT {
        return Err(malformed(format!("pending {version}")));
    }
    let device: DeviceId = o.req("device").map_err(malformed)?;
    if device.kind() != DeviceKind::Local {
        return Err(malformed("the writer is not a Local device".into()));
    }
    let purpose = match o.req::<String>("purpose").map_err(malformed)?.as_str() {
        "save" => Purpose::Save,
        "intake" => Purpose::Intake,
        "salvage" => Purpose::Salvage,
        other => return Err(malformed(format!("unknown purpose {other:?}"))),
    };
    let reconciles = match value.as_obj().and_then(|map| map.get("reconciles")) {
        None => None,
        Some(inner) => {
            let mut r = HeaderReader::new(inner).map_err(malformed)?;
            Some(Reconciles {
                path: r.req_source_path("path").map_err(malformed)?,
                seen_md5: r.req("seenMd5").map_err(malformed)?,
            })
        }
    };
    Ok(PendingHeader {
        device,
        project: o.req("project").map_err(malformed)?,
        purpose,
        reconciles,
    })
}

/// A pending file ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedPending {
    /// `<device>-<hash32>.batch`: the writer and the first 32 hex digits of the hash of the
    /// header line and the line ids (§4.2), which covers every byte of the file, so no other
    /// file can ever have this name.
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PendingWriteError {
    #[error("only a Local device writes pending files")]
    NotLocal,
    #[error("a pending file holds at least one change")]
    Empty,
}

/// Frames `lines` exactly as given (the order of the batch, raw bytes never re-serialized)
/// under a header.
pub fn encode_pending(
    header: &PendingHeader,
    lines: &[Verified],
) -> Result<EncodedPending, PendingWriteError> {
    if header.device.kind() != DeviceKind::Local {
        return Err(PendingWriteError::NotLocal);
    }
    if lines.is_empty() {
        return Err(PendingWriteError::Empty);
    }
    let mut fields = vec![
        ("device", to_json(&header.device)),
        ("pending", Json::from(PENDING_FORMAT)),
        ("project", to_json(&header.project)),
        ("purpose", Json::from(header.purpose.name())),
    ];
    if let Some(reconciles) = &header.reconciles {
        fields.push((
            "reconciles",
            header_object(vec![
                ("path", Json::from(reconciles.path.as_str())),
                ("seenMd5", Json::from(reconciles.seen_md5.to_hex())),
            ]),
        ));
    }
    let mut bytes = frame(&header_object(fields).to_canonical());
    let hash = file_hash(&bytes, lines.iter().map(Verified::id));
    for line in lines {
        bytes.extend_from_slice(&encode_line(line.id(), line.raw()));
    }
    Ok(EncodedPending {
        name: format!("{}-{}.batch", header.device, &hex(&hash)[..32]),
        bytes,
    })
}
