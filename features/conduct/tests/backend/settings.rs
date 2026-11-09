//! Conduct's settings as its backend reads them, against a real store in a temporary
//! directory: what Settings saves for a DSP, each field by its id.
use super::*;
use dispatch_core::{settings, testing};
use serde_json::json;

#[test]
fn reads_what_settings_saves() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    // A writeup counts for a year until the DSP chooses otherwise.
    assert_eq!(general(&store, &dsp).unwrap().months, 12);
    let owner = testing::a_user(&store);
    settings::save(
        &store,
        &dsp,
        &owner,
        &GENERAL,
        &json!({"months": 6}),
        0,
        false,
    )
    .unwrap();
    assert_eq!(general(&store, &dsp).unwrap().months, 6);
}
