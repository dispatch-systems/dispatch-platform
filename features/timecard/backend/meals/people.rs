//! The drivers Cortex's meal breaks name, as Driver Match reads them.
use dispatch_core::{
    Result,
    db::{Store, n, s},
    manifest::people::{self, Appearances, Named, People, Workdays},
    mcp::api::types::{DriverData, DriverSource},
};
use dispatch_cortex as cortex;

pub struct MealDrivers;
impl People for MealDrivers {
    fn data(&self) -> DriverData {
        DriverData::MealBreaks
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    /// After routes, which spell names most plainly.
    fn order(&self) -> u16 {
        30
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.collector(dsp, cortex::PROVIDER)?.all(
            "SELECT i.transporter_id id,i.driver_name name,min(p.report_date) first,\
             max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id GROUP BY i.transporter_id,i.driver_name \
             ORDER BY last DESC",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut named = Named::new(s(row, "id"));
                named.name = s(row, "name").to_owned();
                named.seen(s(row, "first"), s(row, "last"));
                named
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.collector(dsp, cortex::PROVIDER)?,
            "SELECT i.driver_name FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id WHERE i.transporter_id=? ORDER BY p.report_date DESC",
            id,
        )
    }
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.collector(dsp, cortex::PROVIDER)?.all(
            "SELECT i.transporter_id id,count(DISTINCT p.report_date) count,\
             max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id AND p.active=1 GROUP BY i.transporter_id",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| Appearances {
                id: s(row, "id").to_owned(),
                count: n(row, "count") as usize,
                last: s(row, "last").to_owned(),
            })
            .collect())
    }
    /// Every day a meal break was reported was collected, and its drivers were out.
    fn workdays(&self, store: &Store, dsp: &str) -> Result<Workdays> {
        let worked = store
            .collector(dsp, cortex::PROVIDER)?
            .query_as::<(String, String)>(
                "SELECT DISTINCT i.transporter_id,p.report_date FROM meal_itineraries i \
                 JOIN meal_publications p ON p.id=i.publication_id AND p.active=1",
                [],
            )?;
        let collected = worked.iter().map(|(_, day)| day.clone()).collect();
        Ok(Workdays { worked, collected })
    }
}
