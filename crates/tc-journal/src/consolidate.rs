//! Consolidation and redundant packs (`docs/STORAGE.md` §4.7), as pure functions over pack
//! listings. Packs are never edited: a device merges its own packs into new ones whose
//! header `replaces` names the old ones, and the redundancy rule decides, from the file set
//! alone, which packs a newer pack has replaced.

use crate::change::Verified;
use crate::ids::{ChangeId, PackName};
use crate::line::LINE_OVERHEAD;
use crate::pack::{
    EncodedPack, MAX_PACK_BYTES, PackFacts, PackRead, PackSpec, PackWriteError, YearMonth,
    encode_pack, header_line,
};
use std::collections::{BTreeMap, BTreeSet};

/// Packs under this size count as small for the current month's trigger.
pub const SMALL_PACK_BYTES: u64 = 1024 * 1024;
/// The current month is consolidated when it holds more small packs than this.
pub const SMALL_PACKS_TRIGGER: usize = 32;
/// A pack this large is already worth its file: consolidation neither reads nor rewrites it
/// (a resource upgrade writes packs of 48 MiB, and merging a few small ones into such a pack
/// would rewrite its 48 MiB for their sake).
pub const LARGE_PACK_BYTES: u64 = 16 * 1024 * 1024;

/// One of this device's packs, as a directory listing shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnPack {
    pub name: PackName,
    pub month: YearMonth,
    pub size: u64,
}

/// Which of this device's packs to merge at a seal (§4.7 "When"): every month before
/// `current` that holds two or more packs (all of them but the [`LARGE_PACK_BYTES`] ones, one
/// group per month), and the current month's small packs when there are more than
/// [`SMALL_PACKS_TRIGGER`] of them.
/// Groups come in month order; names in each group are sorted.
pub fn consolidation_groups(own: &[OwnPack], current: YearMonth) -> Vec<Vec<PackName>> {
    let mut by_month: BTreeMap<YearMonth, BTreeSet<PackName>> = BTreeMap::new();
    let mut small: BTreeSet<PackName> = BTreeSet::new();
    for pack in own.iter().filter(|p| p.size < LARGE_PACK_BYTES) {
        if pack.month < current {
            by_month.entry(pack.month).or_default().insert(pack.name);
        } else if pack.month == current && pack.size < SMALL_PACK_BYTES {
            small.insert(pack.name);
        }
    }
    let mut groups: Vec<Vec<PackName>> = by_month
        .into_values()
        .filter(|names| names.len() >= 2)
        .map(|names| names.into_iter().collect())
        .collect();
    if small.len() > SMALL_PACKS_TRIGGER {
        groups.push(small.into_iter().collect());
    }
    groups
}

/// Builds the packs that replace `packs` (§4.7 "What"). Only packs that passed P2 to P4 on
/// this device take part; others are left as they are. The new pack's lines are the union
/// of theirs and its `replaces` lists their names and every name they listed. When the
/// union is over 64 MiB, whole old packs are grouped, in the order of their first line, into
/// new packs of at most 64 MiB, each listing what it absorbed. A group of one pack is not
/// rewritten, and an old pack too large to share a new pack stays as it is.
pub fn consolidate(
    spec: &PackSpec,
    packs: &[&PackRead],
) -> Result<Vec<EncodedPack>, PackWriteError> {
    let mut valid: Vec<&PackRead> = packs
        .iter()
        .copied()
        .filter(|pack| pack.is_valid() && pack.path.is_some() && !pack.entries.is_empty())
        .collect();
    valid.sort_by_key(|pack| (pack.entries.iter().map(Verified::sort_key).min(), pack.id));
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut current = Group::default();
    for pack in valid {
        if !current.fits(spec, pack) {
            groups.push(std::mem::take(&mut current));
        }
        if current.fits(spec, pack) {
            current.add(pack);
        }
    }
    groups.push(current);
    groups
        .into_iter()
        .filter(|group| group.packs.len() >= 2)
        .map(|group| group.encode(spec))
        .collect()
}

/// Old packs that will become one new pack.
#[derive(Default)]
struct Group<'a> {
    packs: Vec<&'a PackRead>,
    ids: BTreeSet<ChangeId>,
    body: usize,
    replaces: BTreeSet<PackName>,
}

impl<'a> Group<'a> {
    fn names_of(pack: &PackRead) -> impl Iterator<Item = PackName> + '_ {
        pack.path
            .iter()
            .map(|path| path.name)
            .chain(pack.header.iter().flat_map(|h| h.replaces.iter().copied()))
    }

    /// True when the group with `pack` added still fits one pack.
    fn fits(&self, spec: &PackSpec, pack: &PackRead) -> bool {
        let mut count = self.ids.len();
        let mut body = self.body;
        let mut seen = BTreeSet::new();
        for entry in &pack.entries {
            if !self.ids.contains(&entry.id()) && seen.insert(entry.id()) {
                count += 1;
                body += entry.raw().len() + LINE_OVERHEAD;
            }
        }
        let mut replaces = self.replaces.clone();
        replaces.extend(Self::names_of(pack));
        let replaces: Vec<PackName> = replaces.into_iter().collect();
        header_line(spec, count, &replaces).len() + body <= MAX_PACK_BYTES
    }

    fn add(&mut self, pack: &'a PackRead) {
        for entry in &pack.entries {
            if self.ids.insert(entry.id()) {
                self.body += entry.raw().len() + LINE_OVERHEAD;
            }
        }
        self.replaces.extend(Self::names_of(pack));
        self.packs.push(pack);
    }

    fn encode(self, spec: &PackSpec) -> Result<EncodedPack, PackWriteError> {
        let lines: Vec<Verified> = self
            .packs
            .iter()
            .flat_map(|pack| pack.entries.iter().cloned())
            .collect();
        let replaces: Vec<PackName> = self.replaces.into_iter().collect();
        encode_pack(spec, &lines, &replaces)
    }
}

/// What the redundancy rule decides for a set of pack files (a working tree, a merged
/// commit tree, an exchange file).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Redundancy {
    /// `(redundant, replaced_by)`: indexes into the listing. Redundant packs are left out of
    /// every built tree and removed from the working tree once the replacing pack is
    /// durable.
    pub redundant: Vec<(usize, usize)>,
    /// Packs a valid pack lists but that hold a change it lacks (a bug): kept, reported.
    pub kept: Vec<RedundancyDefect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedundancyDefect {
    pub pack: usize,
    pub listed_by: usize,
    pub missing: Vec<ChangeId>,
}

/// The redundancy rule (§4.7): pack F is redundant if another pack P in the same sealer
/// directory passes P2 to P4 and lists F's name in `replaces`. When F is valid too, every
/// change id of F must be in P; if one is missing, that P does not make F redundant, and the
/// defect is reported. A function of the file set, so every device converges on the same
/// tree. It reads each pack's facts, from a read of the file or from the fold cache
/// ([`PackFacts`]), never a read whose changes were moved out.
pub fn redundancy(packs: &[PackFacts]) -> Redundancy {
    let mut result = Redundancy::default();
    for (f_index, f) in packs.iter().enumerate() {
        let Some(f_path) = &f.path else { continue };
        let mut replaced_by = None;
        let mut defects = Vec::new();
        for (p_index, p) in packs.iter().enumerate() {
            let (Some(p_path), Some(header)) = (&p.path, &p.header) else {
                continue;
            };
            let lists_f = p_index != f_index
                && p.is_valid()
                && p_path.sealer == f_path.sealer
                && header.replaces.binary_search(&f_path.name).is_ok();
            if !lists_f {
                continue;
            }
            if f.is_valid() {
                let missing: Vec<ChangeId> =
                    f.ids.iter().copied().filter(|id| !p.holds(*id)).collect();
                if !missing.is_empty() {
                    defects.push(RedundancyDefect {
                        pack: f_index,
                        listed_by: p_index,
                        missing,
                    });
                    continue;
                }
            }
            replaced_by.get_or_insert(p_index);
        }
        match replaced_by {
            Some(p_index) => result.redundant.push((f_index, p_index)),
            None => result.kept.extend(defects),
        }
    }
    result
}
