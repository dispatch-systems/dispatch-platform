//! The drivers DVIC's inspections name, as Driver Match reads them.
use crate::backend::DvicStore;
use dispatch_core::manifest::people::{self, People, PeopleData, Workdays};

/// Every driver a verified inspection names, counted in inspections. DVIC sometimes writes
/// only a last initial, so every other kind's spelling is read first.
pub const PEOPLE: People = People::new(
    PeopleData {
        id: "dvic",
        order: 40,
        label: "DVIC",
        unit: "inspection",
        icon: "ClipboardCheck",
    },
    &dispatch_cortex::PEOPLE,
    |store, dsp| {
        people::named(
            &*store.dvic_db(dsp)?,
            "SELECT transporter_id id,transporter_name name,min(start_date) first,\
             max(start_date) last FROM dvic_inspections WHERE scope_verified=1 \
             GROUP BY 1,2 ORDER BY last DESC",
        )
    },
)
.records(|store, dsp| {
    people::records(
        &*store.dvic_db(dsp)?,
        "SELECT transporter_id id,count(*) count,max(start_date) last \
         FROM dvic_inspections WHERE scope_verified=1 GROUP BY transporter_id",
    )
})
// An inspection means the driver was out that day, though DVIC alone does not say which
// days were collected.
.workdays(|store, dsp| {
    Ok(Workdays {
        worked: store.dvic_db(dsp)?.query_as(
            "SELECT DISTINCT transporter_id,start_date FROM dvic_inspections \
             WHERE scope_verified=1",
            [],
        )?,
        collected: vec![],
    })
})
.initials();
