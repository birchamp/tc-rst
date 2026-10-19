//! The few JavaScript built-ins whose exact behaviour the ports of tC4's rules depend on.

use std::cmp::Ordering;

/// `WhiteSpace` and `LineTerminator` of ECMAScript: what `String.prototype.trim` removes.
/// Rust's `char::is_whitespace` differs: it treats U+0085 as whitespace and U+FEFF as not.
pub(crate) fn is_space(c: char) -> bool {
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

/// `String.prototype.trim`.
pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// The order of JavaScript's `<` on strings, and of its default `Array.prototype.sort`: by
/// UTF-16 code units, which differs from Rust's order by code points for a character above
/// U+FFFF against one between U+E000 and U+FFFF.
pub(crate) fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `Math.round` of a value that is not negative: half rounds up.
pub(crate) fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_is_javascripts() {
        assert_eq!(trim("\u{feff} a \u{3000}"), "a");
        // NEL is not JavaScript whitespace.
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn strings_compare_by_utf16_code_units() {
        // U+FF21 is above U+E000; U+1F600 is above U+FFFF in code points but its first code
        // unit, U+D83D, is below U+FF21.
        assert_eq!(utf16_cmp("\u{1F600}", "\u{FF21}"), Ordering::Less);
        assert_eq!("\u{1F600}".cmp("\u{FF21}"), Ordering::Greater);
    }

    #[test]
    fn round_is_math_round_for_percentages() {
        assert_eq!(round(10.5), 11.0);
        assert_eq!(round(4.166), 4.0);
        assert_eq!(round(0.0), 0.0);
    }
}
