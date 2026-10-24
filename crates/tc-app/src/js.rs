//! The few JavaScript built-ins whose exact behaviour the ports of tC4's rules depend on.

use std::cmp::Ordering;

pub(crate) use tc_core::js::trim;

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

/// tC4's `toISOString` form, `YYYY-MM-DDTHH:MM:SS.sssZ`, or the same without milliseconds,
/// as milliseconds since the Unix epoch. `None` for any other text or an impossible date.
pub(crate) fn parse_utc_millis(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = b.get(range)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let shape = b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':';
    if !shape {
        return None;
    }
    let millis = match &b[19..] {
        [b'Z'] => 0,
        [b'.', a, c, d, b'Z'] if [a, c, d].iter().all(|x| x.is_ascii_digit()) => digits(20..23)?,
        _ => return None,
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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
