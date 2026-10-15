//! Word tokens, split exactly as tC4 splits them.
//!
//! tC4 decides word identity (alignment, check selections, suggestions) with
//! `string-punctuation-tokenizer` 2.0.0 on `xregexp` 4.4.1 (`src/data/align/tokenize.ts`).
//! This module reproduces that tokenizer, quirks included, because a word's identity is
//! its text plus its occurrence number, and both must match what tC4 recorded:
//!
//! - A word is a run of letters (`\p{L}`), marks (`\p{M}`), U+200D ZERO WIDTH JOINER and
//!   U+2060 WORD JOINER. So Hebrew `וַֽ⁠יְהִי֙` with its word joiner is one word, while
//!   U+200C ZERO WIDTH NON-JOINER splits a word in two.
//! - A number is a run of `\p{N}` and, through a pattern typo in the library
//!   (`[\pN\pNd\pNl\pNo]` reads as `\pN` plus the letters `d`, `l` and `o`), those three
//!   ASCII letters: `abc123def` is the word `abc`, the number `123d` and the word `ef`.
//! - Punctuation is one `\p{P}` character, matched only right where the previous token
//!   ended, or the pairs `<<`, `>>`, `<>`, `><` anywhere.
//! - Whitespace is JavaScript's `\s`.
//! - Everything else (symbols such as `$` or `+`, other format characters, and every
//!   character outside the Basic Multilingual Plane, which xregexp 4 does not classify)
//!   is [`TokenKind::Other`]. A punctuation character that follows such a character
//!   becomes part of the same `Other` token: `$,` is one token.
//!
//! The character classes come from xregexp 4.4.1 itself (`tools/gen-unicode-tables.cjs`).

mod tables;

use std::collections::HashMap;

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Word,
    Number,
    Punctuation,
    Whitespace,
    /// Characters tC4's tokenizer drops: symbols, some format characters, astral
    /// characters, and punctuation that directly follows them.
    Other,
}

/// A token and where it starts in the text, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
    pub start: usize,
}

/// A word or number with its occurrence: the 1-based position among tokens with the same
/// text in the verse, and the number of such tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordToken<'a> {
    pub text: &'a str,
    pub start: usize,
    pub occurrence: u32,
    pub occurrences: u32,
}

/// Split `text` into tokens. The tokens cover the text completely and in order.
pub fn classify(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let Some((start, kind)) = next_match(text, at) else {
            tokens.push(Token {
                kind: TokenKind::Other,
                text: &text[at..],
                start: at,
            });
            break;
        };
        if start > at {
            tokens.push(Token {
                kind: TokenKind::Other,
                text: &text[at..start],
                start: at,
            });
        }
        let end = match_end(text, start, kind);
        tokens.push(Token {
            kind,
            text: &text[start..end],
            start,
        });
        at = end;
    }
    tokens
}

/// The words and numbers of `text`, numbered by occurrence.
pub fn words(text: &str) -> Vec<WordToken<'_>> {
    let found: Vec<Token<'_>> = classify(text)
        .into_iter()
        .filter(|t| matches!(t.kind, TokenKind::Word | TokenKind::Number))
        .collect();
    number_occurrences(found.iter().map(|t| (t.text, t.start)))
}

/// Number words by occurrence. `words` yields each word's text and start offset in
/// document order; the offsets are carried through untouched.
pub fn number_occurrences<'a>(
    words: impl IntoIterator<Item = (&'a str, usize)>,
) -> Vec<WordToken<'a>> {
    let words: Vec<(&'a str, usize)> = words.into_iter().collect();
    let mut totals: HashMap<&str, u32> = HashMap::new();
    for (text, _) in &words {
        *totals.entry(text).or_default() += 1;
    }
    let mut seen: HashMap<&str, u32> = HashMap::new();
    words
        .into_iter()
        .map(|(text, start)| {
            let occurrence = seen.entry(text).or_default();
            *occurrence += 1;
            WordToken {
                text,
                start,
                occurrence: *occurrence,
                occurrences: totals.get(text).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// The earliest position at or after `at` where one of tC4's four token patterns
/// matches, and which pattern wins there (word, then whitespace, then punctuation, then
/// number, the library's order).
fn next_match(text: &str, at: usize) -> Option<(usize, TokenKind)> {
    let mut chars = text[at..].char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        let start = at + offset;
        if is_word(c) {
            return Some((start, TokenKind::Word));
        }
        if is_whitespace(c) {
            return Some((start, TokenKind::Whitespace));
        }
        if (offset == 0 && is_punctuation(c))
            || (is_angle(c) && chars.peek().is_some_and(|&(_, n)| is_angle(n)))
        {
            return Some((start, TokenKind::Punctuation));
        }
        if is_number(c) {
            return Some((start, TokenKind::Number));
        }
    }
    None
}

fn match_end(text: &str, start: usize, kind: TokenKind) -> usize {
    let run = |class: fn(char) -> bool| {
        text[start..]
            .char_indices()
            .find(|&(_, c)| !class(c))
            .map_or(text.len(), |(offset, _)| start + offset)
    };
    match kind {
        TokenKind::Word => run(is_word),
        TokenKind::Whitespace => run(is_whitespace),
        TokenKind::Number => run(is_number),
        TokenKind::Punctuation => {
            // `<` and `>` are not `\p{P}`, so punctuation that starts with one is a pair of
            // one-byte characters.
            if text[start..].starts_with(is_angle) {
                start + 2
            } else {
                start + text[start..].chars().next().map_or(0, char::len_utf8)
            }
        }
        TokenKind::Other => text.len(),
    }
}

fn in_table(table: &[(u16, u16)], c: char) -> bool {
    let Ok(code) = u16::try_from(u32::from(c)) else {
        return false;
    };
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < code {
                std::cmp::Ordering::Less
            } else if lo > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn is_word(c: char) -> bool {
    in_table(tables::WORD, c)
}

fn is_number(c: char) -> bool {
    in_table(tables::NUMBER, c)
}

fn is_punctuation(c: char) -> bool {
    in_table(tables::PUNCTUATION, c)
}

fn is_whitespace(c: char) -> bool {
    in_table(tables::WHITESPACE, c)
}

fn is_angle(c: char) -> bool {
    c == '<' || c == '>'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(TokenKind, &str)> {
        classify(text)
            .into_iter()
            .map(|t| (t.kind, t.text))
            .collect()
    }

    #[test]
    fn quirks_of_the_reference_tokenizer_are_kept() {
        use TokenKind::*;
        assert_eq!(
            pairs("abc123def"),
            vec![(Word, "abc"), (Number, "123d"), (Word, "ef")]
        );
        assert_eq!(
            pairs("$, x<<"),
            vec![
                (Other, "$,"),
                (Whitespace, " "),
                (Word, "x"),
                (Punctuation, "<<")
            ]
        );
        assert_eq!(pairs("<<<"), vec![(Punctuation, "<<"), (Other, "<")]);
        assert_eq!(
            pairs("a\u{200C}b"),
            vec![(Word, "a"), (Other, "\u{200C}"), (Word, "b")]
        );
        assert_eq!(pairs("𝐀b"), vec![(Other, "𝐀"), (Word, "b")]);
    }

    #[test]
    fn occurrences_count_identical_words() {
        let found = words("of God, of the truth of");
        let of: Vec<(u32, u32)> = found
            .iter()
            .filter(|w| w.text == "of")
            .map(|w| (w.occurrence, w.occurrences))
            .collect();
        assert_eq!(of, vec![(1, 3), (2, 3), (3, 3)]);
        assert_eq!(found[1].text, "God");
        assert_eq!(found[1].start, 3);
    }
}
