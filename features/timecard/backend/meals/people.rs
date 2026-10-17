//! The drivers Cortex's meal breaks name, as Driver Match reads them.
use dispatch_core::manifest::people::{self, KindInfo, People, Workdays};
use dispatch_cortex as cortex;

/// Every driver a meal-break report names, counted in the days reported.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "meal_breaks",
        order: 30,
        label: "Meal breaks",
        icon: "Utensils",
    },
    cortex::SOURCE,
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
.records("day", |store, dsp| {
    people::records(
        &*store.collector(dsp, cortex::PROVIDER)?,
        "SELECT i.transporter_id id,count(DISTINCT p.report_date) count,\
         max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
         ON p.id=i.publication_id AND p.active=1 GROUP BY i.transporter_id",
    )
})
.driving()
// Every day a meal break was reported was collected, and its drivers were out.
.workdays(|store, dsp| {
    Ok(Workdays::daily(people::worked(
        &*store.collector(dsp, cortex::PROVIDER)?,
        "SELECT DISTINCT i.transporter_id id,p.report_date day FROM meal_itineraries i \
         JOIN meal_publications p ON p.id=i.publication_id AND p.active=1",
    )?))
});
