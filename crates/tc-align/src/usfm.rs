//! Alignments to and from aligned USFM (`\zaln-s`/`\zaln-e` milestones around `\w` words).
//!
//! Export: [`usfm_groups`] turns a value's cards into `tc-usfm` groups, and
//! [`tc_usfm::weave`] writes them into the verse in en_ult's layout, byte for byte what
//! tC4's export writes (`tc-usfm`'s `tests/tc4_weave.rs`). [`weave_verse`] does both.
//!
//! Import: [`from_aligned`] reads `tc-usfm`'s [`AlignedVerse`] back into cards and a bank
//! exactly as tC4 does (word-aligner's `unmerge`, through word-aligner-lib's
//! `extractAlignmentsFromTargetVerse`). `tc-usfm` gives one group per run of words under
//! the same milestones; tC4 keeps one card per outermost source word, so runs under the
//! same outermost milestone join one card. Where en_ult writes one source word over two
//! runs that sit next to each other (TIT 1:1 "of" and "the chosen people" under
//! ἐκλεκτῶν), the card holds both and the export writes them as one run: the same links,
//! in tC4's bytes, which differ from en_ult's in 5 of the 46 verses of Titus.

use crate::edit::card;
use crate::words::SourceVerse;
use tc_journal::json::JsonMap;
use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};
use tc_usfm::AlignedVerse;

/// The cards that align something, as `tc-usfm` groups for [`tc_usfm::weave`]: the
/// journal's [`AlignmentValue::usfm_groups`]. Cards with no placed word, or with no source
/// word, write nothing and are left out.
pub fn usfm_groups(value: &AlignmentValue) -> Vec<tc_usfm::AlignmentGroup> {
    value.usfm_groups()
}

/// The verse body `plain` with `value`'s alignment woven in. Fails as
/// [`tc_usfm::weave`] fails, for instance when the value names a word the verse does
/// not have: a caller weaves only a value that is valid for the verse (STORAGE.md §7.7),
/// as tC4's export does.
pub fn weave_verse(plain: &str, value: &AlignmentValue) -> Result<String, tc_usfm::Error> {
    tc_usfm::weave(plain, &value.usfm_groups())
}

/// Cards and a bank read from aligned USFM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub alignments: Vec<AlignmentGroup>,
    pub word_bank: Vec<TargetWord>,
}

/// Read an aligned verse into cards and a bank as tC4 reads it. See the module
/// documentation.
///
/// Without `source`, the cards come in the order their first run appears in the verse,
/// and only aligned source words have cards. With the original-language verse, the cards
/// are put in its order and every source word that nothing is aligned to gets an empty
/// card, as word-aligner's `orderAlignments` does: a card goes where its first source
/// word stands, a phrase's other words take no place of their own, and a card whose
/// first word the verse does not have goes last.
///
/// The bank holds every word of the verse that no card holds, grouped by occurrence as
/// tC4's bootstrap groups them, in the order each occurrence number first appears among
/// those words. (tC4 banks only words written as `\w`; a word left as plain text is
/// banked here too, so that every word of the verse is somewhere.)
pub fn from_aligned(aligned: &AlignedVerse, source: Option<&SourceVerse>) -> Extracted {
    let mut cards: Vec<(Option<(String, u32)>, AlignmentGroup)> = Vec::new();
    let mut placed = vec![false; aligned.words.len()];
    for run in aligned.groups.iter().filter(|g| !g.sources.is_empty()) {
        let outer = (run.sources[0].content.clone(), run.sources[0].occurrence);
        let at = match cards
            .iter()
            .position(|(key, _)| key.as_ref() == Some(&outer))
        {
            Some(at) => at,
            None => {
                cards.push((
                    Some(outer),
                    AlignmentGroup {
                        top_words: Vec::new(),
                        bottom_words: Vec::new(),
                        extra: JsonMap::new(),
                    },
                ));
                cards.len() - 1
            }
        };
        let card = &mut cards[at].1;
        if !run.targets.is_empty() {
            for word in &run.sources {
                let word = SourceWord::from_usfm(word);
                if !card.top_words.iter().any(|s| same_source(s, &word)) {
                    card.top_words.push(word);
                }
            }
        }
        for target in &run.targets {
            if let Some(slot) = aligned
                .words
                .iter()
                .position(|w| w.text == target.text && w.occurrence == target.occurrence)
            {
                placed[slot] = true;
            }
            card.bottom_words.push(TargetWord::from_usfm(target));
        }
    }
    // A milestone over no word adds no source word; a card left without one aligns
    // nothing and is not kept, as in word-aligner.
    let unordered: Vec<AlignmentGroup> = cards
        .into_iter()
        .map(|(_, g)| g)
        .filter(|g| !g.top_words.is_empty())
        .collect();
    let alignments = match source {
        Some(source) => order_cards(source, unordered),
        None => unordered,
    };
    Extracted {
        alignments,
        word_bank: occurrence_grouped(
            aligned
                .words
                .iter()
                .zip(&placed)
                .filter(|(_, placed)| !**placed)
                .map(|(w, _)| TargetWord::from_usfm(w)),
        ),
    }
}

fn same_source(a: &SourceWord, b: &SourceWord) -> bool {
    a.word == b.word && a.occurrence == b.occurrence
}

/// word-aligner's `orderAlignments` with the original verse's word list.
fn order_cards(source: &SourceVerse, mut unordered: Vec<AlignmentGroup>) -> Vec<AlignmentGroup> {
    let mut ordered: Vec<AlignmentGroup> = Vec::new();
    for word in source.words() {
        if let Some(at) = unordered
            .iter()
            .position(|g| g.top_words.first().is_some_and(|s| same_source(s, word)))
        {
            ordered.push(unordered.remove(at));
            continue;
        }
        let holds = |g: &AlignmentGroup| g.top_words.iter().any(|s| same_source(s, word));
        if !unordered.iter().any(holds) && !ordered.iter().any(holds) {
            ordered.push(card(word.clone()));
        }
    }
    ordered.extend(unordered);
    ordered
}

/// Words grouped by occurrence number, groups in the order their number first appears,
/// words in their given order within a group.
fn occurrence_grouped(words: impl Iterator<Item = TargetWord>) -> Vec<TargetWord> {
    let mut groups: Vec<(u32, Vec<TargetWord>)> = Vec::new();
    for word in words {
        match groups.iter_mut().find(|(n, _)| *n == word.occurrence) {
            Some((_, group)) => group.push(word),
            None => groups.push((word.occurrence, vec![word])),
        }
    }
    groups.into_iter().flat_map(|(_, g)| g).collect()
}
