//! The drivers Amazon's routes name, as Driver Match reads them.
use crate::backend::RoutesStore;
use dispatch_core::manifest::people::{self, People, PeopleData, Workdays};

/// Every driver the routes name, counted in the days they drove. Routes spell names most
/// plainly, so theirs is read first among Amazon's.
pub const PEOPLE: People = People::new(
    PeopleData {
        id: "routes",
        order: 20,
        label: "Routes",
        unit: "day",
        icon: "Route",
    },
    &dispatch_cortex::PEOPLE,
    |store, dsp| {
        people::named(
            &*store.routes_db(dsp)?,
            "SELECT transporter_id id,COALESCE(first_name,'')||' '||COALESCE(last_name,'') name,\
             first_seen_day first,last_seen_day last FROM drivers",
        )
    },
)
.records(|store, dsp| {
    people::records(
        &*store.routes_db(dsp)?,
        "SELECT i.transporter_id id,count(DISTINCT i.day) count,max(i.day) last \
         FROM itineraries i JOIN route_publications p ON p.id=i.publication_id \
         AND p.active=1 GROUP BY i.transporter_id",
    )
})
// Every day with a route was collected, and its drivers were out.
.workdays(|store, dsp| {
    Ok(Workdays::daily(store.routes_db(dsp)?.query_as(
        "SELECT DISTINCT i.transporter_id,i.day FROM itineraries i \
         JOIN route_publications p ON p.id=i.publication_id AND p.active=1",
        [],
    )?))
});
