//! The drivers the scorecard names, as Driver Match reads them: by the weeks it covers.
use crate::backend::WeeklyScorecardStore;
use dispatch_core::{
    Result,
    db::{Store, n, s},
    foundation::weeks,
    manifest::people::{self, Appearances, DriverData, DriverSource, Named, People, PeopleData},
};

/// The Saturday a scorecard week ends, or the week as written when it does not parse.
fn week_end(week: &str) -> String {
    weeks::week_days(week).map_or_else(|_| week.to_owned(), |(_, end)| end.to_string())
}
fn week_start(week: &str) -> String {
    weeks::week_days(week).map_or_else(|_| week.to_owned(), |(start, _)| start.to_string())
}

/// What it names people in, as Driver Match lists it.
const DATA: PeopleData = PeopleData {
    id: "weekly_scorecard",
    order: 50,
};
pub struct Drivers;
impl People for Drivers {
    fn data(&self) -> DriverData {
        DriverData::new(&DATA)
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    /// After routes and meal breaks: the scorecard often adds a middle name.
    fn order(&self) -> u16 {
        40
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.weekly_scorecard_db(dsp)?.all(
            "SELECT d.transporter_id id,json_extract(d.row,'$.da_name') name,\
             min(p.week) first,max(p.week) last FROM driver_weekly_scorecards d \
             JOIN weekly_scorecard_publications p ON p.id=d.publication_id \
             WHERE p.scope_verified=1 AND COALESCE(d.transporter_id,'')<>'' GROUP BY 1,2 ORDER BY last DESC",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut named = Named::new(s(row, "id"));
                named.name = s(row, "name").to_owned();
                named.seen(&week_start(s(row, "first")), &week_end(s(row, "last")));
                named
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.weekly_scorecard_db(dsp)?,
            "SELECT json_extract(d.row,'$.da_name') FROM driver_weekly_scorecards d \
             JOIN weekly_scorecard_publications p ON p.id=d.publication_id \
             WHERE d.transporter_id=? AND p.scope_verified=1 ORDER BY p.week DESC",
            id,
        )
    }
    /// Counted in weeks, each last seen on the Saturday it ends.
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.weekly_scorecard_db(dsp)?.all(
            "SELECT d.transporter_id id,count(DISTINCT p.week) count,max(p.week) last \
             FROM driver_weekly_scorecards d JOIN weekly_scorecard_publications p \
             ON p.id=d.publication_id AND p.active=1 AND p.scope_verified=1 \
             WHERE COALESCE(d.transporter_id,'')<>'' GROUP BY d.transporter_id",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| Appearances {
                id: s(row, "id").to_owned(),
                count: n(row, "count") as usize,
                last: week_end(s(row, "last")),
            })
            .collect())
    }
}
