//! The drivers Daily Performance names, as Driver Match reads them.
use crate::DailyPerformanceStore;
use dispatch_core::manifest::people::{self, KindInfo, People};

/// Every driver a current day's rows name, counted in the days their quality was reported.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "daily_performance",
        order: 60,
        label: "Daily Performance",
        icon: "CalendarDays",
    },
    dispatch_cortex::SOURCE,
    |store, dsp| {
        people::named(
            &*store.daily_performance_db(dsp)?,
            "SELECT x.transporter_id id,json_extract(x.row,'$.da_name') name,min(x.date) first,\
             max(x.date) last FROM daily_rows x JOIN daily_publications p \
             ON p.id=x.publication_id WHERE p.active=1 AND COALESCE(x.transporter_id,'')<>'' \
             AND COALESCE(json_extract(x.row,'$.da_name'),'')<>'' GROUP BY 1,2 \
             ORDER BY last DESC",
        )
    },
)
.records("day", |store, dsp| {
    people::records(
        &*store.daily_performance_db(dsp)?,
        "SELECT x.transporter_id id,count(DISTINCT x.date) count,max(x.date) last \
         FROM daily_rows x JOIN daily_publications p ON p.id=x.publication_id \
         WHERE p.active=1 AND x.dataset='driver_quality' \
         AND COALESCE(x.transporter_id,'')<>'' GROUP BY 1",
    )
})
.driving();
