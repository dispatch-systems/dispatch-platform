//! The drivers the scorecard names, as Driver Match reads them: by the weeks it covers.
use crate::backend::WeeklyScorecardStore;
use dispatch_core::{
    Result,
    db::Store,
    foundation::weeks,
    manifest::people::{self, Named, People, PeopleData, Records},
};

/// Every driver a verified scorecard names, counted in weeks.
pub const PEOPLE: People = People::new(
    PeopleData {
        id: "weekly_scorecard",
        order: 50,
        label: "Weekly Scorecard",
        unit: "week",
        icon: "Award",
    },
    &dispatch_cortex::PEOPLE,
    drivers,
)
.records(scored_weeks);

/// Each driver, seen from the Sunday their first week starts to the Saturday their last ends.
fn drivers(store: &Store, dsp: &str) -> Result<Vec<Named>> {
    let mut named = people::named(
        &*store.weekly_scorecard_db(dsp)?,
        "SELECT d.transporter_id id,json_extract(d.row,'$.da_name') name,\
         min(p.week) first,max(p.week) last FROM driver_weekly_scorecards d \
         JOIN weekly_scorecard_publications p ON p.id=d.publication_id \
         WHERE p.scope_verified=1 AND COALESCE(d.transporter_id,'')<>'' \
         GROUP BY 1,2 ORDER BY last DESC",
    )?;
    for driver in &mut named {
        driver.first_seen = driver.first_seen.as_deref().map(week_start);
        driver.last_seen = driver.last_seen.as_deref().map(week_end);
    }
    Ok(named)
}
/// The weeks each driver is scored in, the last seen on the Saturday it ends.
fn scored_weeks(store: &Store, dsp: &str) -> Result<Vec<Records>> {
    let mut records = people::records(
        &*store.weekly_scorecard_db(dsp)?,
        "SELECT d.transporter_id id,count(DISTINCT p.week) count,max(p.week) last \
         FROM driver_weekly_scorecards d JOIN weekly_scorecard_publications p \
         ON p.id=d.publication_id AND p.active=1 AND p.scope_verified=1 \
         WHERE COALESCE(d.transporter_id,'')<>'' GROUP BY d.transporter_id",
    )?;
    for record in &mut records {
        record.last = week_end(&record.last);
    }
    Ok(records)
}

/// The Saturday a scorecard week ends, or the week as written when it does not parse.
fn week_end(week: &str) -> String {
    weeks::week_days(week).map_or_else(|_| week.to_owned(), |(_, end)| end.to_string())
}
fn week_start(week: &str) -> String {
    weeks::week_days(week).map_or_else(|_| week.to_owned(), |(start, _)| start.to_string())
}
