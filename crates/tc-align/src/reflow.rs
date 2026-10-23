//! Keeping an alignment across an edit of its verse text (tC4 #213 `reflowAlignment`).
//!
//! A reflow keeps every link whose target word the new text still has, with its occurrence
//! updated; drops the links to words that are gone; rebuilds the bank from the new text's
//! other words in verse order; and never touches a card's source words or makes a link. It
//! reports each dropped link in [`Reflowed::dropped`].
//!
//! Each link follows its own word, by position (tC4 #789, the owner's Q30 (a)). Given the
//! text the value was made against ([`reflow_edit`]), the words of the two texts are matched
//! as a longest common subsequence by text, and a word goes where every such match puts it;
//! when it is not clear where a linked word went (an earlier copy of it was added or
//! deleted, and the texts do not tell which), its link returns to the bank and is reported.
//! Without that text ([`reflow`]), only a word the verse holds once, and held once when it
//! was placed, is clear: a link to a repeated word returns to the bank and is reported,
//! since a copy deleted and another typed elsewhere leaves the same count. So a link never
//! moves to another copy of its word. The verse is not marked for review for it.
//!
//! Nothing here writes. tC4 writes the reflow of every edited verse at draft save. Here the
//! store writes it in the batch of the verse save (APP-DESIGN R25, STORAGE.md §5.1 step 5,
//! §5.2): the reflowed value, lossless or not, becomes the alignment's head, and the value
//! with every link stays in History as its parent, so a dropped link is never gone. A save
//! does not reflow an alignment in conflict, one made for another span, one marked invalid
//! or a verse whose text holds its alignment markup; such a value keeps computing invalid,
//! since its hash no longer matches (§7.7), and the aligner opens the verse with its
//! reflowed value ([`crate::Aligner::open`]), showing the links the reflow would drop, and
//! saves only when the translator edits the verse or keeps the reflow.
use std::collections::HashMap;

use crate::edit::Alignment;
use crate::value::holds_the_words_of;
use crate::words::TargetVerse;
use tc_core::Md5;
use tc_journal::json::JsonMap;
use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};

/// A link the new text cannot keep: the word is gone, or it is not clear where it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedLink {
    /// The card the word was on, by its index in the value.
    pub card: usize,
    /// The card's source words.
    pub top_words: Vec<SourceWord>,
    /// The target word as the old value recorded it.
    pub word: TargetWord,
}

/// The result of a reflow that changed something.
#[derive(Debug, Clone, PartialEq)]
pub struct Reflowed {
    /// The value for the new text: kept links, the rest of the words in the bank, made
    /// against the new text, `done` recomputed.
    pub value: AlignmentValue,
    /// The links that were dropped, in card order.
    pub dropped: Vec<DroppedLink>,
}

impl Reflowed {
    /// Every link was kept: the reflow only renumbered words and banked new ones.
    pub fn is_lossless(&self) -> bool {
        self.dropped.is_empty()
    }
}

/// Why a reflow left the value as it is. In every case the caller keeps the stored value
/// unchanged and its computed validity stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoReflow {
    /// The verse holds alignment markup in its text, which is its alignment (tC4 #794).
    AlignedInText,
    /// The value was made against this very text and holds its words.
    Current,
    /// tC3 or tC4 marked it for re-review (a span created or broken): a person decides.
    MarkedInvalid,
    /// Nothing was placed, so there is nothing to keep.
    NothingPlaced,
    /// The new text has no words.
    NoWords,
}

/// Carry `value` over to `verse`, without the text it was made against, or say why the
/// value stays as it is. See the module documentation.
pub fn reflow(value: &AlignmentValue, verse: &TargetVerse) -> Result<Reflowed, NoReflow> {
    reflow_from(value, None, verse)
}

/// Carry `value` over an edit of its verse from `before` to `after`, as a verse or section
/// save does, or say why the value stays as it is. Each link follows its word by position
/// when `value` was made against `before`; otherwise this is [`reflow`]. See the module
/// documentation.
pub fn reflow_edit(
    value: &AlignmentValue,
    before: &TargetVerse,
    after: &TargetVerse,
) -> Result<Reflowed, NoReflow> {
    reflow_from(value, Some(before), after)
}

fn reflow_from(
    value: &AlignmentValue,
    before: Option<&TargetVerse>,
    verse: &TargetVerse,
) -> Result<Reflowed, NoReflow> {
    if verse.holds_alignment_markup() {
        return Err(NoReflow::AlignedInText);
    }
    if value.marked_invalid {
        return Err(NoReflow::MarkedInvalid);
    }
    if value.placed() == 0 {
        return Err(NoReflow::NothingPlaced);
    }
    if !value.is_stale_against(verse) && holds_the_words_of(value, verse) {
        return Err(NoReflow::Current);
    }
    if verse.words().is_empty() {
        return Err(NoReflow::NoWords);
    }
    Ok(carry(value, before, verse))
}

/// The reflow itself, without tC4's reasons to leave a value alone. The aligner uses it
/// to show any stale value against the verse it shows, so that every word it offers is a
/// word of that verse.
pub(crate) fn carry(
    value: &AlignmentValue,
    before: Option<&TargetVerse>,
    verse: &TargetVerse,
) -> Reflowed {
    let places = Places::new(value, before, verse);
    // The fields tC3 or tC4 wrote on a word go with the word, wherever the new text has
    // it: on its card, or back in the bank.
    let extras: HashMap<(&str, u32), &JsonMap> = value
        .all_target_words()
        .filter(|w| !w.extra.is_empty())
        .map(|w| ((w.word.as_str(), w.occurrence), &w.extra))
        .collect();
    let with_extra = |word: &TargetWord| TargetWord {
        extra: extras
            .get(&(word.word.as_str(), word.occurrence))
            .map(|e| (*e).clone())
            .unwrap_or_default(),
        ..word.clone()
    };
    let mut kept = vec![false; verse.words().len()];
    let mut dropped = Vec::new();
    let mut alignments = Vec::with_capacity(value.alignments.len());
    for (card, group) in value.alignments.iter().enumerate() {
        let mut targets = Vec::new();
        for word in &group.bottom_words {
            match places.of(word, verse) {
                // tC4 keeps a word placed twice on two cards twice; the second copy
                // would put one word on two cards, so it is reported dropped instead.
                Some(at) if !kept[at] => {
                    kept[at] = true;
                    targets.push(with_extra(&verse.words()[at]));
                }
                _ => dropped.push(DroppedLink {
                    card,
                    top_words: group.top_words.clone(),
                    word: word.clone(),
                }),
            }
        }
        alignments.push(AlignmentGroup {
            top_words: group.top_words.clone(),
            bottom_words: targets,
            extra: group.extra.clone(),
        });
    }
    let word_bank = verse
        .words()
        .iter()
        .zip(&kept)
        .filter(|(_, kept)| !**kept)
        .map(|(w, _)| with_extra(w))
        .collect();
    let mut next = AlignmentValue {
        alignments,
        word_bank,
        ..value.clone()
    };
    next.settle(verse);
    Reflowed {
        value: next,
        dropped,
    }
}

/// Where a word of the old value stands in the new verse. See the module documentation.
enum Places<'a> {
    /// The value was made against `before`: each of its positions, mapped to the new
    /// position where that is clear.
    Moved {
        before: &'a TargetVerse,
        to: Vec<Option<usize>>,
    },
    /// Without the earlier text: by text and occurrence, while the count is unchanged, and
    /// over `changed` text only for a word held once.
    Counted { changed: bool },
}

impl<'a> Places<'a> {
    fn new(value: &AlignmentValue, before: Option<&'a TargetVerse>, verse: &TargetVerse) -> Self {
        match before.filter(|b| made_against(value, b)) {
            Some(before) => Places::Moved {
                before,
                to: follow(before.words(), verse.words()),
            },
            None => Places::Counted {
                changed: !made_against(value, verse),
            },
        }
    }

    fn of(&self, word: &TargetWord, verse: &TargetVerse) -> Option<usize> {
        match self {
            Places::Moved { before, to } => to[before.position(word)?],
            Places::Counted { changed } => {
                let at = verse.position(word)?;
                let count = verse.words()[at].occurrences;
                (count == word.occurrences && (count == 1 || !changed)).then_some(at)
            }
        }
    }
}

/// Whether `value` was made against `verse`: its hash is the verse's, or the md5 of the
/// verse's shown text, which tC4's aligner stamps instead (`stampTargetVerse`, STORAGE.md
/// §7.7 on the two texts).
fn made_against(value: &AlignmentValue, verse: &TargetVerse) -> bool {
    !value.is_stale_against(verse) || value.target_verse_md5 == Md5::of(verse.text().as_bytes())
}

/// For each word of `old`, its position in `new` when every longest common subsequence of
/// the two word lists, by text, matches it to that one position; `None` when every one
/// leaves it out (it was deleted) or they disagree (it is not clear where it went).
fn follow(old: &[TargetWord], new: &[TargetWord]) -> Vec<Option<usize>> {
    let (n, m) = (old.len(), new.len());
    let width = m + 1;
    let same = |i: usize, j: usize| old[i].word == new[j].word;
    // `head[i * width + j]`: the longest common subsequence of `old[..i]` and `new[..j]`;
    // `tail[i * width + j]`: of `old[i..]` and `new[j..]`.
    let mut head = vec![0u32; (n + 1) * width];
    for i in 0..n {
        for j in 0..m {
            head[(i + 1) * width + j + 1] = if same(i, j) {
                head[i * width + j] + 1
            } else {
                head[i * width + j + 1].max(head[(i + 1) * width + j])
            };
        }
    }
    let mut tail = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            tail[i * width + j] = if same(i, j) {
                tail[(i + 1) * width + j + 1] + 1
            } else {
                tail[(i + 1) * width + j].max(tail[i * width + j + 1])
            };
        }
    }
    let longest = head[n * width + m];
    (0..n)
        .map(|i| {
            let left_out =
                (0..=m).any(|j| head[i * width + j] + tail[(i + 1) * width + j] == longest);
            let mut matched = (0..m).filter(|&j| {
                same(i, j) && head[i * width + j] + 1 + tail[(i + 1) * width + j + 1] == longest
            });
            match (matched.next(), matched.next()) {
                (Some(j), None) if !left_out => Some(j),
                _ => None,
            }
        })
        .collect()
}
