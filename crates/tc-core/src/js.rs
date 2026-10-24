//! JavaScript's whitespace, which the ports of tC4's rules depend on: one table, so every
//! crate trims and splits as tC4 does.

/// ECMAScript's `WhiteSpace` and `LineTerminator`: what `\s` matches, what
/// `String.prototype.trim` removes and what `parseInt` skips. Rust's `char::is_whitespace`
/// differs: it treats U+0085 as whitespace and U+FEFF as not.
pub fn is_space(c: char) -> bool {
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
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spaces_are_ecmascripts() {
        let spaces: Vec<u32> = (0..=0x10ffff)
            .filter_map(char::from_u32)
            .filter(|c| is_space(*c))
            .map(u32::from)
            .collect();
        let mut expected = vec![
            0x9, 0xa, 0xb, 0xc, 0xd, 0x20, 0xa0, 0x1680, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000,
            0xfeff,
        ];
        expected.extend(0x2000..=0x200a);
        expected.sort_unstable();
        assert_eq!(spaces, expected);
        // Rust's whitespace with U+0085 left out and U+FEFF added: the same set.
        for c in (0..=0x10ffff).filter_map(char::from_u32) {
            let rust = (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}';
            assert_eq!(is_space(c), rust, "U+{:04X}", u32::from(c));
        }
        assert_eq!(trim("\u{feff} a b\u{3000}"), "a b");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }
}
