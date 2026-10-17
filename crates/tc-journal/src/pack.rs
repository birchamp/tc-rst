//! Packs (`docs/STORAGE.md` §4.3, §4.4, §4.6): immutable NDJSON files of change lines under
//! `ingredients/tc/changes/<sealer>/<yyyy-mm>/<pack32>.ndjson`.
//!
//! ```text
//! {"count":N,"project":"…","replaces":[],"sealed":1760520000000,"sealer":"d…","tcpack":1}<TAB><sha256>
//! <change line>                                             N lines, sorted by (device, seq, id)
//! ```
//!
//! [`read_pack`] validates a pack (P1 to P4) and salvages a damaged one line by line;
//! [`seal`] and [`encode_pack`] write packs of at most [`MAX_PACK_BYTES`].

use crate::canon::{HeaderReader, header_object, to_json};
use crate::change::Verified;
use crate::change::parse_change;
use crate::ids::{ChangeId, DeviceId, DeviceKind, PackId, PackName, ProjectId, WallTime};
use crate::json::{self, Json, JsonInt};
use crate::line::{
    LINE_OVERHEAD, RawLine, Span, encode_line, frame, split_lines, verify_line, verify_salvaged,
};
use std::fmt;
use std::sync::Arc;

/// The pack framing version this app reads and writes (`tcpack`).
pub const PACK_FORMAT: u64 = 1;
/// The largest pack a seal or a consolidation writes (§4.6).
pub const MAX_PACK_BYTES: usize = 64 * 1024 * 1024;
/// Where packs live, relative to the repository root.
pub const PACKS_DIR: &str = "ingredients/tc/changes";

// ---------------------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------------------

/// A UTC month, `0000-01` to `9999-12`: the directory of the packs sealed in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    year: u16,
    month: u8,
}

impl YearMonth {
    pub fn new(year: u16, month: u8) -> Option<Self> {
        (year <= 9999 && (1..=12).contains(&month)).then_some(Self { year, month })
    }

    /// The UTC month of `at`. A clock outside the years 0 to 9999 is clamped to them, so a
    /// wrong clock never makes a pack path invalid.
    pub fn of(at: WallTime) -> Self {
        let days = at.0.div_euclid(86_400_000);
        let (year, month) = civil_from_days(days);
        if year < 0 {
            Self { year: 0, month: 1 }
        } else if year > 9999 {
            Self {
                year: 9999,
                month: 12,
            }
        } else {
            Self {
                year: u16::try_from(year).unwrap_or(9999),
                month,
            }
        }
    }

    pub fn year(self) -> u16 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    /// Exactly `yyyy-mm`.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 7 || bytes[4] != b'-' || !text[..4].bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if !text[5..].bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Self::new(text[..4].parse().ok()?, text[5..].parse().ok()?)
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

/// Year and month of a day count since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, u8::try_from(month).unwrap_or(1))
}

/// A pack's path: `ingredients/tc/changes/<sealer>/<yyyy-mm>/<pack32>.ndjson` (rule P1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackPath {
    pub sealer: DeviceId,
    pub month: YearMonth,
    pub name: PackName,
}

impl PackPath {
    pub fn parse(path: &str) -> Option<Self> {
        let rest = path.strip_prefix(PACKS_DIR)?.strip_prefix('/')?;
        let mut parts = rest.split('/');
        let sealer = DeviceId::parse(parts.next()?)?;
        let month = YearMonth::parse(parts.next()?)?;
        let name = PackName::parse(parts.next()?.strip_suffix(".ndjson")?)?;
        if parts.next().is_some() || sealer.kind() != DeviceKind::Local {
            return None;
        }
        Some(Self {
            sealer,
            month,
            name,
        })
    }
}

impl fmt::Display for PackPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{PACKS_DIR}/{}/{}/{}.ndjson",
            self.sealer, self.month, self.name
        )
    }
}

// ---------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------

/// A pack header as tcpack 1 defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackHeader {
    pub count: u32,
    pub project: ProjectId,
    /// Names of the packs this one consolidated, and every name they listed. Sorted, no
    /// repeats.
    pub replaces: Vec<PackName>,
    pub sealed: WallTime,
    pub sealer: DeviceId,
}

/// Why a pack or a pending file is damaged. The first defect found is reported.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, thiserror::Error)]
pub enum PackError {
    #[error("the path is not ingredients/tc/changes/<device>/<yyyy-mm>/<32 hex>.ndjson")]
    BadPath,
    #[error("the hash of the file's header and line ids does not start with its name")]
    NameMismatch,
    #[error("the file is empty")]
    Empty,
    #[error("the header line does not verify")]
    HeaderUnverified,
    #[error("the header does not parse: {0}")]
    HeaderMalformed(Box<str>),
    #[error("the header names sealer {header}, but the file is in the folder of {folder}")]
    SealerMismatch { header: DeviceId, folder: DeviceId },
    #[error("the header says {expected} changes, but {found} lines follow")]
    CountMismatch { expected: u64, found: u64 },
    #[error("line {line} does not verify")]
    LineUnverified { line: u64 },
    #[error("the file does not end with a line break")]
    Truncated,
}

/// What reading a pack found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackStatus {
    /// P1 to P4 pass. Every line is read; a line that breaks a per-change rule is a
    /// quarantined change and the rest of the pack is used.
    Valid,
    /// The pack is damaged: its lines that verify are salvaged.
    Damaged(PackError),
    /// A pack from a newer format (`tcpack` above 1): carried, not read, never "damaged".
    Newer { tcpack: u64 },
    /// A pack of another project: not read, reported.
    Foreign { project: ProjectId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRead {
    /// The pack's id (§4.3) when its header and every line verify (P3, P4), wherever the
    /// file lies; otherwise SHA-256 of all its bytes.
    pub id: PackId,
    /// `None` when the path breaks P1.
    pub path: Option<PackPath>,
    /// The header, when it verified and parsed.
    pub header: Option<PackHeader>,
    pub status: PackStatus,
    /// Every line of a valid pack, or the salvaged lines of a damaged one.
    pub entries: Vec<Verified>,
    /// Where each entry's JSON lies in the file: `spans[i]` is `entries[i]`'s.
    pub spans: Vec<Span>,
}

impl PackRead {
    pub fn is_valid(&self) -> bool {
        self.status == PackStatus::Valid
    }
}

/// What the open's rules (§6.1 O7, the redundancy rule of §4.7) and notices read of a pack:
/// everything a read gives except the changes themselves. Made from a read ([`PackFacts::of`])
/// or from the fold cache's section of the same bytes ([`crate::index::Section::load`]), which
/// give the same facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFacts {
    pub id: PackId,
    pub path: Option<PackPath>,
    pub header: Option<PackHeader>,
    pub status: PackStatus,
    /// The id of every line read (a damaged pack's salvaged lines), sorted, without repeats.
    pub ids: Vec<ChangeId>,
}

impl PackFacts {
    pub fn of(read: &PackRead) -> Self {
        Self {
            id: read.id,
            path: read.path,
            header: read.header.clone(),
            status: read.status.clone(),
            ids: sorted_ids(&read.entries),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.status == PackStatus::Valid
    }

    /// True when the pack holds a line of change `id`.
    pub fn holds(&self, id: ChangeId) -> bool {
        self.ids.binary_search(&id).is_ok()
    }
}

/// The ids of `entries`, sorted, without repeats.
pub(crate) fn sorted_ids(entries: &[Verified]) -> Vec<ChangeId> {
    let mut ids: Vec<ChangeId> = entries.iter().map(Verified::id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Why a file cannot be read here, decided from its first line alone before anything else,
/// so that neither reason is ever reported as damage or salvaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotOurs {
    /// The verified header declares a framing version above `known`.
    Newer(u64),
    /// The verified header names another project: salvaging its lines would merge two
    /// projects.
    Foreign(ProjectId),
}

/// Reads the verified first line of a pack or pending file for a newer framing version in
/// `field` or another project's id. A first line whose line feed became CR LF is read too,
/// as salvage would read it, so another project's file is never salvaged.
pub(crate) fn not_ours(
    first: Option<&RawLine<'_>>,
    field: &str,
    known: u64,
    project: ProjectId,
) -> Option<NotOurs> {
    let (_, json) = verify_salvaged(first?.bytes)?;
    let value = json::parse(json).ok()?;
    let map = value.as_obj()?;
    if let Some(Json::Int(n)) = map.get(field)
        && let Ok(version) = u64::try_from(n.get())
        && version > known
    {
        return Some(NotOurs::Newer(version));
    }
    map.get("project")
        .and_then(Json::as_str)
        .and_then(ProjectId::parse)
        .filter(|owner| *owner != project)
        .map(NotOurs::Foreign)
}

/// Reads and validates a pack at repository path `path` (§4.4). Never fails: whatever is
/// wrong comes back in [`PackRead::status`]. Each byte of a valid pack is hashed once: P2
/// compares the name with the hash of the header line and the ids P4 verified.
pub fn read_pack(path: &str, bytes: &[u8], project: ProjectId) -> PackRead {
    let parsed_path = PackPath::parse(path);
    let lines = split_lines(bytes);
    let mut read = PackRead {
        // Set below, once it is known whether the file frames a pack.
        id: PackId::of(&[]),
        path: parsed_path,
        header: None,
        status: PackStatus::Valid,
        entries: Vec::new(),
        spans: Vec::new(),
    };
    match not_ours(lines.first(), "tcpack", PACK_FORMAT, project) {
        Some(NotOurs::Newer(tcpack)) => {
            read.id = PackId::of(bytes);
            read.status = PackStatus::Newer { tcpack };
            return read;
        }
        Some(NotOurs::Foreign(owner)) => {
            read.id = PackId::of(bytes);
            read.status = PackStatus::Foreign { project: owner };
            return read;
        }
        None => {}
    }
    let header = lines.first().map(|line| parse_header(line));
    if let Some(Ok(header)) = &header {
        read.header = Some(header.clone());
    }
    // P3's header and P4's lines; when both verify, the pack's id follows from them.
    let framed = match &header {
        Some(Ok(header)) => Some(verified_body(header.count, &lines[1..])),
        _ => None,
    };
    // The header verified with its line feed, so the header line is a prefix of the file.
    let header_line = lines
        .first()
        .and_then(|first| bytes.get(..first.bytes.len() + 1));
    read.id = match (&framed, header_line) {
        (Some(Ok(verified)), Some(header_line)) => {
            PackId::of_pack(header_line, verified.iter().map(|line| line.0))
        }
        _ => PackId::of(bytes),
    };
    let problem = match (&parsed_path, &header, &framed) {
        (None, _, _) => Some(PackError::BadPath),
        (_, None, _) => Some(PackError::Empty),
        (_, Some(Err(error)), _) => Some(error.clone()),
        (Some(p), Some(Ok(header)), _) if header.sealer != p.sealer => {
            Some(PackError::SealerMismatch {
                header: header.sealer,
                folder: p.sealer,
            })
        }
        (_, _, Some(Err(error))) => Some(error.clone()),
        (Some(p), _, _) if read.id.name() != p.name => Some(PackError::NameMismatch),
        _ => None,
    };
    let entries = match framed {
        // Every line verified strictly, so salvage would find the same lines again.
        Some(Ok(verified)) => {
            if let Some(problem) = problem {
                read.status = PackStatus::Damaged(problem);
            }
            verified
        }
        _ => {
            read.status = PackStatus::Damaged(problem.unwrap_or(PackError::Empty));
            salvage(&lines)
        }
    };
    (read.entries, read.spans) = parse_entries(entries);
    read
}

/// A line that verified: its id, its JSON and where the JSON lies in the file.
pub(crate) type VerifiedLine<'a> = (ChangeId, &'a [u8], Span);

/// Reads the changes of verified lines.
pub(crate) fn parse_entries(lines: Vec<VerifiedLine<'_>>) -> (Vec<Verified>, Vec<Span>) {
    lines
        .into_iter()
        .map(|(id, json, span)| (parse_change(id, Arc::from(json)), span))
        .unzip()
}

/// P4 on the lines after a header that passed P1 to P3: every line verifies, the last
/// ends with a line feed, and there are `count` of them. Each line is hashed once.
pub(crate) fn verified_body<'a>(
    count: u32,
    lines: &[RawLine<'a>],
) -> Result<Vec<VerifiedLine<'a>>, PackError> {
    let verified = verified_lines(lines)?;
    let found = lines.len() as u64;
    if found != u64::from(count) {
        return Err(PackError::CountMismatch {
            expected: u64::from(count),
            found,
        });
    }
    Ok(verified)
}

/// Every line verified, or the first line (numbered from 1 for the first line after the
/// header) that does not verify, or a missing final line feed.
pub(crate) fn verified_lines<'a>(
    lines: &[RawLine<'a>],
) -> Result<Vec<VerifiedLine<'a>>, PackError> {
    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let Some((id, json)) = verify_line(line.bytes) else {
            return Err(PackError::LineUnverified {
                line: index as u64 + 1,
            });
        };
        out.push((id, json, line.span(json)));
    }
    if lines.last().is_some_and(|line| !line.terminated) {
        return Err(PackError::Truncated);
    }
    Ok(out)
}

/// Every line that verifies, as written (§4.4), a CR before its line feed allowed, except a
/// first line that is a header. A change whose author wrote it is a change wherever it
/// stands; a header never reads as one.
pub(crate) fn salvage<'a>(lines: &[RawLine<'a>]) -> Vec<VerifiedLine<'a>> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let (id, json) = verify_salvaged(line.bytes)?;
            (index > 0 || !is_header(json)).then(|| (id, json, line.span(json)))
        })
        .collect()
}

/// A header of a pack or a pending file, of any framing version: a JSON object with a
/// `tcpack` or a `pending` member. No change has either member.
fn is_header(json: &[u8]) -> bool {
    json::parse(json)
        .ok()
        .and_then(|value| {
            value
                .as_obj()
                .map(|map| map.contains_key("tcpack") || map.contains_key("pending"))
        })
        .unwrap_or(false)
}

fn parse_header(line: &RawLine<'_>) -> Result<PackHeader, PackError> {
    let (_, json) = verify_line(line.bytes).ok_or(PackError::HeaderUnverified)?;
    if !line.terminated {
        return Err(PackError::Truncated);
    }
    let value = json::parse(json).map_err(|e| PackError::HeaderMalformed(e.to_string().into()))?;
    let malformed = |text: String| PackError::HeaderMalformed(text.into());
    let mut o = HeaderReader::new(&value).map_err(malformed)?;
    let tcpack: u64 = o.req("tcpack").map_err(malformed)?;
    if tcpack != PACK_FORMAT {
        return Err(malformed(format!("tcpack {tcpack}")));
    }
    let header = PackHeader {
        count: o.req("count").map_err(malformed)?,
        project: o.req("project").map_err(malformed)?,
        replaces: o.req("replaces").map_err(malformed)?,
        sealed: WallTime(o.req("sealed").map_err(malformed)?),
        sealer: o.req("sealer").map_err(malformed)?,
    };
    if header.replaces.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(malformed(
            "replaces is not sorted, or repeats a name".into(),
        ));
    }
    if header.sealer.kind() != DeviceKind::Local {
        return Err(malformed("the sealer is not a Local device".into()));
    }
    Ok(header)
}

// ---------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------

/// What a new pack's header says, apart from its count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSpec {
    pub project: ProjectId,
    /// This device: a Local device.
    pub sealer: DeviceId,
    pub sealed: WallTime,
}

/// A pack ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedPack {
    pub path: PackPath,
    pub id: PackId,
    pub bytes: Vec<u8>,
    /// The ids of its lines, in file order.
    pub changes: Vec<ChangeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackWriteError {
    #[error("only a Local device seals packs")]
    NotLocal,
    #[error("the pack would have {bytes} bytes; the limit is 64 MiB")]
    TooLarge { bytes: usize },
    #[error("a pack holds at least one change")]
    Empty,
}

pub(crate) fn header_line(spec: &PackSpec, count: usize, replaces: &[PackName]) -> Vec<u8> {
    let header = header_object(vec![
        ("count", Json::from(count as u64)),
        ("project", to_json(&spec.project)),
        (
            "replaces",
            Json::Arr(replaces.iter().map(|n| Json::from(n.to_hex())).collect()),
        ),
        ("sealed", Json::Int(JsonInt::from(spec.sealed.0))),
        ("sealer", to_json(&spec.sealer)),
        ("tcpack", Json::from(PACK_FORMAT)),
    ]);
    frame(&header.to_canonical())
}

/// One pack of `lines`, sorted by (device, seq, id) without repeats, listing `replaces`.
pub fn encode_pack(
    spec: &PackSpec,
    lines: &[Verified],
    replaces: &[PackName],
) -> Result<EncodedPack, PackWriteError> {
    if spec.sealer.kind() != DeviceKind::Local {
        return Err(PackWriteError::NotLocal);
    }
    let mut sorted: Vec<&Verified> = lines.iter().collect();
    sorted.sort_by_key(|line| line.sort_key());
    sorted.dedup_by_key(|line| line.id());
    if sorted.is_empty() {
        return Err(PackWriteError::Empty);
    }
    let mut replaces = replaces.to_vec();
    replaces.sort();
    replaces.dedup();
    let mut bytes = header_line(spec, sorted.len(), &replaces);
    let id = PackId::of_pack(&bytes, sorted.iter().map(|line| line.id()));
    for line in &sorted {
        bytes.extend_from_slice(&encode_line(line.id(), line.raw()));
    }
    if bytes.len() > MAX_PACK_BYTES {
        return Err(PackWriteError::TooLarge { bytes: bytes.len() });
    }
    Ok(EncodedPack {
        path: PackPath {
            sealer: spec.sealer,
            month: YearMonth::of(spec.sealed),
            name: id.name(),
        },
        id,
        bytes,
        changes: sorted.iter().map(|line| line.id()).collect(),
    })
}

/// What a seal writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub packs: Vec<EncodedPack>,
    /// Lines too long for any pack, even alone with its header, in sort order. Only a change
    /// that rule V7 quarantines can be that long (a salvaged line, §4.4); the seal sets it
    /// aside in `.tc/quarantine/` (§5.11 K1) instead of failing, so it never blocks the seal
    /// of every other line.
    pub set_aside: Vec<ChangeId>,
}

/// The seal of §5.11 K1 and K2: every line, without repeats, sorted by (device, seq, id),
/// split at line boundaries into packs of at most [`MAX_PACK_BYTES`], and the lines no
/// pack can hold set aside.
pub fn seal(spec: &PackSpec, lines: &[Verified]) -> Result<Sealed, PackWriteError> {
    if spec.sealer.kind() != DeviceKind::Local {
        return Err(PackWriteError::NotLocal);
    }
    let mut sorted: Vec<&Verified> = lines.iter().collect();
    sorted.sort_by_key(|line| line.sort_key());
    sorted.dedup_by_key(|line| line.id());
    let alone = header_line(spec, 1, &[]).len() + LINE_OVERHEAD;
    let (fits, too_long): (Vec<&Verified>, Vec<&Verified>) = sorted
        .into_iter()
        .partition(|line| alone + line.raw().len() <= MAX_PACK_BYTES);
    let packs = chunk(&fits, |count| header_line(spec, count, &[]).len())
        .into_iter()
        .map(|chunk| {
            let owned: Vec<Verified> = chunk.into_iter().cloned().collect();
            encode_pack(spec, &owned, &[])
        })
        .collect::<Result<_, _>>()?;
    Ok(Sealed {
        packs,
        set_aside: too_long.into_iter().map(Verified::id).collect(),
    })
}

/// Splits sorted lines into the fewest consecutive chunks whose packs fit the limit.
pub(crate) fn chunk<'a>(
    lines: &[&'a Verified],
    header_len: impl Fn(usize) -> usize,
) -> Vec<Vec<&'a Verified>> {
    let mut chunks: Vec<Vec<&Verified>> = Vec::new();
    let mut current: Vec<&Verified> = Vec::new();
    let mut body = 0usize;
    for line in lines {
        let len = line.raw().len() + LINE_OVERHEAD;
        if !current.is_empty() && header_len(current.len() + 1) + body + len > MAX_PACK_BYTES {
            chunks.push(std::mem::take(&mut current));
            body = 0;
        }
        current.push(line);
        body += len;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_come_from_utc_time_and_are_clamped() {
        assert_eq!(YearMonth::of(WallTime(0)).to_string(), "1970-01");
        assert_eq!(
            YearMonth::of(WallTime(1_760_520_000_000)).to_string(),
            "2025-10"
        );
        // 2024-02-29T23:59:59.999Z and the next millisecond.
        assert_eq!(
            YearMonth::of(WallTime(1_709_251_199_999)).to_string(),
            "2024-02"
        );
        assert_eq!(
            YearMonth::of(WallTime(1_709_251_200_000)).to_string(),
            "2024-03"
        );
        assert_eq!(YearMonth::of(WallTime(-1)).to_string(), "1969-12");
        assert_eq!(YearMonth::of(WallTime(i64::MAX)).to_string(), "9999-12");
        assert_eq!(YearMonth::of(WallTime(i64::MIN)).to_string(), "0000-01");
        assert_eq!(YearMonth::parse("2026-10"), YearMonth::new(2026, 10));
        for bad in [
            "2026-13", "2026-00", "26-10", "2026/10", "2026-1x", "+026-10",
        ] {
            assert_eq!(YearMonth::parse(bad), None, "{bad}");
        }
    }
}
