use crate::{
    api::meals::{MealPeriod, PeriodMealGap},
    backend::TimecardStore,
};
use dispatch_core::{Result, db::Store, ensure};
use dispatch_paycom::timecards::EmployeeTimecardPeriod;

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
    let mut gaps = Vec::new();
    for day in &days {
        for row in &day.rows {
            for (index, pair) in row.assessment.pairs.iter().enumerate() {
                for (side, gap) in [("before", &pair.gaps.before), ("after", &pair.gaps.after)] {
                    if let Some(gap) = gap.as_ref().filter(|gap| gap.over_limit) {
                        gaps.push(PeriodMealGap {
                            date: day.date.clone(),
                            meal: index + 1,
                            side: side.into(),
                            milliseconds: gap.milliseconds,
                        });
                    }
                }
            }
        }
    }
    let excess_milliseconds = gaps.iter().map(|gap| gap.milliseconds - 300_000).sum();
    Ok(MealPeriod {
        period,
        days,
        gaps,
        excess_milliseconds,
    })
}
