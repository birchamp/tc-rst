//! Changes (`docs/STORAGE.md` §2.5): one immutable version of one record, identified by the
//! SHA-256 of its canonical bytes.

use crate::canon;
use crate::format::FORMAT_MINOR;
use crate::ids::{BatchRef, ChangeId, DeviceId, DeviceSeq, GitOid, Md5, RepoPath, WallTime};
use crate::json;
use crate::keys::PinGroup;
use crate::record::Record;
use crate::rules::{self, MAX_LINE_BYTES, RuleViolation};
use std::sync::Arc;

/// What a change says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBody {
    /// The format minor that introduced the kinds and fields the change uses (§12).
    pub v: u16,
    pub record: Record,
    /// Heads of the same record the author held and replaced. Sorted ascending, no
    /// repeats. Empty = "no version I know of". A resolution names every head it resolves.
    pub parents: Vec<ChangeId>,
    pub device: DeviceId,
    pub seq: DeviceSeq,
    pub at: WallTime,
    pub batch: BatchRef,
    pub cause: Cause,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    /// A person typed, clicked, aligned or checked.
    Edit,
    /// Chose among conflicting heads.
    Resolve,
    /// Brought an older version back from History.
    Restore { from: ChangeId },
    /// A resource upgrade or gateway change re-attached it.
    CarryOver { group: PinGroup },
    /// A file changed outside the app (§11). `unknown_base` marks a change recorded without
    /// parents by the rules of §11.1 X3; such a change is never chosen as provisional.
    External {
        source: ExternalSource,
        unknown_base: bool,
    },
    /// tC4, tC3, USFM, foreign burrito (§13).
    Import(ImportSource),
    /// Project or book creation.
    Seed,
    /// "Start empty" when restoring a removed book (§5.7): a person's choice to blank the
    /// text, never a seed stub.
    StartOver,
    /// Pin coverage backfill (tC4 D41).
    System(SystemReason),
    /// A cause from a newer minor (§12), shown as "made by a newer translationCore". It is
    /// only ever read, never written.
    Newer { name: Box<str> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalSource {
    LocalFile {
        path: RepoPath,
        seen_md5: Md5,
    },
    Door43 {
        commit: GitOid,
        path: RepoPath,
        author: Box<str>,
    },
    ExchangeFile {
        path: RepoPath,
        exporter: Option<DeviceId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSource {
    Tc4 {
        actor: Box<str>,
        ts: Box<str>,
        op: Box<str>,
    },
    Tc4Reconcile,
    Tc3 {
        user: Option<Box<str>>,
    },
    /// A USFM file: a new book, or "Update a book" (§13.6). `unknown_base` marks a change the
    /// update made with no parents, so it is never the provisional text (§13.6 U3, F7).
    Usfm {
        file_name: Box<str>,
        unknown_base: bool,
    },
    Burrito {
        file_name: Box<str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemReason {
    PinCoverageBackfill,
    /// The pins a new project gets, written at the first open of a project that holds none
    /// (tC4 #766), so History never says the translator chose them.
    CreationPins,
}

/// One immutable version of one record. Never edited, never deleted. Every `Change` value
/// passed the per-change rules of §4.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    id: ChangeId,
    raw: Arc<[u8]>,
    body: Arc<ChangeBody>,
}

/// Why a change cannot be written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChangeError {
    /// A cause from a newer minor is only ever read; this app cannot write one.
    #[error("a cause from a newer translationCore cannot be written")]
    NewerCause,
    /// An unknown record's JSON is not canonical, or names a kind this app knows.
    #[error("the record of a newer kind is not canonical JSON of an unknown kind: {0}")]
    BadUnknownRecord(String),
    /// A change this app writes uses only this app's minor.
    #[error("a change for minor {v} cannot be written by an app of minor {FORMAT_MINOR}")]
    NewerMinor { v: u16 },
    /// The change breaks a rule of §4.5, so readers would quarantine it.
    #[error(transparent)]
    Rule(#[from] RuleViolation),
}

impl Change {
    /// Serializes `body` canonically and checks it as every reader will. The canonical
    /// bytes are read back, so a change this app writes is exactly what readers see.
    pub fn new(body: ChangeBody) -> Result<Self, ChangeError> {
        if body.v > FORMAT_MINOR {
            return Err(ChangeError::NewerMinor { v: body.v });
        }
        let raw: Arc<[u8]> = canon::encode_body(&body)?.into();
        match parse_change(ChangeId::of(&raw), raw) {
            Verified::Change(change) => Ok(change),
            Verified::Quarantined(q) => Err(ChangeError::Rule(q.rule)),
        }
    }

    pub fn id(&self) -> ChangeId {
        self.id
    }

    /// The canonical JSON exactly as stored; never re-serialized.
    pub fn raw(&self) -> &Arc<[u8]> {
        &self.raw
    }

    pub fn body(&self) -> &ChangeBody {
        &self.body
    }

    pub fn into_body(self) -> ChangeBody {
        Arc::unwrap_or_clone(self.body)
    }
}

/// A verified line whose change breaks a rule of §4.5: kept in its file, not folded,
/// listed. `batch` and `device` are read from the bytes when they can be, so the fold can
/// hold the change's batch (§7.2 F2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedChange {
    pub id: ChangeId,
    pub raw: Arc<[u8]>,
    pub rule: RuleViolation,
    pub batch: Option<BatchRef>,
    pub device: Option<DeviceId>,
    pub seq: Option<DeviceSeq>,
}

/// A change line whose hash verified: a change, or a quarantined change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    Change(Change),
    Quarantined(QuarantinedChange),
}

impl Verified {
    pub fn id(&self) -> ChangeId {
        match self {
            Self::Change(c) => c.id,
            Self::Quarantined(q) => q.id,
        }
    }

    pub fn raw(&self) -> &Arc<[u8]> {
        match self {
            Self::Change(c) => &c.raw,
            Self::Quarantined(q) => &q.raw,
        }
    }

    /// The order lines take in a pack: by device, then seq, then id. A quarantined change
    /// whose device or seq could not be read sorts before those that have one.
    pub fn sort_key(&self) -> (Option<DeviceId>, Option<DeviceSeq>, ChangeId) {
        match self {
            Self::Change(c) => (Some(c.body.device), Some(c.body.seq), c.id),
            Self::Quarantined(q) => (q.device, q.seq, q.id),
        }
    }

    pub fn as_change(&self) -> Option<&Change> {
        match self {
            Self::Change(c) => Some(c),
            Self::Quarantined(_) => None,
        }
    }
}

impl From<Change> for Verified {
    fn from(change: Change) -> Self {
        Self::Change(change)
    }
}

/// Reads the canonical JSON of a change whose id has been verified, and applies the rules
/// of §4.5. Never fails: a change that breaks a rule comes back quarantined.
pub fn parse_change(id: ChangeId, raw: Arc<[u8]>) -> Verified {
    let verdict = check_change(&raw);
    match verdict {
        Ok(body) => Verified::Change(Change {
            id,
            raw,
            body: Arc::new(body),
        }),
        Err((rule, (batch, device, seq))) => Verified::Quarantined(QuarantinedChange {
            id,
            raw,
            rule,
            batch,
            device,
            seq,
        }),
    }
}

type Origin = (Option<BatchRef>, Option<DeviceId>, Option<DeviceSeq>);

/// The body of a change's canonical JSON, or the rule it breaks and what could be read of
/// its origin.
fn check_change(raw: &[u8]) -> Result<ChangeBody, (RuleViolation, Origin)> {
    let line_bytes = raw.len() + crate::line::LINE_OVERHEAD;
    if line_bytes > MAX_LINE_BYTES {
        let rule = RuleViolation::LineTooLong {
            bytes: line_bytes as u64,
        };
        return Err((rule, (None, None, None)));
    }
    let tape = json::parse_tape(raw).map_err(|error| {
        let rule = if error.problem == json::JsonProblem::TooDeep {
            RuleViolation::TooDeep
        } else {
            RuleViolation::Shape {
                at: format!("byte {}", error.at).into(),
                problem: error.problem.to_string().into(),
            }
        };
        (rule, (None, None, None))
    })?;
    let value = tape.root();
    let body =
        canon::decode_body_view(value).map_err(|rule| (rule, canon::best_effort_origin(value)))?;
    rules::check(&body).map_err(|rule| (rule, canon::best_effort_origin(value)))?;
    Ok(body)
}
