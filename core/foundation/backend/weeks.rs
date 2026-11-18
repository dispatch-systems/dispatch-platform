//! ISO weeks as Amazon's reports name them.
use crate::{Error, Result, ensure};
use jiff::{
    ToSpan,
    civil::{Date, ISOWeekDate, Weekday},
};

// Amazon's scorecard week runs Sunday to Saturday and is named after the ISO week
// of its Saturday: 2026-W38 is September 13 to 19, 2026.
pub fn parse_week(week: &str) -> Result<(i32, u32)> {
    let invalid = || Error::new("invalid_week", 400);
    let (year, number) = week.split_once("-W").ok_or_else(invalid)?;
    ensure(year.len() == 4 && number.len() == 2, "invalid_week", 400)?;
    let year: i32 = year.parse().map_err(|_| invalid())?;
    let number: u32 = number.parse().map_err(|_| invalid())?;
    ensure((2000..=2100).contains(&year), "invalid_week", 400)?;
    saturday(year, number).ok_or_else(invalid)?;
    Ok((year, number))
}
/// The Saturday of the ISO week `number` of `year`, while the year has that week.
fn saturday(year: i32, number: u32) -> Option<Date> {
    let (year, number) = (i16::try_from(year).ok()?, i8::try_from(number).ok()?);
    let week = ISOWeekDate::new(year, number, Weekday::Saturday).ok()?;
    Some(week.date())
}
/// The week's first and last day: its Sunday and its Saturday.
pub fn week_days(week: &str) -> Result<(Date, Date)> {
    let (year, number) = parse_week(week)?;
    let saturday = saturday(year, number).ok_or_else(|| Error::new("invalid_week", 400))?;
    Ok((saturday - 6.days(), saturday))
}
/// The week a Saturday ends.
fn week_of(saturday: Date) -> String {
    let iso = saturday.iso_week_date();
    format!("{}-W{:02}", iso.year(), iso.week())
}
/// The most recent week that ended before `today`.
pub fn last_completed_week(today: Date) -> String {
    let back = (today.weekday().to_sunday_zero_offset() + 1) % 7;
    let back = if back == 0 { 7 } else { back };
    week_of(today - back.days())
}
/// `week` and the `count` weeks before it, newest first.
pub fn weeks_before(week: &str, count: usize) -> Result<Vec<String>> {
    let (_, saturday) = week_days(week)?;
    Ok((0..=count)
        .map(|index| week_of(saturday - (index as i64).weeks()))
        .collect())
}
/// `week` in the DSP's local calendar, or the last completed one.
pub fn week_or_latest(week: Option<&str>, today: Date) -> Result<String> {
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
