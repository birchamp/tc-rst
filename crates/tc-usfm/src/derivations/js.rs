//! JavaScript string semantics on UTF-16 code units, so the port of usfm-js computes every
//! index, length and character class exactly as the library does under Node.

use std::cmp::Ordering;

/// A JavaScript string: UTF-16 code units, lone surrogates allowed.
pub(super) type JsStr = Vec<u16>;

pub(super) fn js(text: &str) -> JsStr {
    text.encode_utf16().collect()
}

pub(super) fn to_string(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// True when `units` holds exactly the ASCII text `ascii`.
pub(super) fn eq(units: &[u16], ascii: &str) -> bool {
    units.len() == ascii.len()
        && units
            .iter()
            .zip(ascii.bytes())
            .all(|(u, b)| *u == u16::from(b))
}

/// Orders code units against an ASCII text, as JavaScript compares strings.
pub(super) fn cmp_ascii(units: &[u16], ascii: &str) -> Ordering {
    units.iter().copied().cmp(ascii.bytes().map(u16::from))
}

pub(super) fn concat(parts: &[&[u16]]) -> JsStr {
    let mut out = Vec::with_capacity(parts.iter().map(|p| p.len()).sum());
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

/// `\s` and what `String.prototype.trim` removes: WhiteSpace and LineTerminator.
pub(super) fn is_space(unit: u16) -> bool {
    matches!(
        unit,
        0x09..=0x0D
            | 0x20
            | 0xA0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
            | 0xFEFF
    )
}

/// `\w`: ASCII letters, digits and `_`.
pub(super) fn is_word(unit: u16) -> bool {
    matches!(unit, 0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A | 0x5F)
}

pub(super) fn is_digit(unit: u16) -> bool {
    (0x30..=0x39).contains(&unit)
}

/// What `.` does not match: the line terminators.
pub(super) fn is_line_terminator(unit: u16) -> bool {
    matches!(unit, 0x0A | 0x0D | 0x2028 | 0x2029)
}

pub(super) fn trim(units: &[u16]) -> &[u16] {
    let start = units
        .iter()
        .position(|u| !is_space(*u))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|u| !is_space(*u))
        .map_or(start, |at| at + 1);
    &units[start..end.max(start)]
}

/// `String.prototype.substr(start, length)`.
pub(super) fn substr(units: &[u16], start: isize, length: Option<isize>) -> &[u16] {
    let len = units.len() as isize;
    let start = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    let count = length.map_or(len - start, |l| l.clamp(0, len - start));
    &units[start as usize..(start + count) as usize]
}

/// `String.prototype.substring(a, b)`.
pub(super) fn substring(units: &[u16], a: isize, b: isize) -> &[u16] {
    let len = units.len() as isize;
    let (a, b) = (a.clamp(0, len), b.clamp(0, len));
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    &units[from as usize..to as usize]
}

/// `String.prototype.indexOf(needle, from)`, `None` for -1.
pub(super) fn index_of(units: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    let from = from.min(units.len());
    if needle.is_empty() {
        return Some(from);
    }
    if needle.len() > units.len() {
        return None;
    }
    (from..=units.len() - needle.len()).find(|at| &units[*at..*at + needle.len()] == needle)
}

/// `String.prototype.lastIndexOf(needle)`, `None` for -1.
pub(super) fn last_index_of(units: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.len() > units.len() {
        return None;
    }
    (0..=units.len() - needle.len())
        .rev()
        .find(|at| &units[*at..*at + needle.len()] == needle)
}

/// `String.prototype.split(sep)` for a non-empty separator.
pub(super) fn split<'a>(units: &'a [u16], sep: &[u16]) -> Vec<&'a [u16]> {
    let mut parts = Vec::new();
    let mut rest = units;
    while let Some(at) = index_of(rest, sep, 0) {
        parts.push(&rest[..at]);
        rest = &rest[at + sep.len()..];
    }
    parts.push(rest);
    parts
}

pub(super) fn starts_with(units: &[u16], ascii: &str) -> bool {
    units.len() >= ascii.len() && eq(&units[..ascii.len()], ascii)
}

pub(super) fn ends_with(units: &[u16], ascii: &str) -> bool {
    units.len() >= ascii.len() && eq(&units[units.len() - ascii.len()..], ascii)
}

/// `ToNumber` of a string (`StringToNumber`): surrounding whitespace ignored, empty is 0,
/// decimal literals with sign, fraction and exponent, `Infinity`, and `0x`, `0o`, `0b`
/// integers; anything else is NaN.
pub(super) fn to_number(units: &[u16]) -> f64 {
    let text = to_string(trim(units));
    if text.is_empty() {
        return 0.0;
    }
    let lower = text.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits
                .chars()
                .filter_map(|c| c.to_digit(radix))
                .fold(0.0, |acc, d| acc * f64::from(radix) + f64::from(d));
        }
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(&text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    // Rust also reads `inf`, `nan` and `infinity` in any case, which JavaScript does not.
    let decimal = unsigned
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'));
    if !decimal
        || !unsigned
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_digit() || b == b'.')
    {
        return f64::NAN;
    }
    text.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Number.prototype.toString()` for the values the parser can hold.
pub(super) fn number_to_string(n: f64) -> JsStr {
    if n.is_nan() {
        return js("NaN");
    }
    if n.is_infinite() {
        return js(if n > 0.0 { "Infinity" } else { "-Infinity" });
    }
    if n == n.trunc() && n.abs() < 1e21 {
        return js(&format!("{}", n as i128));
    }
    js(&format!("{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substr_and_substring_follow_javascript() {
        let s = js("abcdef");
        assert_eq!(substr(&s, -1, None), js("f"));
        assert_eq!(substr(&s, 2, Some(2)), js("cd"));
        assert_eq!(substr(&s, 10, None), js(""));
        assert_eq!(substr(&s, -10, Some(2)), js("ab"));
        assert_eq!(substring(&s, 4, 1), js("bcd"));
        assert_eq!(substring(&s, -3, 2), js("ab"));
    }

    #[test]
    fn search_and_split_follow_javascript() {
        let s = js("a|b||c");
        assert_eq!(
            split(&s, &js("|")),
            vec![&js("a")[..], &js("b"), &[], &js("c")]
        );
        assert_eq!(index_of(&s, &js("|"), 2), Some(3));
        assert_eq!(index_of(&s, &js(""), 99), Some(6));
        assert_eq!(last_index_of(&s, &js("|")), Some(4));
        assert_eq!(trim(&js("\u{3000} a b\u{feff}\n")), js("a b"));
    }

    #[test]
    fn numbers_follow_javascript() {
        assert_eq!(to_number(&js(" 12 ")), 12.0);
        assert_eq!(to_number(&js("+5")), 5.0);
        assert_eq!(to_number(&js("")), 0.0);
        assert!(to_number(&js("1-3")).is_nan());
        assert!(to_number(&js("inf")).is_nan());
        assert_eq!(to_number(&js("0x1f")), 31.0);
        assert_eq!(number_to_string(3.0), js("3"));
        assert_eq!(number_to_string(f64::NAN), js("NaN"));
    }
}
