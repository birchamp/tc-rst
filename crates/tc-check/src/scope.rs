//! Whether a check falls inside a book's project scope (tC4 `refInScope`, D26).
//!
//! tC4 asks `bible-reference-range` 1.1.1 `doesReferenceContain(range, "c:v")` for each
//! scope range. The ranges are the five typed forms of a burrito scope, but the check's
//! reference can be any text a resource row carries (`front:intro`, `1:3,5`, `2:1a`), and
//! the library reads such text with JavaScript's loose rules. This module ports the
//! library's parse and its non-strict containment tests over JavaScript values, so every
//! input gets tC4's answer. `fixtures/check/check-vectors.json` holds the library's own
//! answers.
//!
//! One of those answers is a quirk tC4 users see: the library reads the end of a
//! `C:V-C:V` range only in its last chapter, so a range within one chapter, such as
//! `2:5-2:9`, holds the rest of chapter 2. `2:5-9` holds verses 5 to 9 only.

use tc_burrito::ScopeRange;

use crate::js::{JsVal, parse_int};
use crate::key::RefPart;

/// True when the check at `chapter:verse` is inside the scope. An empty list is the
/// whole book.
pub fn in_scope(ranges: &[ScopeRange], chapter: &RefPart, verse: &RefPart) -> bool {
    in_scope_text(ranges, &format!("{chapter}:{verse}"))
}

/// [`in_scope`] for a mapping's candidate verse, given as numbers.
pub(crate) fn in_scope_numbers(ranges: &[ScopeRange], chapter: u16, verse: u16) -> bool {
    in_scope_text(ranges, &format!("{chapter}:{verse}"))
}

fn in_scope_text(ranges: &[ScopeRange], reference: &str) -> bool {
    if ranges.is_empty() {
        return true;
    }
    let search = parse_reference(reference);
    ranges.iter().any(|range| {
        let chunk = range_chunk(range);
        search
            .iter()
            .any(|s| contains_chunk(std::slice::from_ref(&chunk), s))
    })
}

/// A parsed piece of a reference (the library's `verseChunk`).
#[derive(Debug, Clone, PartialEq)]
struct Chunk {
    chapter: JsVal,
    verse: JsVal,
    end_chapter: JsVal,
    end_verse: JsVal,
}

impl Chunk {
    fn new(chapter: JsVal) -> Self {
        Self {
            chapter,
            verse: JsVal::Undefined,
            end_chapter: JsVal::Undefined,
            end_verse: JsVal::Undefined,
        }
    }
}

fn num(n: std::num::NonZeroU32) -> JsVal {
    JsVal::Num(f64::from(n.get()))
}

/// What the library parses a scope range into. The forms are typed, so each is one
/// chunk with numbers only.
fn range_chunk(range: &ScopeRange) -> Chunk {
    match *range {
        ScopeRange::Chapter(c) => Chunk::new(num(c)),
        ScopeRange::Chapters(a, b) => Chunk {
            end_chapter: num(b),
            ..Chunk::new(num(a))
        },
        ScopeRange::Verse(c, v) => Chunk {
            verse: num(v),
            ..Chunk::new(num(c))
        },
        ScopeRange::Verses(c, a, b) => Chunk {
            verse: num(a),
            end_verse: num(b),
            ..Chunk::new(num(c))
        },
        ScopeRange::Span(c1, v1, c2, v2) => Chunk {
            verse: num(v1),
            end_chapter: num(c2),
            end_verse: num(v2),
            ..Chunk::new(num(c1))
        },
    }
}

const RANGE_SEPARATORS: [char; 7] = [
    '-', '\u{AD}', '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}',
];

/// `getRangeSeparator`: the position of the first separator of the list that occurs,
/// in list order (not the leftmost one), and its length.
fn range_separator(text: &str) -> Option<(usize, usize)> {
    RANGE_SEPARATORS
        .iter()
        .find_map(|sep| text.find(*sep).map(|at| (at, sep.len_utf8())))
}

/// `toIntIfValid` for text.
fn int_if_valid(text: &str) -> JsVal {
    if range_separator(text).is_some() || text.contains("ff") {
        return JsVal::Str(text.to_owned());
    }
    let n = parse_int(text);
    if n.is_nan() {
        JsVal::Str(text.to_owned())
    } else {
        JsVal::Num(n)
    }
}

/// `getChapterVerse` for text: `(chapter, verse, found)`.
fn chapter_verse(text: &str) -> (JsVal, JsVal, bool) {
    match text.find(':') {
        Some(at) => (
            int_if_valid(&text[..at]),
            int_if_valid(&text[at + 1..]),
            true,
        ),
        None => (JsVal::Undefined, int_if_valid(text), false),
    }
}

/// `getRange`: `(verse, end_chapter, end_verse)`.
fn range_of(verse: &JsVal) -> (JsVal, JsVal, JsVal) {
    let JsVal::Str(text) = verse else {
        return (verse.clone(), JsVal::Undefined, JsVal::Undefined);
    };
    if let Some((at, len)) = range_separator(text) {
        let start = int_if_valid(&text[..at]);
        let end = &text[at + len..];
        let (chapter, end_verse, found) = chapter_verse(end);
        return if found {
            (start, chapter, end_verse)
        } else {
            (start, JsVal::Undefined, int_if_valid(end))
        };
    }
    if text.to_lowercase().contains("ff") {
        // `substring(0, indexOf('ff'))`: an upper-case `FF` is not found, and the
        // negative index reads as 0.
        let start = text.find("ff").map_or("", |at| &text[..at]);
        return (
            int_if_valid(start),
            JsVal::Undefined,
            JsVal::Str("ff".into()),
        );
    }
    (verse.clone(), JsVal::Undefined, JsVal::Undefined)
}

/// `parseReferenceToList`.
fn parse_reference(reference: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for part in reference.split(';') {
        if part.is_empty() {
            continue;
        }
        if !part.contains(':') {
            chunks.push(chapter_chunk(part));
            continue;
        }
        let mut pieces = part.split(',');
        let first = pieces.next().unwrap_or_default();
        let (mut chapter, mut verse, found) = chapter_verse(first);
        if !found {
            chapter = verse;
            verse = JsVal::Undefined;
        }
        // The library carries `lastChapter` across `;` parts, but every part with a `:`
        // sets it before reading it, so it can start afresh here.
        let mut last_chapter = chapter.clone();
        let (v, end_chapter, end_verse) = range_of(&verse);
        if end_chapter.truthy() {
            last_chapter = end_chapter.clone();
        }
        chunks.push(Chunk {
            chapter: chapter.clone(),
            verse: v,
            end_chapter,
            end_verse,
        });
        for piece in pieces {
            if piece.is_empty() {
                continue;
            }
            let (c, v, found) = chapter_verse(piece);
            if found {
                chapter = c;
                last_chapter = chapter.clone();
            } else {
                chapter = last_chapter.clone();
            }
            let (v, end_chapter, end_verse) = range_of(&v);
            if end_verse.truthy() {
                if end_chapter.truthy() {
                    last_chapter = end_chapter.clone();
                }
                chunks.push(Chunk {
                    chapter: chapter.clone(),
                    verse: v,
                    end_chapter,
                    end_verse,
                });
            } else {
                chunks.push(Chunk {
                    verse: v,
                    ..Chunk::new(chapter.clone())
                });
            }
        }
    }
    chunks
}

/// `addChapterReference`.
fn chapter_chunk(text: &str) -> Chunk {
    match range_separator(text) {
        Some((at, len)) => Chunk {
            end_chapter: int_if_valid(&text[at + len..]),
            ..Chunk::new(int_if_valid(&text[..at]))
        },
        None => Chunk::new(int_if_valid(text)),
    }
}

/// `doChunksContainChunk`, non-strict.
fn contains_chunk(ranges: &[Chunk], search: &Chunk) -> bool {
    ranges.iter().any(|range| {
        if search.end_verse.truthy() {
            contains_range(range, search)
        } else {
            contains_verse(range, &search.chapter, &search.verse)
        }
    })
}

fn is_ff(v: &JsVal) -> bool {
    v.strict_eq(&JsVal::Str("ff".into()))
}

/// `chunkContainsVerse`.
fn contains_verse(range: &Chunk, chapter: &JsVal, verse: &JsVal) -> bool {
    if !range.end_chapter.truthy() {
        if !chapter.strict_eq(&range.chapter) {
            return false;
        }
        if !range.verse.truthy() {
            return true;
        }
        if !range.end_verse.truthy() {
            return verse.strict_eq(&range.verse);
        }
        if is_ff(&range.end_verse) {
            return range.verse.le(verse);
        }
        return range.verse.le(verse) && verse.le(&range.end_verse);
    }
    if !(range.chapter.le(chapter) && chapter.le(&range.end_chapter)) {
        return false;
    }
    if !range.verse.truthy() {
        return true;
    }
    if chapter.strict_eq(&range.chapter) {
        verse.ge(&range.verse)
    } else if chapter.strict_eq(&range.end_chapter) {
        verse.le(&range.end_verse)
    } else {
        true
    }
}

/// `chunkContainsVerseRange`, non-strict.
fn contains_range(range: &Chunk, search: &Chunk) -> bool {
    if !range.end_chapter.truthy() {
        return if search.end_chapter.truthy() {
            chapter_range_in_chapter(range, search)
        } else {
            verse_range_in_chapter(range, search)
        };
    }
    if search.end_chapter.truthy() {
        return chapter_range_in_chapter_range(range, search);
    }
    if range.chapter.le(&search.chapter) && search.chapter.le(&range.end_chapter) {
        return verse_range_in_chapter_range(range, search);
    }
    false
}

/// `chapterRangeContainedInChapter`, non-strict.
fn chapter_range_in_chapter(chapter: &Chunk, search: &Chunk) -> bool {
    if search.end_chapter.lt(&chapter.chapter) || search.chapter.gt(&chapter.chapter) {
        return false;
    }
    if search.end_chapter.strict_eq(&chapter.chapter) {
        if !chapter.verse.truthy() {
            return true;
        }
        if !chapter.end_verse.truthy() && search.end_verse.lt(&chapter.verse) {
            return false;
        }
        if is_ff(&chapter.end_verse) {
            return search.end_verse.ge(&chapter.verse);
        }
        return !search.end_verse.lt(&chapter.verse);
    }
    if search.chapter.strict_eq(&chapter.chapter) {
        if !chapter.verse.truthy() {
            return true;
        }
        if !chapter.end_verse.truthy() {
            return !search.verse.gt(&chapter.verse);
        }
        if is_ff(&chapter.end_verse) {
            return true;
        }
        return !search.verse.gt(&chapter.end_verse);
    }
    true
}

/// `chapterVerseRangeContainedInChapter`, non-strict.
fn verse_range_in_chapter(chapter: &Chunk, search: &Chunk) -> bool {
    if !search.chapter.strict_eq(&chapter.chapter) {
        return false;
    }
    if !chapter.verse.truthy() {
        return true;
    }
    if !chapter.end_verse.truthy() {
        return !(search.end_verse.lt(&chapter.verse) || search.verse.gt(&chapter.verse));
    }
    if is_ff(&chapter.end_verse) {
        return search.end_verse.ge(&chapter.verse);
    }
    !(search.end_verse.lt(&chapter.verse) || search.verse.gt(&chapter.end_verse))
}

/// `chapterRangeContainedInChapterRange`, non-strict.
fn chapter_range_in_chapter_range(range: &Chunk, search: &Chunk) -> bool {
    if search.end_chapter.lt(&range.chapter) || search.chapter.gt(&range.end_chapter) {
        return false;
    }
    if search.end_chapter.strict_eq(&range.chapter) {
        if !range.verse.truthy() {
            return true;
        }
        if search.end_verse.lt(&range.verse) {
            return false;
        }
    }
    if search.chapter.strict_eq(&range.end_chapter) {
        if !range.verse.truthy() {
            return true;
        }
        if search.verse.gt(&range.end_verse) {
            return false;
        }
    }
    true
}

/// `chapterVerseRangeContainedInChapterRange`, non-strict.
fn verse_range_in_chapter_range(range: &Chunk, search: &Chunk) -> bool {
    if search.chapter.gt(&range.end_chapter) || search.chapter.lt(&range.chapter) {
        return false;
    }
    if search.chapter.strict_eq(&range.end_chapter) {
        if !range.verse.truthy() {
            return true;
        }
        if search.verse.gt(&range.end_verse) {
            return false;
        }
    }
    if search.chapter.strict_eq(&range.chapter) {
        if !range.verse.truthy() {
            return true;
        }
        if search.end_verse.lt(&range.verse) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(text: &[&str]) -> Vec<ScopeRange> {
        text.iter().map(|r| r.parse().unwrap()).collect()
    }

    fn part(text: &str) -> RefPart {
        RefPart::parse(text).unwrap()
    }

    #[test]
    fn harness_vectors_of_tc4_s0c() {
        let at = |r: &[&str], c: &str, v: &str| in_scope(&ranges(r), &part(c), &part(v));
        assert!(at(&["1:1-2:5"], "2", "5"));
        assert!(!at(&["1:1-2:5"], "2", "6"));
        assert!(at(&["3"], "3", "15"));
        assert!(!at(&["3"], "2", "1"));
        assert!(at(&[], "front", "intro"));
        assert!(!at(&["1"], "front", "intro"));
    }

    #[test]
    fn spans_and_lists_are_read_as_the_library_reads_them() {
        let at = |r: &[&str], c: &str, v: &str| in_scope(&ranges(r), &part(c), &part(v));
        assert!(at(&["2:9"], "2", "9-10"));
        assert!(at(&["2:10-12"], "2", "9-10"));
        assert!(!at(&["2:11-12"], "2", "9-10"));
        assert!(at(&["1:5"], "1", "3,5"));
        assert!(at(&["1:1"], "1", "1a"));
    }
}
