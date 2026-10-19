//! Routes' settings: a window that passes a stored day is confirmed with words that name the
//! first day the expiry after the save keeps.
use super::*;
use crate::backend::tests::publish;
use chrono::TimeZone;
use chrono_tz::America::Chicago;
use dispatch_core::testing;
use dispatch_cortex::routes::Mode;

#[test]
fn a_window_saved_in_the_hour_before_midnight_names_the_day_the_expiry_after_it_keeps() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    db.keep_dsp_profile(&dsp, "Dev DSP", "America/Chicago", None, None)
        .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    // Its route data holds Sep 25: the first day 30 days keep on Oct 25 where the DSP is.
    publish(&db, &dsp, "2026-09-25", Mode::Final);
    let window = json!({"days": 30});
    let saved_at = |hour, minute| {
        let at = Chicago.with_ymd_and_hms(2026, 10, 25, hour, minute, 0);
        deleted(&db, &dsp, &window, at.unwrap().timestamp_millis()).unwrap()
    };
    // Saved at 22:58, the expiry after it runs by 23:59 on Oct 25 and keeps Sep 25.
    assert_eq!(saved_at(22, 58), None);
    // Saved at 23:30, it may run on Oct 26 and delete Sep 25, so the words name Sep 26.
    assert_eq!(
        saved_at(23, 30).as_deref(),
        Some(
            "Route data from before Sep 26, 2026 will be deleted within the hour. This can’t be undone."
        )
    );
}
