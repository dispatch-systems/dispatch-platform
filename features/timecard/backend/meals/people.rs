//! The drivers Cortex's meal breaks name, as Driver Match reads them.
use dispatch_core::manifest::people::{self, People, PeopleData, Workdays};
use dispatch_cortex as cortex;

/// Every driver a meal-break report names, counted in the days reported. After routes, which
/// spell names most plainly.
pub const PEOPLE: People = People::new(
    PeopleData {
        id: "meal_breaks",
        order: 30,
        label: "Meal breaks",
        unit: "day",
        icon: "Utensils",
    },
    &cortex::PEOPLE,
    |store, dsp| {
        people::named(
            &*store.collector(dsp, cortex::PROVIDER)?,
            "SELECT i.transporter_id id,i.driver_name name,min(p.report_date) first,\
             max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id GROUP BY i.transporter_id,i.driver_name \
             ORDER BY last DESC",
        )
    },
)
.records(|store, dsp| {
    people::records(
        &*store.collector(dsp, cortex::PROVIDER)?,
        "SELECT i.transporter_id id,count(DISTINCT p.report_date) count,\
         max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
         ON p.id=i.publication_id AND p.active=1 GROUP BY i.transporter_id",
    )
})
// Every day a meal break was reported was collected, and its drivers were out.
.workdays(|store, dsp| {
    Ok(Workdays::daily(
        store.collector(dsp, cortex::PROVIDER)?.query_as(
            "SELECT DISTINCT i.transporter_id,p.report_date FROM meal_itineraries i \
             JOIN meal_publications p ON p.id=i.publication_id AND p.active=1",
            [],
        )?,
    ))
});
