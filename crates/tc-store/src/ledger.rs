//! The placement ledger (`docs/STORAGE.md` §6.2, §11.1): who wrote the bytes at a path the app
//! derives or places.
//!
//! The ledger keeps one [`State`] per path, in `.tc/materialized.json`, and the heads of each
//! placement it names, in the path's heads file. Before the app renames bytes over a path it
//! durably records them ([`Event::WriteBegun`]); after the rename it records that they are
//! there ([`Event::Renamed`]). Whether bytes found at a path are the app's own is then a lookup
//! in the ledger, and whether they may be written over is the fold's answer to one question:
//! does it still hold, applied, every change the ledger says those bytes show?
//!
//! [`next`] is the whole decision, pure: a state and an event give the next state and the
//! [`Action`]s the writer performs. The writer does all the I/O; it never decides who wrote a
//! file. Every pair the table does not call safe keeps both versions: a copy in
//! `.tc/external/`, the bytes recorded as an edit with no base, and a conflict the person
//! settles. A head whose value cannot be read is never shown as absent.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// A file's size and modification time as a stat gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub size: u64,
    pub mtime_ns: i64,
}

/// Bytes the app placed at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub md5: String,
    pub size: u64,
    /// The file's modification time once the app took it after the rename or a read; `None`
    /// until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ns: Option<i64>,
    /// The bytes were read back after their timestamp tick was over (§6.2): the size and time
    /// stand for them.
    #[serde(default)]
    pub settled: bool,
}

impl Placement {
    /// Bytes the app has just renamed into place: not stat'ed yet.
    pub fn renamed(md5: String, size: u64) -> Self {
        Self {
            md5,
            size,
            mtime_ns: None,
            settled: false,
        }
    }

    /// The size and time the ledger keeps for the bytes, when it took them.
    pub fn stat(&self) -> Option<Stat> {
        self.mtime_ns.map(|mtime_ns| Stat {
            size: self.size,
            mtime_ns,
        })
    }
}

/// Bytes the app holds at a path without writing over them, until the person chooses (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Held {
    pub md5: String,
    pub cause: HeldCause,
    /// The byte copy in `.tc/external/`, relative to the project folder.
    pub copy: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ns: Option<i64>,
}

/// Why a path is held (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeldCause {
    /// A program changed a file the app cannot record (a carried file, §11.4), or the app
    /// cannot tell: `OutsideEditHeld`.
    Edit,
    /// The file changed although its size and time did not: `StorageDamage`.
    Damage,
}

/// What the ledger knows of the bytes at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum State {
    /// No record: the app never placed bytes here, or `.tc/` was lost or copied away.
    Unknown,
    /// The app placed these bytes and knows its rename went through.
    Placed { placed: Placement },
    /// The app named bytes `new` before renaming them over `old` (or over nothing): the rename
    /// may or may not have reached the disk.
    Writing { old: Option<Placement>, new: String },
    /// Bytes the app keeps as they are for the person, with a copy in `.tc/external/`.
    Held { held: Held },
}

/// What happens to a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A read of the path: at the open (O9), before a write or a removal (M1), just before a
    /// rename (M4), when `metadata.json` is listed or written (M2, M6), when the watcher or the
    /// window's focus tells of a change, at the storage check (O14).
    Read(Disk),
    /// M3: the ledger names bytes `new` before they are renamed over the path.
    WriteBegun { new: String },
    /// M4: the rename of the bytes named went through.
    Renamed { size: u64 },
    /// M4: the rename did not happen (another program holds the file, the last look found
    /// bytes the app did not write, the disk refused).
    WriteFailed,
    /// The file's size and time, taken after the app wrote or read bytes it knows; `settled`:
    /// a read of them began 2 s or more after the time (§6.2).
    Stat { stat: Stat, settled: bool },
    /// Bytes `md5` were found to be what the project places there (a carried file's version),
    /// or the app wrote them outside a write group (a fixed file): the app's own now.
    Adopted { md5: String, size: u64 },
    /// The bytes found were copied to `.tc/external/` and are held (§6.2).
    Kept {
        md5: String,
        cause: HeldCause,
        copy: String,
        stat: Option<Stat>,
    },
    /// The bytes `md5` were recorded as an edit (§11.1 X4, D7): they stand for the changes
    /// the batch recorded, so the next write may replace them.
    Recorded { md5: String, size: u64 },
    /// The person takes the held bytes in as an edit.
    TakeIn,
    /// The person restores the held file from the project.
    Restore,
    /// The app removed the file, or the path left the projection and the file is gone.
    Removed,
}

/// What a read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disk {
    Missing,
    /// The file is there and cannot be read now (another program holds it, a placeholder).
    Unreadable,
    Bytes {
        md5: String,
    },
}

/// What the projection is at the path, for a read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projection {
    /// The look does not project (the watcher, the focus, a listing).
    NotAsked,
    /// The projection has no file at the path.
    Absent,
    Bytes {
        md5: String,
    },
}

/// What the writer found besides the bytes, for a read. Each is a lookup or a stat, never a
/// guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub projection: Projection,
    /// Every head the projection read could be read (§2.6). When not, the projection is not
    /// what the project holds, and nothing is written from it.
    pub heads_readable: bool,
    /// Whether the fold holds, applied (its batch complete or released, §7.4), every change
    /// the ledger's heads of the bytes found name; `None` when the ledger keeps no heads for
    /// those bytes.
    pub holds: Option<bool>,
    /// The file's size and time are the ones the ledger keeps for the bytes placed or held.
    /// Never on a network drive (§6.2).
    pub same_stat: bool,
    /// The fold of the job is older than the one that placed the bytes there (a checkpoint's
    /// job, §5.13 C2).
    pub newer_fold: bool,
}

impl Facts {
    /// The facts of a look that does not project.
    pub fn look(holds: Option<bool>, same_stat: bool) -> Self {
        Self {
            projection: Projection::NotAsked,
            heads_readable: true,
            holds,
            same_stat,
            newer_fold: false,
        }
    }
}

/// The base an edit is recorded against (§11.1 X3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The heads the ledger keeps for the placement of these md5, each record's heads cut to
    /// those the fold holds applied (a record that lost one is compared in full). Unknown
    /// when the ledger keeps none.
    Placed(String),
    /// What HEAD's packs hold, when HEAD is this app's; else unknown (§6.2, `.tc/` lost).
    Head,
    /// No base: every changed record is a conflict with the project's text unless equal.
    Unknown,
}

/// Why a path is left as it is for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitFor {
    /// The file cannot be read now.
    FileUnreadable,
    /// A head the projection reads cannot be read: the file keeps its bytes (§2.6).
    ValuesUnreadable,
    /// A newer fold placed the bytes; the next job, on that fold, looks at them.
    NewerFold,
}

/// What the writer does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write the projection over the bytes (M3 to M7).
    Write,
    /// The file is gone: write it again, and tell the person when it was there before (I11).
    Rebuild { told: bool },
    /// Remove the file: the projection no longer has the path.
    Remove,
    /// Take the file's size and time again: the bytes are what the ledger says.
    Refresh,
    /// Keep both: copy the bytes to `.tc/external/` and record them as an edit against the
    /// base (§11.1). Nothing is written over them until the batch is durable.
    Record(Base),
    /// Copy the bytes to `.tc/external/`, leave the file as it is, tell the person (§6.2).
    Hold(HeldCause),
    /// Leave the file as it is, and try again later.
    Wait(WaitFor),
    /// Drop the ledger's record of the path.
    Forget,
}

/// The rows of the transition table for a read (§6.2): R1 to R14 for bytes found, R15 to R18
/// for no file, R19 for a file that cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Row {
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    R16,
    R17,
    R18,
    R19,
}

/// The names [`hits`] counts under: the rows of a read, then the other events.
pub const HIT_NAMES: [&str; 29] = [
    "R1",
    "R2",
    "R3",
    "R4",
    "R5",
    "R6",
    "R7",
    "R8",
    "R9",
    "R10",
    "R11",
    "R12",
    "R13",
    "R14",
    "R15",
    "R16",
    "R17",
    "R18",
    "R19",
    "WriteBegun",
    "Renamed",
    "WriteFailed",
    "Stat",
    "Adopted",
    "Kept",
    "Recorded",
    "TakeIn",
    "Restore",
    "Removed",
];

static HITS: [AtomicU64; HIT_NAMES.len()] = [const { AtomicU64::new(0) }; HIT_NAMES.len()];

/// How often this process took each row of a read and each other event, for the coverage
/// check of the model-based test (`tests/store/model.rs`).
pub fn hits() -> BTreeMap<&'static str, u64> {
    HIT_NAMES
        .iter()
        .zip(&HITS)
        .map(|(name, n)| (*name, n.load(Ordering::Relaxed)))
        .collect()
}

fn hit(event: &Event, row: Option<Row>) {
    let i = match (event, row) {
        (_, Some(row)) => row as usize,
        (Event::Read(_), None) => return,
        (Event::WriteBegun { .. }, None) => 19,
        (Event::Renamed { .. }, None) => 20,
        (Event::WriteFailed, None) => 21,
        (Event::Stat { .. }, None) => 22,
        (Event::Adopted { .. }, None) => 23,
        (Event::Kept { .. }, None) => 24,
        (Event::Recorded { .. }, None) => 25,
        (Event::TakeIn, None) => 26,
        (Event::Restore, None) => 27,
        (Event::Removed, None) => 28,
    };
    HITS[i].fetch_add(1, Ordering::Relaxed);
}

/// Who the bytes found are to the ledger.
enum Who {
    /// The app's own, placed as `Placement`.
    Ours(Placement),
    /// The bytes held.
    Held,
    /// A write under way replaced these bytes: a program put them back after the rename (a
    /// revert, a backup tool that keeps their size and time), or the rename never happened.
    /// Neither the bytes nor their stat can tell which.
    Ambiguous,
    /// Bytes the ledger never named; `damage`: the size and time of the bytes placed did not
    /// move, settled (§6.2 held).
    Other { base: Base, damage: bool },
    /// No record at all.
    Stranger,
}

fn who(state: &State, md5: &str, facts: &Facts) -> Who {
    match state {
        State::Unknown => Who::Stranger,
        State::Placed { placed } if placed.md5 == md5 => Who::Ours(placed.clone()),
        State::Placed { placed } => Who::Other {
            base: Base::Placed(placed.md5.clone()),
            damage: placed.settled && facts.same_stat,
        },
        State::Writing { new, .. } if new == md5 => Who::Ours(Placement::renamed(new.clone(), 0)),
        State::Writing { old: Some(old), .. } if old.md5 == md5 => Who::Ambiguous,
        State::Writing { .. } => Who::Other {
            base: Base::Unknown,
            damage: false,
        },
        State::Held { held } if held.md5 == md5 => Who::Held,
        State::Held { .. } => Who::Other {
            base: Base::Unknown,
            damage: false,
        },
    }
}

/// The table of §6.2 for a read that found bytes `md5`.
fn read_bytes(state: &State, md5: &str, facts: &Facts) -> (Row, State, Vec<Action>) {
    let stay = || state.clone();
    if facts.newer_fold {
        return (Row::R1, stay(), vec![Action::Wait(WaitFor::NewerFold)]);
    }
    let converged = facts.heads_readable
        && matches!(&facts.projection, Projection::Bytes { md5: e } if e == md5);
    let ours = |placed: Placement| State::Placed { placed };
    match who(state, md5, facts) {
        // The bytes are what the project would write: nothing in them is lost, whoever wrote
        // them. A hold ends.
        _ if converged => {
            let placed = match state {
                State::Placed { placed } if placed.md5 == md5 => placed.clone(),
                _ => Placement::renamed(md5.to_owned(), 0),
            };
            (Row::R2, ours(placed), vec![Action::Refresh])
        }
        Who::Held => (Row::R3, stay(), vec![Action::Refresh]),
        Who::Ours(placed) => match facts.holds {
            // The bytes show changes the fold does not hold applied (a journal file lost
            // them, cut or removed; a batch lost a member), or the ledger cannot say what
            // they show: they may be the only record of that work.
            Some(false) | None => (
                Row::R4,
                ours(placed.clone()),
                vec![Action::Record(Base::Placed(placed.md5))],
            ),
            Some(true) => match &facts.projection {
                Projection::NotAsked => (Row::R5, ours(placed), vec![Action::Refresh]),
                _ if !facts.heads_readable => (
                    Row::R6,
                    ours(placed),
                    vec![Action::Wait(WaitFor::ValuesUnreadable)],
                ),
                Projection::Absent => (Row::R7, ours(placed), vec![Action::Remove]),
                Projection::Bytes { .. } => (Row::R8, ours(placed), vec![Action::Write]),
            },
        },
        Who::Ambiguous => (Row::R9, stay(), vec![Action::Record(Base::Unknown)]),
        Who::Other { damage: true, .. } => {
            (Row::R10, stay(), vec![Action::Hold(HeldCause::Damage)])
        }
        Who::Other {
            base: base @ Base::Placed(_),
            ..
        } => (Row::R11, stay(), vec![Action::Record(base)]),
        Who::Other { base, .. } => match state {
            State::Held { .. } => (Row::R13, stay(), vec![Action::Record(base)]),
            _ => (Row::R12, stay(), vec![Action::Record(base)]),
        },
        Who::Stranger => (Row::R14, stay(), vec![Action::Record(Base::Head)]),
    }
}

/// The table of §6.2 for a read that found no file.
fn read_missing(state: &State, facts: &Facts) -> (Row, State, Vec<Action>) {
    if facts.newer_fold {
        return (
            Row::R1,
            state.clone(),
            vec![Action::Wait(WaitFor::NewerFold)],
        );
    }
    let told = !matches!(state, State::Unknown);
    match &facts.projection {
        // A deletion is never read as an edit (I11); the file the project holds comes back,
        // never with a value shown as absent.
        Projection::Bytes { .. } if !facts.heads_readable => (
            Row::R15,
            State::Unknown,
            vec![Action::Wait(WaitFor::ValuesUnreadable)],
        ),
        Projection::Bytes { .. } => (Row::R16, State::Unknown, vec![Action::Rebuild { told }]),
        Projection::Absent => (Row::R17, State::Unknown, vec![Action::Forget]),
        // A held file that is gone ends its hold; the next job writes it.
        Projection::NotAsked => (Row::R18, State::Unknown, vec![Action::Rebuild { told }]),
    }
}

/// The next state of a path and what the writer does (`docs/STORAGE.md` §6.2, the transition
/// table). Every pair is listed; none drops bytes.
pub fn next(state: &State, event: &Event, facts: &Facts) -> (State, Vec<Action>) {
    let (row, after, actions) = match event {
        Event::Read(Disk::Unreadable) => (
            Some(Row::R19),
            state.clone(),
            vec![Action::Wait(WaitFor::FileUnreadable)],
        ),
        Event::Read(Disk::Missing) => {
            let (row, after, actions) = read_missing(state, facts);
            (Some(row), after, actions)
        }
        Event::Read(Disk::Bytes { md5 }) => {
            let (row, after, actions) = read_bytes(state, md5, facts);
            (Some(row), after, actions)
        }
        _ => {
            let (after, actions) = other_event(state, event);
            (None, after, actions)
        }
    };
    hit(event, row);
    (after, actions)
}

/// The table of §6.2 for the events other than a read.
fn other_event(state: &State, event: &Event) -> (State, Vec<Action>) {
    let stay = || (state.clone(), Vec::new());
    match (state, event) {
        (_, Event::Read(_)) => stay(),
        (State::Unknown, Event::WriteBegun { new }) => (
            State::Writing {
                old: None,
                new: new.clone(),
            },
            Vec::new(),
        ),
        (State::Placed { placed }, Event::WriteBegun { new }) => (
            State::Writing {
                old: Some(placed.clone()),
                new: new.clone(),
            },
            Vec::new(),
        ),
        // A second write begun before a read settled the first: the bytes of the first are
        // no longer named, so a read that finds them keeps them (Other, unknown base).
        (State::Writing { old, .. }, Event::WriteBegun { new }) => (
            State::Writing {
                old: old.clone(),
                new: new.clone(),
            },
            Vec::new(),
        ),
        // Held bytes are written over only after the person restores the file.
        (State::Held { .. }, Event::WriteBegun { .. }) => stay(),
        (State::Writing { new, .. }, Event::Renamed { size }) => (
            State::Placed {
                placed: Placement::renamed(new.clone(), *size),
            },
            Vec::new(),
        ),
        (State::Unknown | State::Placed { .. } | State::Held { .. }, Event::Renamed { .. }) => {
            stay()
        }
        (State::Writing { old: Some(old), .. }, Event::WriteFailed) => (
            State::Placed {
                placed: old.clone(),
            },
            Vec::new(),
        ),
        (State::Writing { old: None, .. }, Event::WriteFailed) => (State::Unknown, Vec::new()),
        (State::Unknown | State::Placed { .. } | State::Held { .. }, Event::WriteFailed) => stay(),
        (State::Placed { placed }, Event::Stat { stat, settled }) => (
            State::Placed {
                placed: Placement {
                    md5: placed.md5.clone(),
                    size: stat.size,
                    mtime_ns: Some(stat.mtime_ns),
                    settled: *settled,
                },
            },
            Vec::new(),
        ),
        (State::Held { held }, Event::Stat { stat, .. }) => (
            State::Held {
                held: Held {
                    size: stat.size,
                    mtime_ns: Some(stat.mtime_ns),
                    ..held.clone()
                },
            },
            Vec::new(),
        ),
        (State::Unknown | State::Writing { .. }, Event::Stat { .. }) => stay(),
        // Bytes the project places, found there: nothing in them is lost, and a hold ends.
        (_, Event::Adopted { md5, size }) => (
            State::Placed {
                placed: Placement::renamed(md5.clone(), *size),
            },
            Vec::new(),
        ),
        (
            _,
            Event::Kept {
                md5,
                cause,
                copy,
                stat,
            },
        ) => (
            State::Held {
                held: Held {
                    md5: md5.clone(),
                    cause: *cause,
                    copy: copy.clone(),
                    size: stat.map_or(0, |s| s.size),
                    mtime_ns: stat.map(|s| s.mtime_ns),
                },
            },
            Vec::new(),
        ),
        (_, Event::Recorded { md5, size }) => (
            State::Placed {
                placed: Placement::renamed(md5.clone(), *size),
            },
            Vec::new(),
        ),
        (State::Held { .. }, Event::TakeIn) => (state.clone(), vec![Action::Record(Base::Unknown)]),
        (State::Unknown | State::Placed { .. } | State::Writing { .. }, Event::TakeIn) => stay(),
        // The person gives up the held bytes (their copy stays): they are the app's to write
        // over.
        (State::Held { held }, Event::Restore) => (
            State::Placed {
                placed: Placement {
                    md5: held.md5.clone(),
                    size: held.size,
                    mtime_ns: held.mtime_ns,
                    settled: false,
                },
            },
            vec![Action::Write],
        ),
        (State::Unknown | State::Placed { .. } | State::Writing { .. }, Event::Restore) => stay(),
        (_, Event::Removed) => (State::Unknown, vec![Action::Forget]),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccc";
    const E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    fn placed(md5: &str, settled: bool) -> Placement {
        Placement {
            md5: md5.into(),
            size: 10,
            mtime_ns: Some(5),
            settled,
        }
    }

    fn states() -> Vec<State> {
        vec![
            State::Unknown,
            State::Placed {
                placed: placed(A, true),
            },
            State::Placed {
                placed: placed(A, false),
            },
            State::Writing {
                old: Some(placed(A, true)),
                new: B.into(),
            },
            State::Writing {
                old: None,
                new: B.into(),
            },
            State::Held {
                held: Held {
                    md5: C.into(),
                    cause: HeldCause::Damage,
                    copy: ".tc/external/x".into(),
                    size: 10,
                    mtime_ns: Some(5),
                },
            },
        ]
    }

    fn events() -> Vec<Event> {
        let mut out = vec![
            Event::Read(Disk::Missing),
            Event::Read(Disk::Unreadable),
            Event::WriteBegun { new: E.into() },
            Event::Renamed { size: 3 },
            Event::WriteFailed,
            Event::Stat {
                stat: Stat {
                    size: 3,
                    mtime_ns: 9,
                },
                settled: true,
            },
            Event::Adopted {
                md5: C.into(),
                size: 3,
            },
            Event::Kept {
                md5: C.into(),
                cause: HeldCause::Edit,
                copy: "c".into(),
                stat: None,
            },
            Event::Recorded {
                md5: C.into(),
                size: 3,
            },
            Event::TakeIn,
            Event::Restore,
            Event::Removed,
        ];
        for md5 in [A, B, C, E, "dddddddddddddddddddddddddddddddd"] {
            out.push(Event::Read(Disk::Bytes { md5: md5.into() }));
        }
        out
    }

    fn all_facts() -> Vec<Facts> {
        let mut out = Vec::new();
        for projection in [
            Projection::NotAsked,
            Projection::Absent,
            Projection::Bytes { md5: E.into() },
            Projection::Bytes { md5: A.into() },
        ] {
            for heads_readable in [true, false] {
                for holds in [Some(true), Some(false), None] {
                    for same_stat in [true, false] {
                        for newer_fold in [false, true] {
                            out.push(Facts {
                                projection: projection.clone(),
                                heads_readable,
                                holds,
                                same_stat,
                                newer_fold,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// The md5 of the bytes a state says may be on disk as the app's own.
    fn named(state: &State) -> Vec<String> {
        match state {
            State::Unknown => Vec::new(),
            State::Placed { placed } => vec![placed.md5.clone()],
            State::Writing { old, new } => old
                .iter()
                .map(|o| o.md5.clone())
                .chain([new.clone()])
                .collect(),
            State::Held { held } => vec![held.md5.clone()],
        }
    }

    /// Every (state, event, facts) of the table: bytes are written over or removed only
    /// when they are the app's own and the fold holds what they show, or they are what the
    /// project would write; a value that cannot be read is never written as absent; bytes
    /// the ledger never named are kept (recorded or held); a hold is never written over.
    #[test]
    fn every_pair_of_the_table_keeps_every_byte() {
        let mut pairs = 0;
        for state in states() {
            for event in events() {
                for facts in all_facts() {
                    pairs += 1;
                    let (after, actions) = next(&state, &event, &facts);
                    let destructive = actions
                        .iter()
                        .any(|a| matches!(a, Action::Write | Action::Remove));
                    let context =
                        format!("{state:?} + {event:?} with {facts:?} → {after:?} {actions:?}");
                    if let Event::Read(Disk::Bytes { md5 }) = &event {
                        if destructive {
                            assert!(!facts.newer_fold, "{context}");
                            assert!(facts.heads_readable, "{context}");
                            assert_eq!(facts.holds, Some(true), "{context}");
                            assert!(named(&state).contains(md5), "{context}");
                            assert!(!matches!(state, State::Held { .. }), "{context}");
                            // The bytes a write under way replaced are never taken as the
                            // app's: a restore can give them back with their size and time.
                            if let State::Writing {
                                old: Some(old),
                                new,
                            } = &state
                            {
                                assert!(&old.md5 != md5 || new == md5, "{context}");
                            }
                        }
                        let kept = actions
                            .iter()
                            .any(|a| matches!(a, Action::Record(_) | Action::Hold(_)));
                        let left = actions
                            .iter()
                            .all(|a| matches!(a, Action::Wait(_) | Action::Refresh));
                        assert!(
                            destructive || kept || left,
                            "bytes are either written over safely, kept, or left: {context}"
                        );
                        if !named(&state).contains(md5)
                            && !matches!(&facts.projection, Projection::Bytes { md5: e } if e == md5 && facts.heads_readable)
                            && !facts.newer_fold
                        {
                            assert!(kept, "bytes the ledger never named are kept: {context}");
                        }
                    }
                    if let Event::Read(Disk::Missing) = &event
                        && actions.iter().any(|a| matches!(a, Action::Rebuild { .. }))
                    {
                        assert!(
                            facts.heads_readable || facts.projection == Projection::NotAsked,
                            "{context}"
                        );
                    }
                    if matches!(event, Event::Read(Disk::Unreadable)) {
                        assert_eq!(after, state, "{context}");
                        assert_eq!(actions, vec![Action::Wait(WaitFor::FileUnreadable)]);
                    }
                    if let State::Held { .. } = &state
                        && !matches!(event, Event::Read(_) | Event::Restore)
                    {
                        assert!(
                            !destructive,
                            "a hold is written over only when restored: {context}"
                        );
                    }
                }
            }
        }
        assert_eq!(pairs, states().len() * events().len() * all_facts().len());
    }

    /// The review's cases, in the table's own terms.
    #[test]
    fn the_reviews_cases_are_kept() {
        let ok = Facts {
            projection: Projection::Bytes { md5: E.into() },
            heads_readable: true,
            holds: Some(true),
            same_stat: false,
            newer_fold: false,
        };
        let writing = State::Writing {
            old: Some(placed(A, true)),
            new: B.into(),
        };
        // A revert to the bytes a stopped write replaced, made while the app was closed.
        let (_, actions) = next(&writing, &Event::Read(Disk::Bytes { md5: A.into() }), &ok);
        assert_eq!(actions, vec![Action::Record(Base::Unknown)]);
        // The same bytes with the size and time they had: the rename never reached the disk,
        // or a backup tool put them back with their time. Both are kept.
        let untouched = Facts {
            same_stat: true,
            ..ok.clone()
        };
        let (after, actions) = next(
            &writing,
            &Event::Read(Disk::Bytes { md5: A.into() }),
            &untouched,
        );
        assert_eq!(actions, vec![Action::Record(Base::Unknown)]);
        assert_eq!(after, writing);
        // Bytes whose changes a journal file lost, or whose batch lost a member.
        let lost = Facts {
            holds: Some(false),
            ..ok.clone()
        };
        let (_, actions) = next(&writing, &Event::Read(Disk::Bytes { md5: B.into() }), &lost);
        assert_eq!(actions, vec![Action::Record(Base::Placed(B.into()))]);
        // A head whose value cannot be read.
        let unreadable = Facts {
            heads_readable: false,
            ..ok.clone()
        };
        let (_, actions) = next(
            &writing,
            &Event::Read(Disk::Bytes { md5: B.into() }),
            &unreadable,
        );
        assert_eq!(actions, vec![Action::Wait(WaitFor::ValuesUnreadable)]);
        // A second write begun names only the bytes the first replaced and the new ones: the
        // bytes of the first, put back, are kept.
        let (second, _) = next(&writing, &Event::WriteBegun { new: C.into() }, &ok);
        let (_, actions) = next(&second, &Event::Read(Disk::Bytes { md5: B.into() }), &ok);
        assert_eq!(actions, vec![Action::Record(Base::Unknown)]);
    }
}
