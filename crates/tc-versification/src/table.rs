//! A port of proskomma-core 0.11.3's verse-mapping tables (`src/util/versification.js`:
//! `reverseVersification`, `preSuccinctVerseMapping`, `succinctifyVerseMappings` and
//! `mapVerse`).
//!
//! Proskomma packs each chapter's rules into a byte array. This port keeps the rules as
//! plain values but reproduces every observable rule of the encoding:
//!
//! - rules are tried in table order and the first whose source range holds the verse wins;
//! - a verse maps to `first target verse + (verse - first source verse)` for each target,
//!   so a target's declared end and its sub-verse letter play no part;
//! - every target of a rule lands in one book: the first target's book when it differs
//!   from the source book, otherwise the source book;
//! - a chapter with no rules, or a verse no rule holds, maps to itself;
//! - the encoding refuses a rule that crosses into a book missing from Proskomma's book
//!   list, and a rule longer than 63 bytes. This port refuses the same rules.

use crate::book::BookCode;
use crate::spec::{MappedRange, Mapping, VerseRef};
use std::collections::HashMap;

/// The compiled mapping rules of one direction of one scheme.
#[derive(Debug, Clone)]
pub struct MappingTable {
    chapters: HashMap<(BookCode, u16), Vec<Rule>>,
}

#[derive(Debug, Clone)]
struct Rule {
    first: u16,
    last: u16,
    book: BookCode,
    /// `(chapter, first verse)` of each target.
    starts: Vec<(u16, u16)>,
    mapping: Mapping,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error(
        "mapping `{mapping}` crosses into book {book}, which Proskomma cannot encode because it is not in its book list"
    )]
    UnknownTargetBook { mapping: String, book: BookCode },
    #[error("mapping `{mapping}` needs {bytes} bytes; Proskomma encodes at most 63 per mapping")]
    TooLong { mapping: String, bytes: usize },
    #[error("mapping `{mapping}` would number a target verse past 65535")]
    VerseOverflow { mapping: String },
}

/// Where one verse lands through one table: Proskomma's `[book, [[chapter, verse], ...]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop<'a> {
    /// Never empty, and every entry is in the same book. More than one entry is a
    /// one-to-many mapping. Entries can repeat when a source lists a target twice.
    pub targets: Vec<VerseRef>,
    /// The rule that moved the verse, as this table applies it. `None` when no rule holds
    /// the verse and it maps to itself.
    pub rule: Option<&'a Mapping>,
    /// Later rules that also hold the verse. Proskomma applies only the first.
    pub shadowed: Vec<&'a Mapping>,
}

impl Hop<'_> {
    pub fn book(&self) -> BookCode {
        // `apply` always returns at least one target.
        self.targets[0].book
    }
}

impl MappingTable {
    /// The table that takes a scheme's verses into `org`, from its own `mappedVerses`.
    pub fn forward(mappings: &[Mapping]) -> Result<Self, CompileError> {
        Self::compile(mappings.iter().cloned())
    }

    /// The table that takes `org` verses back into a scheme, from the inverse of its
    /// `mappedVerses` (Proskomma's `reverseVersification`).
    pub fn reverse(mappings: &[Mapping]) -> Result<Self, CompileError> {
        Self::compile(reverse_mappings(mappings))
    }

    fn compile(mappings: impl IntoIterator<Item = Mapping>) -> Result<Self, CompileError> {
        let mut chapters: HashMap<(BookCode, u16), Vec<Rule>> = HashMap::new();
        for mapping in mappings {
            let rule = Rule::compile(mapping)?;
            let source = rule.mapping.source();
            chapters
                .entry((source.book, source.chapter))
                .or_default()
                .push(rule);
        }
        Ok(Self { chapters })
    }

    /// Proskomma's `mapVerse`, with the chapter lookup tC4 (`applyOne`) and Proskomma
    /// (`do_cv`) put in front of it.
    pub fn apply(&self, verse: VerseRef) -> Hop<'_> {
        let unchanged = Hop {
            targets: vec![verse],
            rule: None,
            shadowed: Vec::new(),
        };
        let Some(rules) = self.chapters.get(&(verse.book, verse.chapter)) else {
            return unchanged;
        };
        let mut holding = rules
            .iter()
            .filter(|r| r.first <= verse.verse && verse.verse <= r.last);
        let Some(rule) = holding.next() else {
            return unchanged;
        };
        let offset = verse.verse - rule.first;
        Hop {
            targets: rule
                .starts
                .iter()
                // `Rule::compile` refuses a rule whose targets could pass u16::MAX, so
                // this never saturates.
                .map(|&(chapter, start)| {
                    VerseRef::new(rule.book, chapter, start.saturating_add(offset))
                })
                .collect(),
            rule: Some(&rule.mapping),
            shadowed: holding.map(|r| &r.mapping).collect(),
        }
    }
}

impl Rule {
    fn compile(mapping: Mapping) -> Result<Self, CompileError> {
        let source = *mapping.source();
        // `Mapping` guarantees at least one target.
        let first_target = mapping.targets()[0];
        let crosses_book = first_target.book != source.book;
        let book = if crosses_book {
            first_target.book
        } else {
            source.book
        };
        let last = source.end();
        let starts: Vec<(u16, u16)> = mapping
            .targets()
            .iter()
            .map(|t| (t.chapter, t.first))
            .collect();

        let mut bytes = 1 + nbyte_len(source.first.into()) + nbyte_len(last.into());
        if crosses_book {
            let index = book
                .proskomma_index()
                .ok_or_else(|| CompileError::UnknownTargetBook {
                    mapping: mapping.to_string(),
                    book,
                })?;
            bytes += nbyte_len(index);
        }
        bytes += nbyte_len(starts.len());
        for &(chapter, start) in &starts {
            bytes += nbyte_len(chapter.into()) + nbyte_len(start.into());
        }
        if bytes > 63 {
            return Err(CompileError::TooLong {
                mapping: mapping.to_string(),
                bytes,
            });
        }
        if let Some(width) = last.checked_sub(source.first)
            && starts
                .iter()
                .any(|&(_, start)| start.checked_add(width).is_none())
        {
            return Err(CompileError::VerseOverflow {
                mapping: mapping.to_string(),
            });
        }
        Ok(Self {
            first: source.first,
            last,
            book,
            starts,
            mapping,
        })
    }
}

/// Proskomma's variable-length integer: seven bits per byte.
fn nbyte_len(mut value: usize) -> usize {
    let mut len = 1;
    while value > 127 {
        value >>= 7;
        len += 1;
    }
    len
}

/// Proskomma's `reverseVersification`: every target becomes a source whose targets are
/// the sources that named it, keyed by the target exactly as written and ordered by
/// first appearance. `PSA 51:2` and `PSA 51:2a` stay distinct keys.
pub fn reverse_mappings(mappings: &[Mapping]) -> Vec<Mapping> {
    let mut reversed: Vec<Mapping> = Vec::new();
    let mut index: HashMap<MappedRange, usize> = HashMap::new();
    for mapping in mappings {
        for target in mapping.targets() {
            match index.get(target) {
                Some(&at) => reversed[at].push_target(*mapping.source()),
                None => {
                    index.insert(*target, reversed.len());
                    reversed.push(Mapping::single(*target, *mapping.source()));
                }
            }
        }
    }
    reversed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(s: &str) -> MappedRange {
        MappedRange::parse_target(s).unwrap()
    }

    fn mapping(from: &str, to: &[&str]) -> Mapping {
        Mapping::new(range(from), to.iter().map(|t| range(t)).collect()).unwrap()
    }

    fn verse(s: &str) -> VerseRef {
        s.parse().unwrap()
    }

    #[test]
    fn first_rule_wins_and_offsets_count_from_the_first_target() {
        let table = MappingTable::forward(&[
            mapping("S3Y 1:1-29", &["DAG 3:24-52"]),
            mapping("S3Y 1:29-31", &["DAG 3:60"]),
        ])
        .unwrap();
        let hop = table.apply(verse("S3Y 1:29"));
        assert_eq!(hop.targets, vec![verse("DAG 3:52")]);
        assert_eq!(hop.shadowed.len(), 1);
        // The second rule's single target is spread over the source range.
        assert_eq!(
            table.apply(verse("S3Y 1:31")).targets,
            vec![verse("DAG 3:62")]
        );
        assert_eq!(
            table.apply(verse("S3Y 1:32")).targets,
            vec![verse("S3Y 1:32")]
        );
        assert_eq!(table.apply(verse("S3Y 2:1")).rule, None);
    }

    #[test]
    fn all_targets_take_one_book() {
        let table = MappingTable::forward(&[mapping("PSA 1:1", &["PSA 2:1", "MAN 1:1"])]).unwrap();
        assert_eq!(
            table.apply(verse("PSA 1:1")).targets,
            vec![verse("PSA 2:1"), verse("PSA 1:1")]
        );
        let table = MappingTable::forward(&[mapping("PSA 1:1", &["MAN 2:1", "PSA 1:1"])]).unwrap();
        assert_eq!(
            table.apply(verse("PSA 1:1")).targets,
            vec![verse("MAN 2:1"), verse("MAN 1:1")]
        );
    }

    #[test]
    fn reversal_keys_by_the_exact_target_text() {
        let reversed = reverse_mappings(&[
            mapping("ESG 1:1", &["ESG 1:1a"]),
            mapping("ESG 1:2", &["ESG 1:1b"]),
            mapping("ESG 1:3", &["ESG 1:1b"]),
        ]);
        assert_eq!(reversed.len(), 2);
        assert_eq!(reversed[1].to_string(), "ESG 1:1b = ESG 1:2, ESG 1:3");
        let table = MappingTable::reverse(&[
            mapping("ESG 1:1", &["ESG 1:1a"]),
            mapping("ESG 1:2", &["ESG 1:1b"]),
        ])
        .unwrap();
        // Both reversed rules hold ESG 1:1; the `a` rule comes first.
        let hop = table.apply(verse("ESG 1:1"));
        assert_eq!(hop.targets, vec![verse("ESG 1:1")]);
        assert_eq!(hop.shadowed.len(), 1);
    }

    #[test]
    fn refuses_what_proskomma_cannot_encode() {
        assert!(matches!(
            MappingTable::forward(&[mapping("PSA 1:1", &["XXA 1:1"])]),
            Err(CompileError::UnknownTargetBook { .. })
        ));
        // Same-book rules need no book index, so any code works there.
        assert!(MappingTable::forward(&[mapping("XXA 1:1", &["XXA 1:2"])]).is_ok());
        let many: Vec<String> = (1..=30).map(|v| format!("PSA 2:{v}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(matches!(
            MappingTable::forward(&[mapping("PSA 1:1", &many)]),
            Err(CompileError::TooLong { bytes: 64, .. })
        ));
        assert!(MappingTable::forward(&[mapping("PSA 1:1", &many[..29])]).is_ok());
        assert!(matches!(
            MappingTable::forward(&[mapping("PSA 1:1-10", &["PSA 1:65530"])]),
            Err(CompileError::VerseOverflow { .. })
        ));
    }

    #[test]
    fn variable_length_integers() {
        assert_eq!(nbyte_len(0), 1);
        assert_eq!(nbyte_len(127), 1);
        assert_eq!(nbyte_len(128), 2);
        assert_eq!(nbyte_len(16383), 2);
        assert_eq!(nbyte_len(16384), 3);
    }
}
