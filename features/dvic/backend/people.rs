//! The drivers DVIC's inspections name, as Driver Match reads them.
use crate::backend::DvicStore;
use dispatch_core::manifest::people::{self, KindInfo, People, Workdays};

/// Every driver a verified inspection names, counted in inspections, each of which shows them
/// driving. It sometimes writes only a last initial, so it is read last.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "dvic",
        order: 90,
        label: "DVIC",
        icon: "ClipboardCheck",
    },
    dispatch_cortex::SOURCE,
    |store, dsp| {
        people::named(
            &*store.dvic_db(dsp)?,
            "SELECT transporter_id id,transporter_name name,min(start_date) first,\
             max(start_date) last FROM dvic_inspections WHERE scope_verified=1 \
             GROUP BY 1,2 ORDER BY last DESC",
        )
    },
)
.records("inspection", |store, dsp| {
    people::records(
        &*store.dvic_db(dsp)?,
        "SELECT transporter_id id,count(*) count,max(start_date) last \
         FROM dvic_inspections WHERE scope_verified=1 GROUP BY transporter_id",
    )
})
.driving()
// An inspection means the driver was out that day; its reports list only short inspections,
// so the days without one say nothing.
.workdays(|store, dsp| {
    Ok(Workdays {
        worked: people::worked(
            &*store.dvic_db(dsp)?,
            "SELECT DISTINCT transporter_id id,start_date day FROM dvic_inspections \
             WHERE scope_verified=1",
        )?,
        collected: vec![],
    })
});
