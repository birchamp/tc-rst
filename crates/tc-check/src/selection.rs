//! The checker's selection: the words of their own verse that render a check's quote.
//!
//! The words are the words of the target's shown text (tC4's `verseText` of its content,
//! [`TargetText::shown`]), split with tC4's tokenizer ([`tc_usfm::words::words`]), the one
//! the Check screen renders as tappable words, so an index on the screen and an index here
//! are the same word. A selection records each word's text, which occurrence of that word
//! it is among the verse's words, and how many times its text occurs in the verse (tC4
//! `selections.ts`).
//!
//! The count a selection records is the count `selections_v1` reads
//! ([`TargetText::occurrences`]: the text searched as a substring of the content read
//! through usfm-js's `removeMarker`), as word-aligner-lib's own `generateSelection` records
//! it, so a selection always holds on the text it was made on and on any edit that does not
//! touch its text. tC4's recorder counted words instead, which its validator then read as
//! changed whenever the word is also part of a longer one (`de` in "desde", אֵת in וְאֵת),
//! even on the unchanged verse. The occurrence stays a count of words: highlighting matches
//! on the text and the occurrence, as tC4 does, so the same words light up. Revalidation is
//! the journal's ([`TargetText::fits`]), the one the fold runs.

use std::collections::BTreeSet;

use tc_journal::json::JsonMap;
use tc_usfm::words::{WordToken, words};

use crate::Error;

pub use tc_journal::decision::TargetText;
pub use tc_journal::record::Selection;

/// The verse's words in order: the tappable words of the Check screen. `shown` is the
/// target's shown text ([`TargetText::shown`]).
pub fn target_words(shown: &str) -> Vec<WordToken<'_>> {
    words(shown)
}

/// The selection the checker made by tapping the words at `indices` (into
/// [`target_words`] of the target's shown text), in verse order. An index can repeat; one
/// past the last word is refused.
pub fn from_indices(text: &TargetText, indices: &[usize]) -> Result<Vec<Selection>, Error> {
    let shown = text.shown();
    let tokens = target_words(shown);
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
            // Where usfm-js has no value for the content, any count fits; record what
            // tC4's recorder counts in the shown text.
            occurrences: text.occurrences(tokens[i].text).unwrap_or_else(|| {
                u32::try_from(shown.matches(tokens[i].text).count()).unwrap_or(u32::MAX)
            }),
            extra: JsonMap::new(),
        })
        .collect())
}

/// The word indices a stored selection highlights, matched on text and occurrence (tC4
/// `tokenIndicesFromSelections`). A selection that names no word of the verse marks
/// nothing.
pub fn indices(shown: &str, selections: &[Selection]) -> Vec<usize> {
    target_words(shown)
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

/// A piece of the verse text for display: a run of text, highlighted or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece<'a> {
    pub text: &'a str,
    pub selected: bool,
}

/// The verse cut into pieces with the selected words highlighted. The pieces join to
/// the verse text.
pub fn pieces<'a>(shown: &'a str, selections: &[Selection]) -> Vec<Piece<'a>> {
    let tokens = target_words(shown);
    let lit: BTreeSet<usize> = indices(shown, selections).into_iter().collect();
    let mut out: Vec<Piece<'a>> = Vec::new();
    let mut push = |text: &'a str, selected: bool| {
        if !text.is_empty() {
            out.push(Piece { text, selected });
        }
    };
    let mut at = 0;
    for (i, token) in tokens.iter().enumerate() {
        let end = token.start + token.text.len();
        push(&shown[at..token.start], false);
        push(&shown[token.start..end], lit.contains(&i));
        at = end;
    }
    push(&shown[at..], false);
    out
}

/// The selected words as one line of text, in verse order: words next to each other in
/// the verse keep the text between them, and a gap shows as ` … `. `None` when the
/// selection marks no word of the verse.
pub fn selected_text(shown: &str, selections: &[Selection]) -> Option<String> {
    let tokens = target_words(shown);
    let lit = indices(shown, selections);
    let mut out = String::new();
    let mut previous: Option<usize> = None;
    for i in lit {
        let token = &tokens[i];
        match previous {
            None => {}
            Some(p) if p + 1 == i => {
                let gap = &shown[tokens[p].start + tokens[p].text.len()..token.start];
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

    fn selection(text: &str, occurrence: u32, occurrences: u32) -> Selection {
        Selection {
            text: text.into(),
            occurrence,
            occurrences,
            extra: JsonMap::new(),
        }
    }

    #[test]
    fn tapping_words_records_text_and_occurrence() {
        let text = TargetText::new(VERSE.into());
        let picked = from_indices(&text, &[3, 3, 0]).unwrap();
        assert_eq!(
            picked,
            vec![selection("Pablo", 1, 1), selection("Dios", 1, 2)]
        );
        assert_eq!(indices(VERSE, &picked), vec![0, 3]);
        assert!(from_indices(&text, &[99]).is_err());
    }

    #[test]
    fn a_selection_counts_its_text_as_the_validator_does() {
        // The second "the" of four words; "theory" holds a third "the".
        let verse = TargetText::new("the theory of the law".into());
        let the = from_indices(&verse, &[3]).unwrap();
        assert_eq!(the, [selection("the", 2, 3)]);
        assert_eq!(indices(verse.shown(), &the), [3]);
        assert!(verse.fits(&the));
        assert!(TargetText::new("the theory of the law!".into()).fits(&the));
        assert!(!TargetText::new("the theory of law".into()).fits(&the));
        // Genesis 1:1: אֵת is also the end of וְאֵת.
        let verse = TargetText::new("בְּרֵאשִׁית בָּרָא אֱלֹהִים אֵת הַשָּׁמַיִם וְאֵת הָאָרֶץ".into());
        let et = from_indices(&verse, &[3]).unwrap();
        assert_eq!((et[0].occurrence, et[0].occurrences), (1, 2));
        assert_eq!(indices(verse.shown(), &et), [3]);
        assert!(verse.fits(&et));
    }

    #[test]
    fn display_keeps_neighbours_and_marks_gaps() {
        let text = TargetText::new(VERSE.into());
        let picked = from_indices(&text, &[0, 1, 3]).unwrap();
        assert_eq!(
            selected_text(VERSE, &picked).unwrap(),
            "Pablo, siervo … Dios"
        );
        let joined: String = pieces(VERSE, &picked).iter().map(|p| p.text).collect();
        assert_eq!(joined, VERSE);
        assert_eq!(selected_text(VERSE, &[]), None);
    }
}
