//! A calendar day (proleptic Gregorian, no time zone), written `YYYY-MM-DD`.

use std::fmt;

/// 0001-01-01 and 9999-12-31 as days since 1970-01-01.
const FIRST: i32 = -719_162;
const LAST: i32 = 2_932_896;

/// Days since 1970-01-01, so days compare and order as numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(i32);

impl Day {
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Day> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Day(days_from_civil(year, month, day)))
    }

    /// Reads exactly `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Day> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let number = |range: std::ops::Range<usize>| -> Option<u32> {
            let part = &text[range];
            part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse().ok())?
        };
        Day::from_ymd(number(0..4)? as i32, number(5..7)?, number(8..10)?)
    }

    /// The day `days` after 1970-01-01, kept within the years 1 to 9999.
    pub fn from_unix_days(days: i64) -> Day {
        Day(days.clamp(i64::from(FIRST), i64::from(LAST)) as i32)
    }

    /// Today in UTC by the computer's clock.
    pub fn today_utc() -> Day {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        Day::from_unix_days(seconds.div_euclid(86_400))
    }

    pub fn ymd(self) -> (i32, u32, u32) {
        civil_from_days(self.0)
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = self.ymd();
        write!(f, "{year:04}-{month:02}-{day:02}")
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's `days_from_civil` and `civil_from_days`.
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era as u32 * 365 + year_of_era as u32 / 4 - year_of_era as u32 / 100 + day_of_year;
    era * 146_097 + day_of_era as i32 - 719_468
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = (z - era * 146_097) as u32;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era as i32 + era * 400 + i32::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_days() {
        assert_eq!(Day::parse("1970-01-01"), Some(Day(0)));
        assert_eq!(Day::parse("2026-10-24").unwrap().to_string(), "2026-10-24");
        assert_eq!(Day::parse("2024-02-29").unwrap().to_string(), "2024-02-29");
        for bad in ["2025-02-29", "2026-13-01", "2026-00-10", "2026-1-01", "2026/01/01", "+026-01-01", "", "2026-01-32"] {
            assert_eq!(Day::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn round_trips_over_centuries() {
        for days in (-200_000..200_000).step_by(37) {
            let day = Day(days);
            let (y, m, d) = day.ymd();
            if (1..=9999).contains(&y) {
                assert_eq!(Day::from_ymd(y, m, d), Some(day));
            }
        }
        assert!(Day::parse("2027-01-01").unwrap() > Day::parse("2026-12-31").unwrap());
        assert_eq!(Day::from_unix_days(i64::MIN).to_string(), "0001-01-01");
        assert_eq!(Day::from_unix_days(i64::MAX).to_string(), "9999-12-31");
    }
}
