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
#[path = "../tests/backend/weeks.rs"]
mod tests;
