//! The checker's selection: the words of their own verse that render a check's quote.
//!
//! The words are split with tC4's tokenizer ([`tc_usfm::words::words`]), the one the
//! Check screen renders as tappable words, so an index on the screen and an index here
//! are the same word. A selection records each word's text, which occurrence of that
//! word it is among the verse's words, and how many times its text occurs in the verse
//! (tC4 `selections.ts`).
//!
//! [`validate`] is the revalidation tC4 runs after a verse edit, `word-aligner-lib`
//! 1.0.1 `validateVerseSelections` (STORAGE.md `selections_v1`). It counts a selection's
//! text as a substring of the verse, not as a word, exactly as the library does: `the`
//! occurs twice in "the theory". The count a selection records is that same substring
//! count, as the library's own `generateSelection` records it, so a selection always
//! holds on the text it was made on and on any edit that does not touch its text. tC4's
//! recorder counted words instead, which its validator then read as changed whenever
//! the word is also part of a longer one (`de` in "desde", אֵת in וְאֵת), even on the
//! unchanged verse. The occurrence stays a count of words: highlighting matches on the
//! text and the occurrence, as tC4 does, so the same words light up.

use std::collections::BTreeSet;

use tc_usfm::words::{WordToken, words};

use crate::Error;

/// One selected word (tC4 §5.2 `selections` entry).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Selection {
    pub text: String,
    /// Which of the verse's words with this text, from 1.
    pub occurrence: u32,
    /// How many times `text` occurs in the verse, counted as [`validate`] counts it.
    pub occurrences: u32,
}

/// The verse's words in order: the tappable words of the Check screen.
pub fn target_words(verse_text: &str) -> Vec<WordToken<'_>> {
    words(verse_text)
}

/// The selection the checker made by tapping the words at `indices` (into
/// [`target_words`]), in verse order. An index can repeat; one past the last word is
/// refused.
pub fn from_indices(verse_text: &str, indices: &[usize]) -> Result<Vec<Selection>, Error> {
    let tokens = target_words(verse_text);
    let chosen: BTreeSet<usize> = indices.iter().copied().collect();
    if let Some(&index) = chosen.iter().find(|&&i| i >= tokens.len()) {
        return Err(Error::WordIndex {
            index,
            count: tokens.len(),
        });
    }
    Ok(chosen
        .into_iter()
        .map(|i| Selection {
            text: tokens[i].text.to_owned(),
            occurrence: tokens[i].occurrence,
            occurrences: occurrences_in(verse_text, tokens[i].text),
        })
        .collect())
}

/// The word indices a stored selection highlights, matched on text and occurrence (tC4
/// `tokenIndicesFromSelections`). A selection that names no word of the verse marks
/// nothing.
pub fn indices(verse_text: &str, selections: &[Selection]) -> Vec<usize> {
    target_words(verse_text)
        .iter()
        .enumerate()
        .filter(|(_, w)| {
            selections
                .iter()
                .any(|s| s.text == w.text && s.occurrence == w.occurrence)
        })
        .map(|(i, _)| i)
        .collect()
}

/// The result of revalidating a selection against the verse text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revalidation {
    /// The selections the verse still supports, in their stored order.
    pub valid: Vec<Selection>,
    /// True when any selection is no longer supported: the decision needs a new look.
    pub changed: bool,
}

/// `selections_v1`: keeps a selection while its text occurs in the verse exactly as
/// often as it did when selected (`word-aligner-lib` `validateVerseSelections` on plain
/// text). `verse_text` is the verse's plain text; usfm-js drops its leading whitespace
/// before counting, which changes no count but is done here too.
pub fn validate(verse_text: &str, selections: &[Selection]) -> Revalidation {
    let valid: Vec<Selection> = selections
        .iter()
        .filter(|s| occurrences_in(verse_text, &s.text) == s.occurrences)
        .cloned()
        .collect();
    Revalidation {
        changed: valid.len() != selections.len(),
        valid,
    }
}

/// How many times `text` occurs in the verse as `selections_v1` counts it:
/// non-overlapping substrings, left to right, none for empty text.
fn occurrences_in(verse_text: &str, text: &str) -> u32 {
    let verse = verse_text.trim_start_matches(crate::js::is_whitespace);
    let count = if text.is_empty() {
        0
    } else {
        verse.matches(text).count()
    };
    // No verse holds 2^32 matches; saturate rather than wrap if one ever did.
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// A piece of the verse text for display: a run of text, highlighted or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece<'a> {
    pub text: &'a str,
    pub selected: bool,
}

/// The verse cut into pieces with the selected words highlighted. The pieces join to
/// the verse text.
pub fn pieces<'a>(verse_text: &'a str, selections: &[Selection]) -> Vec<Piece<'a>> {
    let tokens = target_words(verse_text);
    let lit: BTreeSet<usize> = indices(verse_text, selections).into_iter().collect();
    let mut out: Vec<Piece<'a>> = Vec::new();
    let mut push = |text: &'a str, selected: bool| {
        if !text.is_empty() {
            out.push(Piece { text, selected });
        }
    };
    let mut at = 0;
    for (i, token) in tokens.iter().enumerate() {
        let end = token.start + token.text.len();
        push(&verse_text[at..token.start], false);
        push(&verse_text[token.start..end], lit.contains(&i));
        at = end;
    }
    push(&verse_text[at..], false);
    out
}

/// The selected words as one line of text, in verse order: words next to each other in
/// the verse keep the text between them, and a gap shows as ` … `. `None` when the
/// selection marks no word of the verse.
pub fn selected_text(verse_text: &str, selections: &[Selection]) -> Option<String> {
    let tokens = target_words(verse_text);
    let lit = indices(verse_text, selections);
    let mut out = String::new();
    let mut previous: Option<usize> = None;
    for i in lit {
        let token = &tokens[i];
        match previous {
            None => {}
            Some(p) if p + 1 == i => {
                let gap = &verse_text[tokens[p].start + tokens[p].text.len()..token.start];
                out.push_str(gap);
            }
            Some(_) => out.push_str(" … "),
        }
        out.push_str(token.text);
        previous = Some(i);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSE: &str =
        "Pablo, siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos de Dios";

    #[test]
    fn tapping_words_records_text_and_occurrence() {
        let picked = from_indices(VERSE, &[3, 3, 0]).unwrap();
        assert_eq!(
            picked,
            vec![
                Selection {
                    text: "Pablo".into(),
                    occurrence: 1,
                    occurrences: 1
                },
                Selection {
                    text: "Dios".into(),
                    occurrence: 1,
                    occurrences: 2
                },
            ]
        );
        assert_eq!(indices(VERSE, &picked), vec![0, 3]);
        assert!(from_indices(VERSE, &[99]).is_err());
    }

    #[test]
    fn revalidation_counts_substrings_as_the_library_does() {
        let dios = from_indices(VERSE, &[3]).unwrap();
        assert!(!validate(VERSE, &dios).changed);
        assert!(validate(&VERSE.replace("de Dios y", "del Señor y"), &dios).changed);
        let the = [Selection {
            text: "the".into(),
            occurrence: 1,
            occurrences: 1,
        }];
        assert!(validate("the theory", &the).changed);
        assert!(!validate("the word", &the).changed);
    }

    #[test]
    fn a_selection_counts_its_text_as_the_validator_does() {
        // The second "the" of four words; "theory" holds a third "the".
        let verse = "the theory of the law";
        let the = from_indices(verse, &[3]).unwrap();
        assert_eq!(
            the,
            [Selection {
                text: "the".into(),
                occurrence: 2,
                occurrences: 3
            }]
        );
        assert_eq!(indices(verse, &the), [3]);
        assert!(!validate(verse, &the).changed);
        assert!(!validate("the theory of the law!", &the).changed);
        assert!(validate("the theory of law", &the).changed);
        // Genesis 1:1: אֵת is also the end of וְאֵת.
        let verse = "בְּרֵאשִׁית בָּרָא אֱלֹהִים אֵת הַשָּׁמַיִם וְאֵת הָאָרֶץ";
        let et = from_indices(verse, &[3]).unwrap();
        assert_eq!((et[0].occurrence, et[0].occurrences), (1, 2));
        assert_eq!(indices(verse, &et), [3]);
        assert!(!validate(verse, &et).changed);
    }

    #[test]
    fn display_keeps_neighbours_and_marks_gaps() {
        let picked = from_indices(VERSE, &[0, 1, 3]).unwrap();
        assert_eq!(
            selected_text(VERSE, &picked).unwrap(),
            "Pablo, siervo … Dios"
        );
        let joined: String = pieces(VERSE, &picked).iter().map(|p| p.text).collect();
        assert_eq!(joined, VERSE);
        assert_eq!(selected_text(VERSE, &[]), None);
    }
}
