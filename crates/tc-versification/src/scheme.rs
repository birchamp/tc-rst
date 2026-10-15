//! A compiled scheme, and verse mapping between schemes through `org`.

use crate::book::BookCode;
use crate::spec::{Mapping, VerseRef};
use crate::table::{CompileError, Hop, MappingTable};
use crate::versification::{Absence, Versification};
use std::collections::HashSet;
use std::ops::RangeInclusive;

/// A versification scheme with its mapping tables built, ready to map verses.
#[derive(Debug, Clone)]
pub struct Scheme {
    name: String,
    versification: Versification,
    to_org: MappingTable,
    from_org: MappingTable,
    excluded: HashSet<VerseRef>,
}

impl Scheme {
    /// `name` identifies the scheme the way tC4 does (`eng`, `rsc`, ...). Two schemes with
    /// the same name are the same frame, and `map_reference` passes references between
    /// them through unchanged.
    pub fn new(
        name: impl Into<String>,
        versification: Versification,
    ) -> Result<Self, CompileError> {
        Ok(Self {
            name: name.into(),
            to_org: MappingTable::forward(versification.mappings())?,
            from_org: MappingTable::reverse(versification.mappings())?,
            excluded: versification.excluded_verses().iter().copied().collect(),
            versification,
        })
    }

    /// Whether the scheme lists this verse in `excludedVerses`.
    pub fn is_excluded(&self, verse: VerseRef) -> bool {
        self.excluded.contains(&verse)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn versification(&self) -> &Versification {
        &self.versification
    }

    /// Where a verse of this scheme is in `org`: Proskomma's `mapVerse` through this
    /// scheme's own `mappedVerses`.
    pub fn to_org(&self, verse: VerseRef) -> Hop<'_> {
        self.to_org.apply(verse)
    }

    /// Where an `org` verse is in this scheme: `mapVerse` through the reversed table.
    pub fn from_org(&self, verse: VerseRef) -> Hop<'_> {
        self.from_org.apply(verse)
    }
}

/// One verse mapped from one scheme to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseMapping {
    pub source: VerseRef,
    /// One landing per verse the source has in `org`, in order. Usually one.
    pub landings: Vec<Landing>,
    /// Everything that keeps this from being a faithful one-to-one move. Empty means the
    /// source is a verse of the source scheme, it lands on exactly one verse of the
    /// target scheme, neither scheme excludes either verse, the target maps back to the
    /// source alone, and no rule on the way was bent or overruled.
    pub losses: Vec<Loss>,
}

/// Where one `org` verse lands in the target scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    pub pivot: VerseRef,
    /// Never empty; all in one book.
    pub targets: Vec<VerseRef>,
}

/// A way a mapping loses or bends information. Each is reported, never dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loss {
    /// The source verse is not in the source scheme's verse counts. Proskomma maps it
    /// anyway.
    SourceMissing { absence: Absence },
    /// The source scheme lists the source verse in `excludedVerses`.
    SourceExcluded,
    /// A target verse does not exist in the target scheme.
    Missing { verse: VerseRef, absence: Absence },
    /// The target scheme lists a target verse in `excludedVerses`: it numbers the verse,
    /// but a text in that scheme leaves it out.
    Excluded { verse: VerseRef },
    /// The source lands on this many distinct verses.
    FanOut { count: usize },
    /// Mapping the landing back gives these verses instead of the source alone: other
    /// verses share the landing, or the way back goes elsewhere.
    NotReversible { back: Vec<VerseRef> },
    /// An applied rule maps part of a verse (`ESG 1:1a`); the whole verse is used.
    PartialVerse { rule: Mapping },
    /// An applied rule's target range is not as long as its source range. Proskomma
    /// counts from the first target verse and ignores the declared end, so the landing
    /// can miss declared verses or run past them.
    LengthMismatch { rule: Mapping },
    /// An applied rule names targets in more than one book. Proskomma puts all of them in
    /// one book.
    MixedBooks { rule: Mapping },
    /// A later rule also holds the verse, and Proskomma ignored it.
    Shadowed { rule: Mapping },
}

impl VerseMapping {
    /// Every target verse, landing by landing. Can repeat.
    pub fn targets(&self) -> impl Iterator<Item = VerseRef> + '_ {
        self.landings.iter().flat_map(|l| l.targets.iter().copied())
    }

    pub fn is_lossless(&self) -> bool {
        self.losses.is_empty()
    }

    /// The target, when the mapping is lossless.
    pub fn exact(&self) -> Option<VerseRef> {
        if self.is_lossless() {
            self.targets().next()
        } else {
            None
        }
    }
}

/// The two hops, exactly as Proskomma's `do_cv` makes them: forward through `from`'s
/// table, then each `org` verse back through `to`'s reversed table.
///
/// Proskomma then flattens the landings and labels them all with the first landing's
/// book. This keeps each landing's own book. `do_cv` also skips the reverse hop when the
/// target docSet has no document for the pivot book (`org` has no `DAG`, so an `eng`
/// `S3Y` verse reads nothing from an `org` docSet); tC4 maps through the tables alone,
/// and so does this.
fn compose<'a>(
    from: &'a Scheme,
    to: &'a Scheme,
    verse: VerseRef,
) -> (Hop<'a>, Vec<(VerseRef, Hop<'a>)>) {
    let forward = from.to_org(verse);
    let back = forward
        .targets()
        .map(|pivot| (pivot, to.from_org(pivot)))
        .collect();
    (forward, back)
}

/// Maps one verse from `from` to `to` through `org`, as Proskomma does, and reports
/// every loss.
///
/// This always composes the two tables, even when `from` and `to` are the same scheme;
/// `map_reference` is where same-frame references pass through untouched. Like
/// Proskomma, it maps a verse the source scheme does not have, and reports that as a
/// loss. Unlike `map_reference`, which follows tC4, it reports excluded verses.
pub fn map_verse(from: &Scheme, to: &Scheme, verse: VerseRef) -> VerseMapping {
    let (forward, back) = compose(from, to, verse);
    let mut losses = Vec::new();
    if let Some(absence) = from.versification().absence(verse) {
        losses.push(Loss::SourceMissing { absence });
    } else if from.is_excluded(verse) {
        losses.push(Loss::SourceExcluded);
    }
    for hop in std::iter::once(&forward).chain(back.iter().map(|(_, hop)| hop)) {
        if let Some(rule) = hop.rule() {
            for loss in rule_losses(rule) {
                push_unique(&mut losses, loss);
            }
        }
        for rule in hop.shadowed() {
            push_unique(
                &mut losses,
                Loss::Shadowed {
                    rule: (*rule).clone(),
                },
            );
        }
    }
    let landings: Vec<Landing> = back
        .into_iter()
        .map(|(pivot, hop)| Landing {
            pivot,
            targets: hop.targets().collect(),
        })
        .collect();

    let targets = distinct(landings.iter().flat_map(|l| l.targets.iter().copied()));
    if targets.len() > 1 {
        losses.push(Loss::FanOut {
            count: targets.len(),
        });
    }
    for &target in &targets {
        if let Some(absence) = to.versification().absence(target) {
            losses.push(Loss::Missing {
                verse: target,
                absence,
            });
        } else if to.is_excluded(target) {
            losses.push(Loss::Excluded { verse: target });
        }
    }
    let back = distinct(targets.iter().flat_map(|&target| {
        let (_, hops) = compose(to, from, target);
        hops.into_iter()
            .flat_map(|(_, hop)| hop.targets().collect::<Vec<_>>())
    }));
    if back != [verse] {
        losses.push(Loss::NotReversible { back });
    }
    VerseMapping {
        source: verse,
        landings,
        losses,
    }
}

/// Maps every verse of a span, one at a time. Proskomma has no span mapping of its
/// own; tC4 maps a span the same way, verse by verse.
pub fn map_span(
    from: &Scheme,
    to: &Scheme,
    book: BookCode,
    chapter: u16,
    verses: RangeInclusive<u16>,
) -> Vec<VerseMapping> {
    verses
        .map(|verse| map_verse(from, to, VerseRef::new(book, chapter, verse)))
        .collect()
}

/// The ways a rule's own shape makes Proskomma's arithmetic disagree with what the rule
/// says.
fn rule_losses(rule: &Mapping) -> Vec<Loss> {
    let mut losses = Vec::new();
    let source = rule.source();
    let targets = rule.targets();
    if source.part.is_some() || targets.iter().any(|t| t.part.is_some()) {
        losses.push(Loss::PartialVerse { rule: rule.clone() });
    }
    if source.extent() < 0 || targets.iter().any(|t| t.extent() != source.extent()) {
        losses.push(Loss::LengthMismatch { rule: rule.clone() });
    }
    // `Mapping` guarantees at least one target.
    let book = targets[0].book;
    if targets.iter().any(|t| t.book != book) {
        losses.push(Loss::MixedBooks { rule: rule.clone() });
    }
    losses
}

fn push_unique(losses: &mut Vec<Loss>, loss: Loss) {
    if !losses.contains(&loss) {
        losses.push(loss);
    }
}

fn distinct(verses: impl Iterator<Item = VerseRef>) -> Vec<VerseRef> {
    let mut out: Vec<VerseRef> = Vec::new();
    for v in verses {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}
