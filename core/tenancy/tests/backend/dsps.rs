//! Each DSP is made with a random custom ID of its own, which owns its custom features and never
//! names it; in Production it stays the one it was made with.
use crate::{foundation::config::Environment, testing};

fn random_id(id: &str) -> bool {
    id.len() == 8
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && !id.contains(['l', 'o', '0', '1'])
}

#[test]
fn a_new_dsp_is_made_with_a_random_custom_id_of_its_own() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let owner = testing::platform_owner(&db);
    let first = db.new_dsp("First", "UTC", &owner, false).unwrap();
    let second = db.new_dsp("Second", "UTC", &owner, false).unwrap();
    let (first, second) = (first.custom_id.unwrap(), second.custom_id.unwrap());
    assert!(random_id(&first) && random_id(&second), "{first} {second}");
    assert_ne!(first, second);
}

#[test]
fn production_never_changes_a_dsps_custom_id() {
    testing::install(&[], &[]);
    let (_root, mut db) = testing::seeded();
    let owner = testing::platform_owner(&db);
    let dsp = db.new_dsp("Made", "UTC", &owner, false).unwrap();
    db.config.environment = Environment::Production;
    let refused = db.set_custom_id(&dsp.id, "chosen").unwrap_err();
    assert_eq!(refused.code, "custom_id_fixed");
    assert_eq!(db.find_dsp(&dsp.id).unwrap().custom_id, dsp.custom_id);
    // Off Production, a demo, test or Dev DSP may take a known one.
    db.config.environment = Environment::Preview;
    assert_eq!(db.set_custom_id(&dsp.id, "chosen").unwrap(), "chosen");
}
