//! ISO weeks as Amazon's reports name them.
use crate::{Error, Result, ensure};
use chrono::{Datelike, Duration, NaiveDate, Weekday};

// Amazon's scorecard week runs Sunday to Saturday and is named after the ISO week
// of its Saturday: 2026-W38 is September 13 to 19, 2026.
pub fn parse_week(week: &str) -> Result<(i32, u32)> {
    let invalid = || Error::new("invalid_week", 400);
    let (year, number) = week.split_once("-W").ok_or_else(invalid)?;
    ensure(year.len() == 4 && number.len() == 2, "invalid_week", 400)?;
    let year: i32 = year.parse().map_err(|_| invalid())?;
    let number: u32 = number.parse().map_err(|_| invalid())?;
    ensure((2000..=2100).contains(&year), "invalid_week", 400)?;
    NaiveDate::from_isoywd_opt(year, number, Weekday::Sat).ok_or_else(invalid)?;
    Ok((year, number))
}
/// The week's first and last day: its Sunday and its Saturday.
pub fn week_days(week: &str) -> Result<(NaiveDate, NaiveDate)> {
    let (year, number) = parse_week(week)?;
    let saturday = NaiveDate::from_isoywd_opt(year, number, Weekday::Sat)
        .ok_or_else(|| Error::new("invalid_week", 400))?;
    Ok((saturday - Duration::days(6), saturday))
}
/// The week a Saturday ends.
fn week_of(saturday: NaiveDate) -> String {
    let iso = saturday.iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}
/// The most recent week that ended before `today`.
pub fn last_completed_week(today: NaiveDate) -> String {
    let back = (today.weekday().num_days_from_sunday() + 1) % 7;
    let back = if back == 0 { 7 } else { back };
    week_of(today - Duration::days(i64::from(back)))
}
/// `week` and the `count` weeks before it, newest first.
pub fn weeks_before(week: &str, count: usize) -> Result<Vec<String>> {
    let (_, saturday) = week_days(week)?;
    Ok((0..=count)
        .map(|index| week_of(saturday - Duration::weeks(index as i64)))
        .collect())
}
/// `week` in the DSP's local calendar, or the last completed one.
pub fn week_or_latest(week: Option<&str>, today: NaiveDate) -> Result<String> {
    match week {
        Some(week) => {
            parse_week(week)?;
            ensure(
                week <= last_completed_week(today).as_str(),
                "week_not_completed",
                400,
            )?;
            Ok(week.to_owned())
        }
        None => Ok(last_completed_week(today)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }
    #[test]
    fn weeks_run_sunday_to_saturday_and_are_named_after_the_saturday() {
        assert_eq!(
            week_days("2026-W38").unwrap(),
            (date("2026-09-13"), date("2026-09-19"))
        );
        assert_eq!(week_of(date("2026-09-19")), "2026-W38");
        // Every day of the following week still sees week 38 as the last completed one.
        for day in ["2026-09-20", "2026-09-23", "2026-09-26"] {
            assert_eq!(last_completed_week(date(day)), "2026-W38", "{day}");
        }
        assert_eq!(last_completed_week(date("2026-09-27")), "2026-W39");
        assert_eq!(
            weeks_before("2026-W02", 3).unwrap(),
            ["2026-W02", "2026-W01", "2025-W52", "2025-W51"]
        );
        for invalid in ["2026-W00", "2026-W54", "2026-38", "26-W38", "2026-W3"] {
            assert!(parse_week(invalid).is_err(), "{invalid}");
        }
        assert!(week_or_latest(Some("2026-W39"), date("2026-09-25")).is_err());
        assert_eq!(
            week_or_latest(None, date("2026-09-25")).unwrap(),
            "2026-W38"
        );
    }
}
