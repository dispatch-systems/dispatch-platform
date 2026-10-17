//! The drivers Amazon's routes name, as Driver Match reads them.
use crate::backend::RoutesStore;
use dispatch_core::manifest::people::{self, KindInfo, People, Workdays};

/// Every driver the routes name, counted in the days they drove.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "routes",
        order: 20,
        label: "Routes",
        icon: "Route",
    },
    dispatch_cortex::SOURCE,
    |store, dsp| {
        people::named(
            &*store.routes_db(dsp)?,
            "SELECT transporter_id id,COALESCE(first_name,'')||' '||COALESCE(last_name,'') name,\
             first_seen_day first,last_seen_day last FROM drivers",
        )
    },
)
.records("day", |store, dsp| {
    people::records(
        &*store.routes_db(dsp)?,
        "SELECT i.transporter_id id,count(DISTINCT i.day) count,max(i.day) last \
         FROM itineraries i JOIN route_publications p ON p.id=i.publication_id \
         AND p.active=1 GROUP BY i.transporter_id",
    )
})
.driving()
// Every day with a route was collected, and its drivers were out.
.workdays(|store, dsp| {
    Ok(Workdays::daily(people::worked(
        &*store.routes_db(dsp)?,
        "SELECT DISTINCT i.transporter_id id,i.day day FROM itineraries i \
         JOIN route_publications p ON p.id=i.publication_id AND p.active=1",
    )?))
});
