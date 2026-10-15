use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Why a string is not an RFC 3339 date-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub enum TimestampError {
    #[error("the date-time has no timezone; end it with 'Z' or an offset such as '+01:00'")]
    MissingTimezone,
    #[error("not an RFC 3339 date-time such as 2026-07-02T12:00:00.000Z")]
    Malformed,
}

/// A timestamp as the burrito writes it. Reading keeps any string, so a read-modify-write
/// never rewrites one; [`Timestamp::check`] judges it. New timestamps are RFC 3339 in UTC
/// with milliseconds, the form tC4 writes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    /// Times before 1970 are written as the epoch and times after 9999 as the last second
    /// of 9999, because RFC 3339 has four-digit years and a clock that far off is broken.
    pub fn from_system_time(time: SystemTime) -> Self {
        const LAST_SECOND_OF_9999: u64 = 253_402_300_799;
        let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
        let (seconds, millis) = if since.as_secs() > LAST_SECOND_OF_9999 {
            (LAST_SECOND_OF_9999, 999)
        } else {
            (since.as_secs(), since.subsec_millis())
        };
        let (year, month, day) = civil_from_days(seconds / 86_400);
        let of_day = seconds % 86_400;
        Self(format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
            of_day / 3600,
            of_day / 60 % 60,
            of_day % 60
        ))
    }

    /// Keeps `text` as written, valid or not.
    pub fn from_text(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks that the value is an RFC 3339 date-time with a timezone.
    pub fn check(&self) -> Result<(), TimestampError> {
        check_date_time(&self.0)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// Checks `text` against RFC 3339 `date-time`: `YYYY-MM-DDTHH:MM:SS[.fraction]` then `Z` or
/// `±HH:MM`. `T` and `Z` may be lower case, as the RFC allows. Day numbers are checked
/// against the month, leap years included, and a leap second `:60` only at 23:59 UTC, as
/// the schema's `date-time` format checks it.
pub(crate) fn check_date_time(text: &str) -> Result<(), TimestampError> {
    let mut cursor = Cursor(text.as_bytes());
    let date_ok = cursor.date();
    let time = date_ok && matches!(cursor.byte(), Some(b'T' | b't'));
    let Some((minute_of_day, second)) = time.then(|| cursor.time()).flatten() else {
        return Err(TimestampError::Malformed);
    };
    if cursor.0.is_empty() {
        return Err(TimestampError::MissingTimezone);
    }
    let Some(offset) = cursor.zone().filter(|_| cursor.0.is_empty()) else {
        return Err(TimestampError::Malformed);
    };
    let utc_minute = (minute_of_day - offset).rem_euclid(24 * 60);
    if second == 60 && utc_minute != 23 * 60 + 59 {
        return Err(TimestampError::Malformed);
    }
    Ok(())
}

/// The date forms the schema means for `identification.primary` timestamps
/// (`common.schema.json` `timestamp`): `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, year 1000 to 2999.
pub(crate) fn is_schema_date(text: &str) -> bool {
    let mut cursor = Cursor(text.as_bytes());
    let Some(year) = cursor.digits(4) else {
        return false;
    };
    if !(1000..=2999).contains(&year) {
        return false;
    }
    if cursor.0.is_empty() {
        return true;
    }
    if cursor.byte() != Some(b'-') {
        return false;
    }
    let Some(month) = cursor.digits(2).filter(|m| (1..=12).contains(m)) else {
        return false;
    };
    if cursor.0.is_empty() {
        return true;
    }
    if cursor.byte() != Some(b'-') {
        return false;
    }
    let day_ok = cursor
        .digits(2)
        .is_some_and(|day| day >= 1 && day <= days_in_month(year, month));
    day_ok && cursor.0.is_empty()
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn byte(&mut self) -> Option<u8> {
        let (first, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(*first)
    }

    fn expect(&mut self, wanted: u8) -> Option<()> {
        (self.byte()? == wanted).then_some(())
    }

    fn digits(&mut self, count: usize) -> Option<u32> {
        let head = self.0.get(..count)?;
        if !head.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.0 = &self.0[count..];
        Some(
            head.iter()
                .fold(0, |total, digit| total * 10 + u32::from(digit - b'0')),
        )
    }

    fn date(&mut self) -> bool {
        let Some(year) = self.digits(4) else {
            return false;
        };
        if self.byte() != Some(b'-') {
            return false;
        }
        let Some(month) = self.digits(2).filter(|m| (1..=12).contains(m)) else {
            return false;
        };
        if self.byte() != Some(b'-') {
            return false;
        }
        self.digits(2)
            .is_some_and(|day| day >= 1 && day <= days_in_month(year, month))
    }

    /// `HH:MM:SS[.fraction]`, as the minute of the day and the second (60 is a leap second).
    fn time(&mut self) -> Option<(i32, u32)> {
        let hour = self.digits(2).filter(|h| *h <= 23)?;
        self.expect(b':')?;
        let minute = self.digits(2).filter(|m| *m <= 59)?;
        self.expect(b':')?;
        let second = self.digits(2).filter(|s| *s <= 60)?;
        if let Some(rest) = self.0.strip_prefix(b".") {
            let fraction = rest.iter().take_while(|b| b.is_ascii_digit()).count();
            if fraction == 0 {
                return None;
            }
            self.0 = &rest[fraction..];
        }
        let minute_of_day = i32::try_from(hour * 60 + minute).ok()?;
        Some((minute_of_day, second))
    }

    /// `Z` or `±HH:MM`, as minutes east of UTC.
    fn zone(&mut self) -> Option<i32> {
        let sign = match self.byte()? {
            b'Z' | b'z' => return Some(0),
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let hours = self.digits(2).filter(|h| *h <= 23)?;
        self.expect(b':')?;
        let minutes = self.digits(2).filter(|m| *m <= 59)?;
        Some(sign * i32::try_from(hours * 60 + minutes).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn date_times_need_a_timezone() {
        for ok in [
            "2026-07-02T12:00:00.000Z",
            "2026-07-02T12:00:00Z",
            "2026-07-02t12:00:00z",
            "2026-07-02T12:00:00+01:00",
            "2016-12-31T23:59:60Z",
            "2016-12-31T18:29:60-05:30",
            "2017-01-01T00:59:60+01:00",
        ] {
            assert_eq!(check_date_time(ok), Ok(()), "{ok}");
        }
        for missing in ["2026-07-02T12:00:00", "2026-07-02T12:00:00.123"] {
            assert_eq!(
                check_date_time(missing),
                Err(TimestampError::MissingTimezone)
            );
        }
        for bad in [
            "2026-07-02",
            "yesterday",
            "not a date",
            "2026-13-01T00:00:00Z",
            "2025-02-29T00:00:00Z",
            "2026-07-02T24:00:00Z",
            "2026-07-02 12:00:00Z",
            "2026-07-02T12:00:00.Z",
            "2026-07-02T12:00:00+0100",
            "2026-07-02T12:00:00Z ",
            "2026-07-02T12:00:60Z",
            "2016-12-31T23:59:60+01:00",
            "",
        ] {
            assert_eq!(
                check_date_time(bad),
                Err(TimestampError::Malformed),
                "{bad}"
            );
        }
    }

    #[test]
    fn schema_dates_are_year_month_or_day() {
        for ok in ["2026", "2026-07", "2026-07-02", "2024-02-29"] {
            assert!(is_schema_date(ok), "{ok}");
        }
        for bad in [
            "26",
            "2026-7",
            "2026-07-32",
            "2025-02-29",
            "3026",
            "2026-07-02T00:00:00Z",
        ] {
            assert!(!is_schema_date(bad), "{bad}");
        }
    }

    #[test]
    fn new_timestamps_are_utc_with_milliseconds() {
        let at = |seconds: u64, millis: u64| {
            Timestamp::from_system_time(UNIX_EPOCH + Duration::from_millis(seconds * 1000 + millis))
        };
        assert_eq!(at(0, 0).as_str(), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(1_782_993_600, 0).as_str(), "2026-07-02T12:00:00.000Z");
        assert_eq!(at(951_782_400, 7).as_str(), "2000-02-29T00:00:00.007Z");
        assert_eq!(at(253_402_300_799, 0).as_str(), "9999-12-31T23:59:59.000Z");
        assert_eq!(at(253_402_300_800, 0).as_str(), "9999-12-31T23:59:59.999Z");
        assert_eq!(Timestamp::now().check(), Ok(()));
    }
}
