//! Live Route Data's settings as its backend reads them, against a real store in a temporary
//! directory: what Settings saves for a DSP, each field by its id, and the schedules it refuses.
use super::*;
use chrono::Weekday::*;
use dispatch_core::{settings, testing};
use serde_json::json;

#[test]
fn reads_what_settings_saves_and_refuses_no_day_or_a_window_past_midnight() {
    testing::install(
        &[&dispatch_cortex::COLLECTOR],
        &[&dispatch_routes::FEATURE, &crate::FEATURE],
    );
    let (_root, store, dsp) = testing::bootstrapped();
    let read = || {
        let collection = collection(&store, &dsp).unwrap();
        (collection.schedule().unwrap(), collection.save_to_history)
    };
    let schedule = |days: &[Weekday], start, stop, minutes: u64| {
        Schedule::new(days, start, stop, Duration::from_secs(minutes * 60)).unwrap()
    };
    // Its defaults, until the DSP saves one: every day from eight until nine, every five
    // minutes, each day saved.
    let every_day = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
    let defaults = (schedule(&every_day, "08:00", "21:00", 5), true);
    assert_eq!(read(), defaults);
    let owner = testing::a_user(&store);
    // No day, or a stop no later than its start, as given or against the start kept, is
    // refused, and nothing is kept.
    for (values, code) in [
        (json!({"days":[]}), "live_route_data_days_required"),
        (
            json!({"start":"09:00","stop":"09:00"}),
            "live_route_data_window_invalid",
        ),
        (
            json!({"start":"22:00","stop":"06:00"}),
            "live_route_data_window_invalid",
        ),
        (json!({"stop":"07:59"}), "live_route_data_window_invalid"),
    ] {
        let refused = settings::save(&store, &dsp, &owner, &COLLECTION, &values, 0, false);
        assert_eq!(
            refused.err().map(|error| error.code).as_deref(),
            Some(code),
            "{values}"
        );
    }
    assert_eq!(read(), defaults);
    let values = json!({
        "days": ["sat", "mon"],
        "start": "05:30",
        "stop": "23:59",
        "every": "60",
        "save_to_history": false,
    });
    settings::save(&store, &dsp, &owner, &COLLECTION, &values, 0, false).unwrap();
    assert_eq!(read(), (schedule(&[Mon, Sat], "05:30", "23:59", 60), false));
}
