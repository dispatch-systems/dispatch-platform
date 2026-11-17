//! Legacy Tracker's settings as its backend reads them, against a real store in a temporary
//! directory: what Settings saves for a DSP, each field by its id.
use super::*;
use dispatch_core::{settings, testing};
use serde_json::json;

static REGISTERED_FEATURE: dispatch_core::manifest::Feature =
    crate::FEATURE.registered(Some("cjf87gqd"), "cjf87gqd_legacy_tracker");

#[test]
fn opens_only_a_secure_web_address_the_dsp_set() {
    // Conduct with it, which it depends on.
    testing::install(&[], &[&dispatch_conduct::FEATURE, &REGISTERED_FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    store.set_custom_id(&dsp, "cjf87gqd").unwrap();
    let owner = testing::a_user(&store);
    store
        .set_feature(&dsp, "cjf87gqd_legacy_tracker", true, &owner)
        .unwrap();
    // None, until the DSP sets one.
    assert_eq!(link(&store, &dsp).unwrap(), None);
    let mut revision = 0;
    for (typed, opened) in [
        (
            "https://tracker.example.com/sheet",
            Some("https://tracker.example.com/sheet"),
        ),
        ("javascript:alert(1)", None),
        ("http://tracker.example.com/sheet", None),
    ] {
        let values = json!({ "link": typed });
        let saved = settings::save(&store, &dsp, &owner, &GENERAL, &values, revision, false);
        let settings::Saved::Saved { revision: next, .. } = saved.unwrap() else {
            panic!("saving {typed} asked to confirm");
        };
        revision = next;
        assert_eq!(link(&store, &dsp).unwrap().as_deref(), opened, "{typed}");
    }
}
