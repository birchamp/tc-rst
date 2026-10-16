//! Which lexicon entry a word's `strong` attribute names: tC4's rule, ported exactly.
//!
//! tC4 4.0 adds no lexicon reader of its own (tC4 `docs/PACKAGING.md:996-997`, issue
//! #703). The rule its word details use is `lexiconHelpers` in word-aligner-lib 1.0.1, a
//! dependency tC4 pins (tC4 `package.json:39`; `src/helpers/lexiconHelpers.js:61-144` in
//! that package's source map, minified at `dist/index.es.js:1241-1263`). The same file
//! drives the word popover of the aligner tC4 tests against
//! (`@gabrielaillet/word-aligner-rcl` 1.3.18, `PrimaryToken.jsx:141`,
//! `WordLexiconDetails.jsx:162-182`).
//!
//! - A `strong` attribute is split at every `:` into parts (`getStrongsParts`), so
//!   `c:d:H0776` has the parts `c`, `d` and `H0776`.
//! - A part that starts with `G` names the Greek lexicon `ugl`; any other part names the
//!   Hebrew lexicon `uhl` (`lexiconIdFromStrongs`).
//! - The entry number is the part with its first ASCII word character removed, then, unless
//!   the part starts with `H`, its last character removed too (UGNT numbers carry a final
//!   variant digit: `G35880` is entry 3588), read by JavaScript's `parseInt`
//!   (`lexiconEntryIdFromStrongs`). A result of 0 or `NaN` names no entry, so a prefix part
//!   such as `c` is skipped.
//! - Each part with a number is looked up in its lexicon (`lookupStrongsNumbers`).
//!
//! `fixtures/lexicon/strongs-vectors.json` holds what tC4's code computes for edge cases
//! and for every `strong` attribute of the bundled UHB and UGNT; the tests compare.

use tc_core::Testament;

/// tC4's two lexicon ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LexiconId {
    /// `ugl`, the Greek lexicon: the project's `lexicon.nt` pin (tC4
    /// `installedSuite.js:84`, `uW/en_ugl`).
    Ugl,
    /// `uhl`, the Hebrew lexicon: the project's `lexicon.ot` pin (tC4
    /// `installedSuite.js:85`, `uW/en_uhl`).
    Uhl,
}

impl LexiconId {
    /// tC4's name for it.
    pub fn as_str(self) -> &'static str {
        match self {
            LexiconId::Ugl => "ugl",
            LexiconId::Uhl => "uhl",
        }
    }

    /// The language its entries describe, as its metadata names the lexicon.
    pub fn language(self) -> &'static str {
        match self {
            LexiconId::Ugl => "Greek",
            LexiconId::Uhl => "Hebrew",
        }
    }

    /// The testament whose `lexicon` pin holds this lexicon.
    pub fn testament(self) -> Testament {
        match self {
            LexiconId::Ugl => Testament::New,
            LexiconId::Uhl => Testament::Old,
        }
    }
}

/// The number tC4 reads from a part, when it is not 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrongsNumber {
    /// A number an entry can have.
    Entry(u32),
    /// A negative number, or one above `u32::MAX`. tC4 looks it up and finds nothing,
    /// because no entry has it; its value is not kept.
    OutOfRange,
}

/// One `:`-separated part of a `strong` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrongsPart<'s> {
    pub text: &'s str,
    pub lexicon: LexiconId,
    /// `None` where tC4 reads 0 or `NaN`: the part names no entry.
    pub number: Option<StrongsNumber>,
}

/// Every part of `strong`, in order (`getStrongsParts`). An empty attribute has none.
pub fn strongs_parts(strong: &str) -> Vec<StrongsPart<'_>> {
    if strong.is_empty() {
        return Vec::new();
    }
    strong
        .split(':')
        .map(|text| StrongsPart {
            text,
            lexicon: lexicon_of(text),
            number: number_of(text),
        })
        .collect()
}

/// Whether any part names an entry (`containsValidStrongsNumber`).
pub fn contains_valid_strongs_number(strong: &str) -> bool {
    strongs_parts(strong).iter().any(|p| p.number.is_some())
}

/// `lexiconIdFromStrongs`.
fn lexicon_of(part: &str) -> LexiconId {
    if part.starts_with('G') {
        LexiconId::Ugl
    } else {
        LexiconId::Uhl
    }
}

/// `lexiconEntryIdFromStrongs` on one part.
fn number_of(part: &str) -> Option<StrongsNumber> {
    // `replace(/\w/, '')`: without the `u` flag, `\w` is `[A-Za-z0-9_]`.
    let mut code = String::from(part);
    if let Some(at) = code
        .bytes()
        .position(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        code.remove(at);
    }
    // `slice(0, -1)` drops the last UTF-16 unit. Dropping the whole last character gives
    // the same number: the low half of a surrogate pair is never a digit, and the high half
    // left behind stops `parseInt` as the end of the string does.
    if !part.starts_with('H') {
        code.pop();
    }
    parse_int(&code)
}

/// JavaScript's `parseInt(s)` with no radix, then `|| 0`.
fn parse_int(s: &str) -> Option<StrongsNumber> {
    let s = s.trim_start_matches(is_js_whitespace);
    let (negative, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (radix, s) = match s.get(..2) {
        Some("0x" | "0X") => (16, &s[2..]),
        _ => (10, s),
    };
    let digits: Vec<u32> = s.chars().map_while(|c| c.to_digit(radix)).collect();
    if digits.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for digit in digits {
        value = match value
            .checked_mul(u64::from(radix))
            .and_then(|v| v.checked_add(u64::from(digit)))
        {
            Some(v) => v,
            // Larger than any entry, and still not 0.
            None => return Some(StrongsNumber::OutOfRange),
        };
    }
    match (value, negative) {
        // `-0` and `0` are both falsy.
        (0, _) => None,
        (_, true) => Some(StrongsNumber::OutOfRange),
        (v, false) => {
            Some(u32::try_from(v).map_or(StrongsNumber::OutOfRange, StrongsNumber::Entry))
        }
    }
}

/// ECMAScript's `StrWhiteSpaceChar`: what `parseInt` skips at the start.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(strong: &str) -> Vec<(LexiconId, Option<StrongsNumber>)> {
        strongs_parts(strong)
            .iter()
            .map(|p| (p.lexicon, p.number))
            .collect()
    }

    const fn entry(n: u32) -> Option<StrongsNumber> {
        Some(StrongsNumber::Entry(n))
    }

    #[test]
    fn greek_numbers_lose_their_variant_digit_and_hebrew_numbers_keep_every_digit() {
        assert_eq!(numbers("G35880"), [(LexiconId::Ugl, entry(3588))]);
        assert_eq!(numbers("G3588"), [(LexiconId::Ugl, entry(358))]);
        assert_eq!(numbers("H1234"), [(LexiconId::Uhl, entry(1234))]);
        assert_eq!(numbers("H1254a"), [(LexiconId::Uhl, entry(1254))]);
    }

    #[test]
    fn prefixes_are_parts_that_name_no_entry() {
        assert_eq!(
            numbers("c:d:H0776"),
            [
                (LexiconId::Uhl, None),
                (LexiconId::Uhl, None),
                (LexiconId::Uhl, entry(776))
            ]
        );
        assert!(contains_valid_strongs_number("b:H7225"));
        assert!(!contains_valid_strongs_number("c:d"));
        assert!(strongs_parts("").is_empty());
    }

    #[test]
    fn javascript_parse_int_quirks_are_kept() {
        // Only a part starting with `G` is Greek; a lowercase `g` is read as Hebrew.
        assert_eq!(numbers("g35880"), [(LexiconId::Uhl, entry(3588))]);
        assert_eq!(numbers(" G1230"), [(LexiconId::Uhl, entry(123))]);
        assert_eq!(numbers("G0x1F0"), [(LexiconId::Ugl, entry(31))]);
        assert_eq!(
            numbers("H-12"),
            [(LexiconId::Uhl, Some(StrongsNumber::OutOfRange))]
        );
        assert_eq!(numbers("H-0"), [(LexiconId::Uhl, None)]);
        assert_eq!(numbers("H\u{a0}\u{feff}12"), [(LexiconId::Uhl, entry(12))]);
        assert_eq!(numbers("H\u{85}12"), [(LexiconId::Uhl, None)]);
        assert_eq!(
            numbers("H4294967296"),
            [(LexiconId::Uhl, Some(StrongsNumber::OutOfRange))]
        );
        assert_eq!(numbers("G\u{1F600}"), [(LexiconId::Ugl, None)]);
    }
}
