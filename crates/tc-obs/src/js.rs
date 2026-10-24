//! The few JavaScript built-ins whose exact behaviour tC4's OBS rules depend on.

use std::cmp::Ordering;

pub(crate) use tc_core::js::{is_space, trim};

/// tC4's one blank-line predicate (`journal/story.mjs:46`): the line trims to nothing.
pub(crate) fn is_blank(line: &str) -> bool {
    line.chars().all(is_space)
}

/// `decodeURIComponent`, or `None` where it throws: a `%` without two hex digits after
/// it, or escapes that are not UTF-8.
pub(crate) fn decode_uri_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let high = char::from(hex[0]).to_digit(16)?;
            let low = char::from(hex[1]).to_digit(16)?;
            // Two hex digits make one byte.
            out.push(u8::try_from(high * 16 + low).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The order of JavaScript's default `Array.prototype.sort`: by UTF-16 code units.
pub(crate) fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `Number(text)` for a string: the StringNumericLiteral grammar after trimming, `NaN`
/// for anything else.
pub(crate) fn to_number(text: &str) -> f64 {
    let s = trim(text);
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            return radix_number(digits, radix);
        }
    }
    let (sign, unsigned) = match s.as_bytes()[0] {
        b'+' => (1.0, &s[1..]),
        b'-' => (-1.0, &s[1..]),
        _ => (1.0, s),
    };
    if unsigned == "Infinity" {
        return sign * f64::INFINITY;
    }
    if !decimal_literal(unsigned) {
        return f64::NAN;
    }
    // The grammar above is a subset of what Rust's float parser reads, and both round
    // to the nearest double.
    unsigned.parse::<f64>().map_or(f64::NAN, |n| sign * n)
}

fn radix_number(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut value = 0.0_f64;
    for c in digits.chars() {
        match c.to_digit(radix) {
            Some(d) => value = value * f64::from(radix) + f64::from(d),
            None => return f64::NAN,
        }
    }
    value
}

/// `StrUnsignedDecimalLiteral` without `Infinity`: digits with an optional fraction, or a
/// fraction alone, then an optional exponent.
fn decimal_literal(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let int = digits(0);
    let mut i = int;
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        frac = digits(i + 1);
        i += 1 + frac;
    }
    if int == 0 && frac == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp = digits(i);
        if exp == 0 {
            return false;
        }
        i += exp;
    }
    i == b.len()
}

/// `Math.round` for the non-negative values the percentages need.
pub(crate) fn round(x: f64) -> f64 {
    // `f64::round` rounds halves away from zero, which for non-negative values is
    // JavaScript's rounding of halves up.
    x.round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_follows_javascript_not_rust() {
        assert_eq!(trim("\u{feff} x \u{a0}"), "x");
        assert_eq!(trim("\u{85}x"), "\u{85}x");
        assert!(is_blank("\u{2028}\u{3000}"));
        assert!(!is_blank("\u{200b}"));
        assert!(!is_blank("\u{180e}"));
    }

    #[test]
    fn decode_uri_component_refuses_what_javascript_throws_on() {
        assert_eq!(decode_uri_component("a%2Fb").as_deref(), Some("a/b"));
        assert_eq!(decode_uri_component("%F0%9F%98%80").as_deref(), Some("😀"));
        assert_eq!(decode_uri_component("%"), None);
        assert_eq!(decode_uri_component("%E0%A4%A"), None);
        assert_eq!(decode_uri_component("%C0%80"), None);
        assert_eq!(decode_uri_component("%ED%A0%80"), None);
    }
}
