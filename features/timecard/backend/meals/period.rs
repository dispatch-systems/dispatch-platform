use crate::{
    api::meals::{MealComparison, MealPeriod, MealPeriodGaps, PeriodMealGap},
    backend::TimecardStore,
};
use dispatch_core::{Result, db::Store, ensure};
use dispatch_paycom::timecards::EmployeeTimecardPeriod;
use std::collections::BTreeMap;

pub(crate) fn meal_period(
    store: &Store,
    id: &str,
    source: &str,
    date: Option<&str>,
    requested: Option<&EmployeeTimecardPeriod>,
    timezone: &str,
) -> Result<MealPeriod> {
    let code = source.strip_prefix("paycom:");
    ensure(
        code.is_some()
            || source
                .strip_prefix("cortex:")
                .is_some_and(|id| !id.is_empty()),
        "invalid_source",
        400,
    )?;
    let period = store.timecard_period(id, code, date, requested)?;
    let selected = [source.to_owned()];
    let days: Vec<_> = store
        .meal_comparisons(id, &period.from, &period.to, timezone, Some(&selected))?
        .into_values()
        .collect();
    let gaps: Vec<_> = days
        .iter()
        .flat_map(verified_gaps)
        .map(|(_, gap)| gap)
        .collect();
    let excess_milliseconds = gaps.iter().map(|gap| gap.milliseconds - 300_000).sum();
    Ok(MealPeriod {
        period,
        days,
        gaps,
        excess_milliseconds,
    })
}

/// Every person's verified gaps over the timecard period holding `date`, counted as their
/// own period's summary counts them. Every Paycom period follows one calendar, so the DSP's
/// period for the day is each person's.
pub(crate) fn meal_period_gaps(
    store: &Store,
    id: &str,
    date: &str,
    timezone: &str,
) -> Result<MealPeriodGaps> {
    let mut counts = BTreeMap::new();
    let period = match store.timecard_period(id, None, Some(date), None) {
        Ok(period) => period,
        // Before Paycom's first collection, or past its latest period, the day has none.
        Err(error)
            if matches!(
                error.code.as_str(),
                "timecard_period_unavailable" | "invalid_period"
            ) =>
        {
            return Ok(MealPeriodGaps {
                period: None,
                counts,
            });
        }
        Err(error) => return Err(error),
    };
    for day in store
        .meal_comparisons(id, &period.from, &period.to, timezone, None)?
        .values()
    {
        for (row, _) in verified_gaps(day) {
            *counts.entry(row.to_owned()).or_default() += 1;
        }
    }
    Ok(MealPeriodGaps {
        period: Some(period),
        counts,
    })
}

/// A day's delivery gaps over five minutes, before and after each meal, with the id of the
/// row each belongs to.
fn verified_gaps(day: &MealComparison) -> impl Iterator<Item = (&str, PeriodMealGap)> {
    day.rows.iter().flat_map(move |row| {
        row.assessment
            .pairs
            .iter()
            .enumerate()
            .flat_map(move |(index, pair)| {
                [("before", &pair.gaps.before), ("after", &pair.gaps.after)]
                    .into_iter()
                    .filter_map(move |(side, gap)| {
                        let gap = gap.as_ref().filter(|gap| gap.over_limit)?;
                        Some((
                            row.source.id.as_str(),
                            PeriodMealGap {
                                date: day.date.clone(),
                                meal: index + 1,
                                side: side.into(),
                                milliseconds: gap.milliseconds,
                            },
                        ))
                    })
            })
    })
}
