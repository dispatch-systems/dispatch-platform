//! The keeper, with what Cortex's fixture returns for `shifts`.
use super::*;
use crate::backend::support::station;
use dispatch_core::{manifest::Collector, testing};

#[test]
fn a_week_runs_for_the_dsps_station_on_the_day_it_is_there() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert_eq!(
        Shifts.schedule_ready(&store, &dsp).unwrap_err().code,
        "shifts_station_required"
    );
    station(&store, &dsp);
    let (_, queued) = &Shifts.scheduled(&store, &dsp, "shifts").unwrap()[0];
    let bound = Shifts.bind(&store, &dsp, queued).unwrap();
    let zone = store.find_dsp(&dsp).unwrap().timezone;
    let today = dispatch_core::foundation::clock::today(
        &dispatch_core::foundation::clock::zone(&zone).unwrap(),
    );
    assert_eq!(
        (bound["station"].as_str(), bound["date"].as_str()),
        (Some("TST1"), Some(today.to_string().as_str()))
    );
    let collected = dispatch_cortex::COLLECTOR.fixture(&zone, &bound).unwrap();
    Shifts.publish(&store, &dsp, "job-1", collected).unwrap();
    assert!(Shifts.last_collected(&store, &dsp).unwrap().is_some());
}
