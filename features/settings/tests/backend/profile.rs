//! The DSP's profile on Settings: the DSP's own, kept where its onboarding keeps it, its short
//! code given once, and each save recorded as a change of Settings.
use super::PROFILE;
use dispatch_core::{db::Store, settings, testing};
use serde_json::{Value, json};

fn dsp(db: &Store, name: &str) -> String {
    let dsps = db.served_dsps().unwrap();
    dsps.into_iter().find(|dsp| dsp.name == name).unwrap().id
}

#[test]
fn the_profile_is_the_dsps_own_its_short_code_given_once_and_a_save_reads_as_a_change() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db) = testing::seeded();
    let actor = testing::platform_owner(&db);
    let (north, summit, dev) = (
        dsp(&db, "Northline Logistics"),
        dsp(&db, "Summit Delivery"),
        dsp(&db, "Dev DSP"),
    );
    let save = |dsp: &str, values: Value, revision| {
        settings::save(&db, dsp, &actor, &PROFILE, &values, revision, false)
    };
    let shown = settings::read::<Value>(&db, &north, &PROFILE).unwrap();
    assert_eq!(
        shown,
        json!({"name": "Northline Logistics", "abbreviation": "NLL", "station_code": "",
            "timezone": "America/Chicago"})
    );
    // The short code names the DSP's address: once given, it stays.
    let locked = save(&north, json!({"abbreviation": "OTHER"}), 0).unwrap_err();
    assert_eq!(locked.code, "invalid_input");
    let changed = json!({"name": "Northline Freight", "station_code": "xyz9",
        "timezone": "Asia/Tokyo"});
    save(&north, changed, 0).unwrap();
    let kept = db.find_dsp(&north).unwrap();
    assert_eq!(
        (
            kept.name.as_str(),
            kept.timezone.as_str(),
            kept.code.as_deref()
        ),
        ("Northline Freight", "Asia/Tokyo", Some("nll"))
    );
    assert_eq!(db.profile(&north).unwrap().station_code, "XYZ9");
    assert_eq!(
        testing::audit_actions(&db, &north, "settings."),
        ["settings.changed"]
    );
    // Not as onboarding records the DSP being set up.
    let setup = ["dsp.profile_completed", "dsp.settings_updated"];
    let logged = testing::audit_actions(&db, &north, "dsp.");
    assert!(!logged.iter().any(|action| setup.contains(&action.as_str())));
    assert_eq!(db.find_dsp(&summit).unwrap().name, "Summit Delivery");
    // One without a short code yet is given the one saved.
    save(
        &dev,
        json!({"abbreviation": "devq7", "station_code": "DEV1"}),
        0,
    )
    .unwrap();
    assert_eq!(db.find_dsp(&dev).unwrap().code.as_deref(), Some("devq7"));
}
