//! Live Route Data's settings as its backend reads them, against a real store in a temporary
//! directory: what Settings saves for a DSP, each field by its id, and a window it refuses.
use super::*;
use dispatch_core::{settings, testing};
use serde_json::json;

#[test]
fn reads_what_settings_saves_and_refuses_a_window_past_midnight() {
    testing::install(
        &[&dispatch_cortex::COLLECTOR],
        &[&dispatch_routes::FEATURE, &crate::FEATURE],
    );
    let (_root, store, dsp) = testing::bootstrapped();
    let read = || {
        let collection = collection(&store, &dsp).unwrap();
        let window = collection.window().unwrap();
        (window, collection.save_to_history)
    };
    let window = |start, stop| Window::parse(start, stop).unwrap();
    // Its defaults, until the DSP saves one: from eight until nine, each day saved.
    assert_eq!(read(), (window("08:00", "21:00"), true));
    let owner = testing::a_user(&store);
    // A stop no later than its start is refused, as given or against the start kept, and
    // nothing is kept.
    for values in [
        json!({"start":"09:00","stop":"09:00"}),
        json!({"start":"22:00","stop":"06:00"}),
        json!({"stop":"07:59"}),
    ] {
        let refused = settings::save(&store, &dsp, &owner, &COLLECTION, &values, 0, false);
        assert_eq!(
            refused.err().map(|error| error.code).as_deref(),
            Some("live_route_data_window_invalid"),
            "{values}"
        );
    }
    assert_eq!(read(), (window("08:00", "21:00"), true));
    let values = json!({"start":"05:30","stop":"23:59","save_to_history":false});
    settings::save(&store, &dsp, &owner, &COLLECTION, &values, 0, false).unwrap();
    assert_eq!(read(), (window("05:30", "23:59"), false));
}
