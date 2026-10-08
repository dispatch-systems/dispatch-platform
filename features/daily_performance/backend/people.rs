//! Daily driver identity feeds the existing Driver Match service.
use crate::DailyPerformanceStore;
use dispatch_core::{
    Result,
    db::{Store, n, s},
    manifest::people::{self, Appearances, Named, People},
    mcp::api::types::{DriverData, DriverSource, PeopleData},
};
/// What it names people in, as Driver Match lists it.
const DATA: PeopleData = PeopleData {
    id: "daily_performance",
    order: 60,
};
pub struct Drivers;
impl People for Drivers {
    fn data(&self) -> DriverData {
        DriverData::new(&DATA)
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    fn order(&self) -> u16 {
        45
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.daily_performance_db(dsp)?.all(
            "SELECT x.transporter_id id,json_extract(x.row,'$.da_name') name,min(x.date) first,max(x.date) last \
             FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id WHERE p.active=1 \
             AND COALESCE(x.transporter_id,'')<>'' AND COALESCE(json_extract(x.row,'$.da_name'),'')<>'' \
             GROUP BY 1,2 ORDER BY last DESC",[])?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut named = Named::new(s(row, "id"));
                named.name = s(row, "name").into();
                named.seen(s(row, "first"), s(row, "last"));
                named
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.daily_performance_db(dsp)?,
            "SELECT json_extract(x.row,'$.da_name') FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id \
             WHERE p.active=1 AND x.transporter_id=? AND COALESCE(json_extract(x.row,'$.da_name'),'')<>'' ORDER BY x.date DESC",
            id,
        )
    }
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.daily_performance_db(dsp)?.all(
            "SELECT x.transporter_id id,count(DISTINCT x.date) count,max(x.date) last FROM daily_rows x \
             JOIN daily_publications p ON p.id=x.publication_id WHERE p.active=1 AND x.dataset='driver_quality' \
             AND COALESCE(x.transporter_id,'')<>'' GROUP BY 1",[])?;
        Ok(rows
            .iter()
            .map(|row| Appearances {
                id: s(row, "id").into(),
                count: n(row, "count") as usize,
                last: s(row, "last").into(),
            })
            .collect())
    }
}
