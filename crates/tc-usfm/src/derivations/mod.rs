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

mod js;
mod usfmjs;
mod usfmjs_tables;

use js::{JsStr, concat, index_of, js, to_string, trim};
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
    let Ok(shown) = usfmjs::remove_marker(&js(text)) else {
        return true;
    };
    selections
        .iter()
        .all(|s| u64::from(s.occurrences) == count(&shown, &js(s.text)))
}

/// Non-overlapping occurrences of `needle` in `text`, 0 for an empty needle.
fn count(text: &[u16], needle: &[u16]) -> u64 {
    if needle.is_empty() {
        return 0;
    }
    let mut found = 0;
    let mut at = 0;
    while let Some(next) = index_of(text, needle, at) {
        found += 1;
        at = next + needle.len();
    }
    found
}

#[cfg(test)]
mod tests;
