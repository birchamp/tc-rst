//! The versioned derivations (`docs/STORAGE.md` §7.7, §12.2 rule 3). They decide whether an
//! alignment or a check decision still fits the verse it was made on, so their output is
//! part of the project format: the `derivations` of `format.json` name their versions, and
//! a change to any output is a new version.
//!
//! Version 1 of each is tC4's behaviour, byte for byte, so alignments and decisions
//! imported from tC4 and tC3 keep their hashes and their validity:
//!
//! - [`plain_text_v1`] is the text tC4 hashes into `targetVerseMd5` and `verseTextMd5`
//!   (`journal/fold.mjs`): usfm-js 3.4.3 parses `"\v 1 " + content` as a chunk, the `text`
//!   of every top-level verse object is joined, and the result is trimmed. The parser is
//!   ported in `usfmjs` with its quirks: the text of `\w` words inside `\zaln` milestones is
//!   left out, headings that are not displayable are left out, poetry line breaks are kept.
//! - [`tokenize_v1`] is string-punctuation-tokenizer 2.0.0's default `tokenize`: the words
//!   and numbers of [`crate::words::classify`], in order.
//! - [`selections_v1`] is word-aligner-lib's `validateVerseSelections`: the text goes
//!   through usfm-js's `removeMarker`, and a selection still fits when its text occurs in
//!   the result exactly `occurrences` times.
//!
//! The library throws a TypeError on a few malformed nestings (an end marker that closes a
//! phrase whose parent is not a marker, a milestone with no words inside another
//! milestone). There it has no value, so version 1 defines one: [`plain_text_v1`] is then
//! the content trimmed, which changes with every edit, and [`selections_v1`] says the
//! selections fit, as tC4's `decisionIsStale` does when the validator fails.
//!
//! `removeMarker` also runs out of stack a few thousand levels of nesting down, at a depth
//! that depends on the JavaScript engine. Version 1 has no depth limit: the port walks with
//! its own stacks and gives the value the walk has when it finishes, which is the library's
//! at every depth the library finishes. Every derivation takes time linear in the length of
//! the verse and the selections.

mod js;
mod shown;
mod usfmjs;
mod usfmjs_tables;

use js::{JsStr, concat, js, to_string, trim};
use usfmjs::{Key, truthy};

/// The text tC4 hashes for a verse's content (`words + layout` of a verse value): see the
/// module documentation. Total: every string has a value.
pub fn plain_text_v1(content: &str) -> String {
    let parsed = usfmjs::to_json_chunk(&concat(&[&js("\\v 1 "), &js(content)]));
    if parsed.thrown {
        return to_string(trim(&js(content)));
    }
    let heap = &parsed.heap;
    let mut text: JsStr = Vec::new();
    for (_, verse) in &parsed.verses {
        for object in heap.elems(*verse) {
            if let Some(value) = heap.get(*object, &Key::Text).filter(|v| truthy(v)) {
                text.extend(heap.string_of(Some(value)));
            }
        }
    }
    to_string(trim(&text))
}

/// The text tC4's Translate, Check and Align screens and its print preview show for a verse's
/// content, and the text whose words the Check screen selects in and the aligner banks:
/// `verseText` of `src/data/bookModel.js` at tC4 9ad4d5c (#782), the walk of `tokenizeVerse`
/// in `src/data/sourceHighlight.ts` over usfm-js's verse objects of `"\v 1 " + content`,
/// joined and trimmed with JavaScript's `trim`. The walk is in `shown.rs`.
///
/// - Shown: every `\w` word under `\zaln` milestones nested to any depth (Genesis 1:1 "the
///   heavens", a Psalm's "Selah" aligned inside `\qs … \qs*`), the text outside markers,
///   the words of `\add`, `\nd` and other character spans, also after a span nested in them
///   (`\wj Worship the \+nd Lord\+nd* your God\wj*`, which tC4 shows as "Worship the ", a
///   declared difference), a superscription (`\d`) and a `\qs` Selah, with the file's
///   whitespace and line breaks.
/// - Not shown: footnotes, cross references and figures, a section heading's own text, the
///   items of a `\li1` list, and the text usfm-js keeps for a marker it does not know
///   (`\qm1`), as in tC4.
/// - Words never join across a marker: an object left out, or the start or end of a
///   character span or paragraph marker. Where an object left out stands between two characters
///   that are not whitespace, the space or line break usfm-js keeps after its end marker is
///   put back: `siervo\f + \ft o esclavo\f* de` shows "siervo de" (tC4 #795). Where usfm-js
///   keeps none (`siervo\f + \ft o esclavo\f*de`) and the two characters would read as one
///   word or number, a space is put back. tC4's open PR #800 does the first for notes, cross
///   references and figures only; the rebuild does both for every object left out (`\va`,
///   `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*` …), and puts a space back at the start or end of
///   a span or paragraph marker where the two characters would join (`the \add Lord\add*s`,
///   `Yahweh\q2 que`), a declared difference, so the words are the ones the export weaves.
///   Words still join where usfm-js misreads alignment markup (UST 2SA 23:8), as in tC4.
///
/// It is never hashed: hashes are of [`plain_text_v1`]. It is not one of `format.json`'s
/// derivations (`docs/STORAGE.md` §12.1), since no fold output reads it, but it decides
/// which words an alignment holds and which links the reflow of a verse save keeps. It has
/// no depth limit. Where usfm-js throws, or the walk would (a `children` that is neither a
/// list nor a string, a `text` that is not a string, an object inside itself), the value is
/// the content trimmed, as for [`plain_text_v1`].
pub fn shown_text_v1(content: &str) -> String {
    let parsed = usfmjs::to_json_chunk(&concat(&[&js("\\v 1 "), &js(content)]));
    let fallback = || to_string(trim(&js(content)));
    if parsed.thrown {
        return fallback();
    }
    match shown::text(&parsed) {
        Ok(text) => to_string(trim(&text)),
        Err(_) => fallback(),
    }
}

/// The words and numbers of `text`, in order, as tC4 lists the words a checker can select.
pub fn tokenize_v1(text: &str) -> Vec<&str> {
    crate::words::classify(text)
        .into_iter()
        .filter(|t| {
            matches!(
                t.kind,
                crate::words::TokenKind::Word | crate::words::TokenKind::Number
            )
        })
        .map(|t| t.text)
        .collect()
}

/// A selection as `selections_v1` checks it: its text and how many times the verse held
/// that text when it was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionCount<'a> {
    pub text: &'a str,
    pub occurrences: u32,
}

/// True when every selection still fits `text`: see the module documentation.
pub fn selections_v1(text: &str, selections: &[SelectionCount<'_>]) -> bool {
    SelectionText::v1(text).fits(selections)
}

/// [`selections_v1`] in two steps, for checking many decisions against one verse: the
/// verse's text as the validator reads it, made once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionText(Option<JsStr>);

impl SelectionText {
    /// The text `selections_v1` searches; none when the library throws on it.
    pub fn v1(text: &str) -> Self {
        Self(usfmjs::remove_marker(&js(text)).ok())
    }

    /// How many times `selection` occurs in the text, as [`selections_v1`] counts it: the
    /// `occurrences` a selection of it made now records, so that it fits until the text
    /// changes. `None` when the library throws on the text, where any count fits.
    pub fn occurrences(&self, selection: &str) -> Option<u32> {
        let shown = self.0.as_ref()?;
        Some(u32::try_from(count(shown, &js(selection))).unwrap_or(u32::MAX))
    }

    /// [`selections_v1`] of the text this was made from.
    pub fn fits(&self, selections: &[SelectionCount<'_>]) -> bool {
        let Some(shown) = &self.0 else {
            return true;
        };
        selections
            .iter()
            .all(|s| u64::from(s.occurrences) == count(shown, &js(s.text)))
    }
}

/// Non-overlapping occurrences of `needle` in `text`, 0 for an empty needle: what a loop of
/// `indexOf(needle, at)` with `at` set past each match counts. Both come from the project
/// (a selection is part of a decision), so the search is Knuth-Morris-Pratt, linear in
/// both lengths; a naive search of a long selection in a long verse takes their product.
fn count(text: &[u16], needle: &[u16]) -> u64 {
    if needle.is_empty() {
        return 0;
    }
    // fail[i]: the length of the longest proper border of needle[..=i].
    let mut fail = vec![0usize; needle.len()];
    let mut k = 0;
    for i in 1..needle.len() {
        while k > 0 && needle[i] != needle[k] {
            k = fail[k - 1];
        }
        if needle[i] == needle[k] {
            k += 1;
        }
        fail[i] = k;
    }
    let mut found = 0;
    let mut matched = 0;
    for unit in text {
        while matched > 0 && *unit != needle[matched] {
            matched = fail[matched - 1];
        }
        if *unit == needle[matched] {
            matched += 1;
        }
        if matched == needle.len() {
            found += 1;
            // The next occurrence starts after this one.
            matched = 0;
        }
    }
    found
}

#[cfg(test)]
mod tests;
