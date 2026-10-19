//! A team's work day by day, as a computer that syncs every working day receives it.
//!
//! [`Corpus::generate`] gives the changes of three years of a team of three. This module puts
//! them where the real app would: each device seals what it wrote into packs at its
//! checkpoints (`seals_per_day` a working day: before each sync and at the end of the day),
//! merges its own packs by the rules of `docs/STORAGE.md` §4.7 (a completed month into one
//! pack, a current month's small packs when there are more than 32), and pushes its packs and
//! the removals of the packs it merged to a hub at its syncs. A computer outside the team that
//! pulls from the hub every day sees, day after day, which packs arrived and which went.
//! `Corpus::generate` is untouched: the timeline regroups its changes by the day and device
//! that wrote them, so the changes (and their ids) are the same ones.

use crate::history::{PackFile, START_MS};
use crate::rng::Rng;
use crate::{Corpus, CorpusError, Spec, Stats};
use std::collections::{BTreeMap, BTreeSet};
use tc_journal::consolidate::{OwnPack, consolidate, consolidation_groups};
use tc_journal::pack::{PackPath, PackRead, PackSpec, YearMonth, read_pack, seal};
use tc_journal::{DeviceId, PackName, Verified, WallTime};

const DAY_MS: i64 = 86_400_000;
const SEAL_EVERY_MS: i64 = 2 * 60 * 60 * 1000;

/// How the team seals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// Checkpoints a device makes in a working day.
    pub seals_per_day: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Self { seals_per_day: 4 }
    }
}

/// One working day, as the computer that pulls every day sees it.
#[derive(Debug, Clone)]
pub struct Day {
    /// Working days since the start, 0-based (five a week).
    pub day: u32,
    /// The evening of the day, when the computer would sync.
    pub evening: WallTime,
    pub month: YearMonth,
    /// The last working day of its calendar month.
    pub month_ends: bool,
    /// Packs that reached the hub since its last pull and are still there.
    pub add: Vec<PackFile>,
    /// Packs it holds that the team removed from the hub (merged into a newer pack).
    pub remove: Vec<String>,
    /// The changes in every pack it holds after today's pull.
    pub changes: usize,
}

/// The UTC time a working day starts: five working days a week (`history::day_start`).
fn day_start(day: i64) -> i64 {
    START_MS + (day / 5 * 7 + day % 5) * DAY_MS
}

/// The working day an `at` falls in.
fn day_of(at: i64) -> i64 {
    let calendar = (at - START_MS).div_euclid(DAY_MS);
    (calendar.div_euclid(7) * 5 + calendar.rem_euclid(7).min(4)).max(0)
}

/// A pack a device holds and may merge again.
struct Own {
    rel: String,
    path: PackPath,
    bytes: Vec<u8>,
    /// The pack's size, kept when its bytes are let go.
    size: u64,
}

enum Op {
    Add(PackFile),
    Remove(String),
}

struct Team {
    own: Vec<Own>,
    unpushed: Vec<Op>,
}

impl Corpus {
    /// Runs the team's three years day by day and calls `on_day` for each working day with
    /// what a computer that pulls every day receives. The changes are those of
    /// [`Corpus::generate`] for `spec`.
    pub fn timeline(
        spec: &Spec,
        schedule: &Schedule,
        on_day: &mut dyn FnMut(&Day) -> Result<(), String>,
    ) -> Result<Stats, CorpusError> {
        let corpus = Corpus::generate(spec)?;
        let project = corpus.journal.project;
        let devices = corpus.journal.devices.clone();
        let stats = corpus.journal.stats.clone();
        let mut lines = corpus.journal.lines();
        drop(corpus);
        let device_index = |id: DeviceId| devices.iter().position(|d| *d == id).unwrap_or(0);
        // Each line by the working day and device that wrote it, in the order it was written.
        let mut keyed: Vec<(i64, usize, u64, Verified)> = lines
            .drain(..)
            .filter_map(|line| {
                let body = line.as_change()?.body();
                Some((
                    day_of(body.at.0),
                    device_index(body.device),
                    body.seq.0,
                    line,
                ))
            })
            .collect();
        keyed.sort_by_key(|(day, device, seq, _)| (*day, *device, *seq));
        let last_day = keyed.last().map_or(0, |(day, ..)| *day);
        let mut rng = Rng::new(spec.seed ^ 0x7131_5eed);
        let mut team: Vec<Team> = devices
            .iter()
            .map(|_| Team {
                own: Vec::new(),
                unpushed: Vec::new(),
            })
            .collect();
        let mut hub: BTreeMap<String, PackFile> = BTreeMap::new();
        let mut delivered: BTreeMap<String, usize> = BTreeMap::new();
        let mut held = 0usize;
        let mut cursor = 0usize;
        let mut index = 0usize;
        let mut taken: Vec<(usize, Vec<Verified>)> = Vec::new();
        for day in 0..=last_day {
            // This day's lines, by device.
            taken.clear();
            while index < keyed.len() && keyed[index].0 == day {
                index += 1;
            }
            let todays = &mut keyed[cursor..index];
            let mut per_device: BTreeMap<usize, Vec<Verified>> = BTreeMap::new();
            for (_, device, _, line) in todays.iter() {
                per_device.entry(*device).or_default().push(line.clone());
            }
            cursor = index;
            for (device, mine) in per_device {
                let runs = split_runs(mine, schedule.seals_per_day.max(1) as usize);
                for (slot, run) in runs.into_iter().enumerate() {
                    let sealed = WallTime(day_start(day) + (slot as i64 + 1) * SEAL_EVERY_MS);
                    seal_run(&mut team[device], project, devices[device], sealed, run)?;
                }
            }
            // Syncs: most devices push at the end of most days, all of them on the last day.
            for t in team.iter_mut() {
                if day == last_day || rng.chance(spec.sync_rate) {
                    for op in std::mem::take(&mut t.unpushed) {
                        match op {
                            Op::Add(pack) => {
                                hub.insert(pack.path.clone(), pack);
                            }
                            Op::Remove(path) => {
                                hub.remove(&path);
                            }
                        }
                    }
                }
            }
            // The computer that pulls every day.
            let add: Vec<PackFile> = hub
                .values()
                .filter(|p| !delivered.contains_key(&p.path))
                .cloned()
                .collect();
            let remove: Vec<String> = delivered
                .keys()
                .filter(|path| !hub.contains_key(*path))
                .cloned()
                .collect();
            for path in &remove {
                held -= delivered.remove(path).unwrap_or(0);
            }
            for pack in &add {
                delivered.insert(pack.path.clone(), pack.changes);
                held += pack.changes;
            }
            let month = YearMonth::of(WallTime(day_start(day)));
            let next = YearMonth::of(WallTime(day_start(day + 1)));
            let today = Day {
                day: u32::try_from(day).unwrap_or(u32::MAX),
                evening: WallTime(day_start(day) + 10 * 60 * 60 * 1000),
                month,
                month_ends: month != next || day == last_day,
                add,
                remove,
                changes: held,
            };
            on_day(&today).map_err(CorpusError::Build)?;
        }
        Ok(stats)
    }
}

/// `lines` (in the order written) in `runs` consecutive runs of about equal size.
fn split_runs(lines: Vec<Verified>, runs: usize) -> Vec<Vec<Verified>> {
    let each = lines.len().div_ceil(runs).max(1);
    let mut out: Vec<Vec<Verified>> = Vec::new();
    let mut it = lines.into_iter().peekable();
    while it.peek().is_some() {
        out.push(it.by_ref().take(each).collect());
    }
    out
}

/// A device seals `lines` into packs of its own, then merges what §4.7 says to merge.
fn seal_run(
    team: &mut Team,
    project: tc_journal::ProjectId,
    device: DeviceId,
    sealed: WallTime,
    lines: Vec<Verified>,
) -> Result<(), CorpusError> {
    if lines.is_empty() {
        return Ok(());
    }
    let spec = PackSpec {
        project,
        sealer: device,
        sealed,
    };
    let built = seal(&spec, &lines).map_err(|e| CorpusError::Build(e.to_string()))?;
    for pack in built.packs {
        let rel = pack.path.to_string();
        team.unpushed.push(Op::Add(PackFile {
            path: rel.clone(),
            id: pack.id,
            bytes: pack.bytes.clone(),
            changes: pack.changes.len(),
        }));
        team.own.push(Own {
            rel,
            path: pack.path,
            size: pack.bytes.len() as u64,
            bytes: pack.bytes,
        });
    }
    // K6: merge what §4.7 says to merge.
    let listing: Vec<OwnPack> = team
        .own
        .iter()
        .map(|o| OwnPack {
            name: o.path.name,
            month: o.path.month,
            size: o.size,
        })
        .collect();
    for group in consolidation_groups(&listing, YearMonth::of(sealed)) {
        let names: BTreeSet<PackName> = group.into_iter().collect();
        let reads: Vec<(String, PackRead)> = team
            .own
            .iter()
            .filter(|o| names.contains(&o.path.name))
            .map(|o| (o.rel.clone(), read_pack(&o.rel, &o.bytes, project)))
            .collect();
        let refs: Vec<&PackRead> = reads.iter().map(|(_, r)| r).collect();
        let merged = consolidate(&spec, &refs).map_err(|e| CorpusError::Build(e.to_string()))?;
        for pack in merged {
            let rel = pack.path.to_string();
            let replaced: BTreeSet<PackName> = read_pack(&rel, &pack.bytes, project)
                .header
                .map(|h| h.replaces.into_iter().collect())
                .unwrap_or_default();
            let gone: Vec<String> = team
                .own
                .iter()
                .filter(|o| replaced.contains(&o.path.name))
                .map(|o| o.rel.clone())
                .collect();
            team.own.retain(|o| !replaced.contains(&o.path.name));
            for path in gone {
                team.unpushed.push(Op::Remove(path));
            }
            team.unpushed.push(Op::Add(PackFile {
                path: rel.clone(),
                id: pack.id,
                bytes: pack.bytes.clone(),
                changes: pack.changes.len(),
            }));
            team.own.push(Own {
                rel,
                path: pack.path,
                size: pack.bytes.len() as u64,
                bytes: pack.bytes,
            });
        }
    }
    // Older months are one pack each and are not read again: their bytes go.
    let current = YearMonth::of(sealed);
    for own in &mut team.own {
        if own.path.month < current {
            own.bytes = Vec::new();
        }
    }
    Ok(())
}
