//! What the development fixtures start with: each DSP under the same ID whenever they start
//! afresh at the same address, and its drivers written up in Conduct, signed and not.
use dispatch_core::{
    db::Store,
    server::{operations, startup},
    testing,
};

/// The fixtures' DSPs, by name, with their IDs.
fn dsps(db: &Store) -> Vec<(String, String)> {
    db.platform
        .query_as("SELECT name,id FROM dsps ORDER BY name", [])
        .unwrap()
}

#[test]
fn the_fixtures_keep_their_dsp_ids_at_one_address_and_others_at_another() {
    crate::install();
    let (_first, first) = testing::seeded();
    let (_again, again) = testing::seeded();
    assert_eq!(dsps(&first), dsps(&again));
    // Fixtures at another address, as a test server's, keep their DSPs apart from these.
    let (_other, store) = testing::store();
    let mut config = store.config.clone();
    config.origin = "http://127.0.0.1:5174".into();
    drop(store);
    let other = startup::prepare_storage(config).unwrap();
    operations::seed(&other).unwrap();
    let theirs = dsps(&other);
    assert_eq!(theirs.len(), 3);
    for (name, id) in dsps(&first) {
        assert!(!theirs.iter().any(|(_, other)| *other == id), "{name}");
    }
}

/// A writeup as the test reads it: who it's for, its category, its tier, and when it was signed.
type Writeup = (String, String, String, Option<String>);

#[test]
fn the_fixtures_write_drivers_up_in_several_categories_signed_and_not() {
    crate::install();
    let (_root, db) = testing::seeded();
    for name in ["Dev DSP", "Northline Logistics"] {
        let dsp = testing::dsp_named(&db, name);
        let writeups: Vec<Writeup> = db
            .dsp(&dsp)
            .unwrap()
            .query_as(
                "SELECT person,category_id,tier,signed_on FROM conduct_writeups",
                [],
            )
            .unwrap();
        let distinct = |of: fn(&Writeup) -> &str| {
            let mut values: Vec<&str> = writeups.iter().map(of).collect();
            values.sort_unstable();
            values.dedup();
            values.len()
        };
        assert!(distinct(|w| &w.0) >= 3, "{name}: {writeups:?}");
        assert!(distinct(|w| &w.1) >= 4, "{name}: {writeups:?}");
        assert_eq!(distinct(|w| &w.2), 3, "{name}: {writeups:?}");
        assert!(
            writeups.iter().any(|w| w.3.is_some()),
            "{name}: {writeups:?}"
        );
        assert!(
            writeups.iter().any(|w| w.3.is_none()),
            "{name}: {writeups:?}"
        );
    }
}
