//! The few JavaScript value rules that tC4's checking code depends on, so the ports that
//! need them (reference sorting and the scope test) give tC4's answers on any input.

use std::cmp::Ordering;

pub(crate) use tc_core::js::is_space as is_whitespace;

/// `Number(text)`.
pub(crate) fn to_number(text: &str) -> f64 {
    let t = text.trim_matches(is_whitespace);
    if t.is_empty() {
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
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits
                .chars()
                .filter_map(|c| c.to_digit(radix))
                .fold(0.0, |n, d| n * f64::from(radix) + f64::from(d));
        }
    }
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    if unsigned == "Infinity" {
        return if t.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if !is_decimal_literal(unsigned) {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// `StrUnsignedDecimalLiteral` without `Infinity`: `1`, `1.`, `.5`, `1e3`, `1.5E-2`.
fn is_decimal_literal(s: &str) -> bool {
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let (int, frac) = match mantissa.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (mantissa, None),
    };
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = digits(int)
        && frac.is_none_or(digits)
        && !(int.is_empty() && frac.is_none_or(str::is_empty));
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exponent_ok
}

/// `parseInt(text, 10)`.
pub(crate) fn parse_int(text: &str) -> f64 {
    let t = text.trim_start_matches(is_whitespace);
    let (negative, rest) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let n = digits
        .bytes()
        .fold(0.0, |n, d| n * 10.0 + f64::from(d - b'0'));
    if negative { -n } else { n }
}

/// A JavaScript value as the ported code sees it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JsVal {
    Undefined,
    Num(f64),
    Str(String),
}

impl JsVal {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined => false,
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Str(s) => !s.is_empty(),
        }
    }

    /// `===`.
    pub fn strict_eq(&self, other: &JsVal) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) => true,
            (Self::Num(a), Self::Num(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            _ => false,
        }
    }

    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Num(n) => *n,
            Self::Str(s) => to_number(s),
        }
    }

    /// The abstract relational comparison: strings compare by UTF-16 code units, any
    /// other pair as numbers. `None` when a number is NaN.
    fn compare(&self, other: &JsVal) -> Option<Ordering> {
        if let (Self::Str(a), Self::Str(b)) = (self, other) {
            return Some(a.encode_utf16().cmp(b.encode_utf16()));
        }
        self.number().partial_cmp(&other.number())
    }

    pub fn lt(&self, other: &JsVal) -> bool {
        self.compare(other) == Some(Ordering::Less)
    }

    pub fn gt(&self, other: &JsVal) -> bool {
        self.compare(other) == Some(Ordering::Greater)
    }

    pub fn le(&self, other: &JsVal) -> bool {
        matches!(self.compare(other), Some(Ordering::Less | Ordering::Equal))
    }

    pub fn ge(&self, other: &JsVal) -> bool {
        matches!(
            self.compare(other),
            Some(Ordering::Greater | Ordering::Equal)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_reads_text_as_javascript_does() {
        assert_eq!(to_number("12"), 12.0);
        assert_eq!(to_number(" 7 "), 7.0);
        assert_eq!(to_number(""), 0.0);
        assert_eq!(to_number("0x1f"), 31.0);
        assert_eq!(to_number("1e3"), 1000.0);
        assert_eq!(to_number(".5"), 0.5);
        assert_eq!(to_number("-Infinity"), f64::NEG_INFINITY);
        for nan in [
            "9-10", "front", "1a", ".", "e3", "inf", "NaN", "-0x1", "1,3",
        ] {
            assert!(to_number(nan).is_nan(), "{nan:?}");
        }
    }

    #[test]
    fn parse_int_reads_a_leading_number() {
        assert_eq!(parse_int("1a"), 1.0);
        assert_eq!(parse_int(" -3x"), -3.0);
        assert!(parse_int("front").is_nan());
        assert!(parse_int("").is_nan());
    }

    #[test]
    fn comparisons_mix_types_as_javascript_does() {
        let n = |x: f64| JsVal::Num(x);
        let s = |x: &str| JsVal::Str(x.to_owned());
        assert!(n(1.0).le(&s("2")));
        assert!(!n(1.0).le(&s("front")));
        assert!(!s("front").ge(&n(1.0)));
        assert!(s("10").lt(&s("9")));
        assert!(!n(1.0).strict_eq(&s("1")));
        assert!(!JsVal::Undefined.le(&n(1.0)));
    }
}
