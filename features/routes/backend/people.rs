//! The drivers Amazon's routes name, as Driver Match reads them.
use crate::backend::RoutesStore;
use dispatch_core::{
    Result,
    db::{Store, n, s},
    manifest::people::{self, Appearances, Named, People, Workdays},
    mcp::api::types::{DriverData, DriverSource, PeopleData},
};

/// What it names people in, as Driver Match lists it.
const DATA: PeopleData = PeopleData {
    id: "routes",
    order: 20,
};
pub struct Drivers;
impl People for Drivers {
    fn data(&self) -> DriverData {
        DriverData::new(&DATA)
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    /// Amazon's first: routes spell names most plainly.
    fn order(&self) -> u16 {
        20
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let rows = store.routes_db(dsp)?.all(
            "SELECT transporter_id id,first_name,last_name,first_seen_day first,\
             last_seen_day last FROM drivers",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut named = Named::new(s(row, "id"));
                named.name = format!("{} {}", s(row, "first_name"), s(row, "last_name"));
                named.seen(s(row, "first"), s(row, "last"));
                named
            })
            .collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.routes_db(dsp)?,
            "SELECT first_name||' '||last_name FROM drivers WHERE transporter_id=?",
            id,
        )
    }
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.routes_db(dsp)?.all(
            "SELECT i.transporter_id id,count(DISTINCT i.day) count,max(i.day) last \
             FROM itineraries i JOIN route_publications p ON p.id=i.publication_id \
             AND p.active=1 GROUP BY i.transporter_id",
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
    /// Every day with a route was collected, and its drivers were out.
    fn workdays(&self, store: &Store, dsp: &str) -> Result<Workdays> {
        let worked = store.routes_db(dsp)?.query_as::<(String, String)>(
            "SELECT DISTINCT i.transporter_id,i.day FROM itineraries i \
             JOIN route_publications p ON p.id=i.publication_id AND p.active=1",
            [],
        )?;
        let collected = worked.iter().map(|(_, day)| day.clone()).collect();
        Ok(Workdays { worked, collected })
    }
}
