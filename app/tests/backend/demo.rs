//! What the development fixtures start with: each DSP and each of its drivers under the same
//! ID and code whenever they start afresh at the same address, and its drivers written up in
//! Conduct, signed and not.
use dispatch_core::{
    db::Store,
    manifest::{people, registry},
    server::{operations, startup},
    testing,
};
use dispatch_driver_match::DriverMatchStore;

/// The fixtures' DSPs, by name, with their IDs.
fn dsps(db: &Store) -> Vec<(String, String)> {
    db.platform
        .query_as("SELECT name,id FROM dsps ORDER BY name", [])
        .unwrap()
}

/// Every ID the DSP `name`'s people hold, with its source and its holder's code: a member's
/// account, drawn afresh each time, by its email address.
fn codes(db: &Store, name: &str) -> Vec<(String, String, String)> {
    let dsp = testing::dsp_named(db, name);
    let members = db.members(&dsp).unwrap();
    let mut codes: Vec<(String, String, String)> = db
        .dsp(&dsp)
        .unwrap()
        .query_as("SELECT source,external_id,code FROM person_ids", [])
        .unwrap();
    for (source, id, _) in &mut codes {
        if *source == people::MEMBERS.as_str()
            && let Some(member) = members.iter().find(|m| m.user_id == *id)
        {
            id.clone_from(&member.email);
        }
    }
    codes.sort();
    codes
}

#[test]
fn the_fixtures_keep_their_dsp_ids_and_codes_at_one_address_and_others_at_another() {
    crate::install();
    let (_first, first) = testing::seeded();
    let (_again, again) = testing::seeded();
    assert_eq!(dsps(&first), dsps(&again));
    let northline = codes(&first, "Northline Logistics");
    assert!(!northline.is_empty());
    assert_eq!(northline, codes(&again, "Northline Logistics"));
    // A real pass draws its codes at random, even at a DSP whose ID holds: Summit has no demo
    // data, so the same timecards reach it in each, as a collection would bring them.
    let timecards = registry()
        .features
        .iter()
        .find(|f| f.switch.id == "timecard")
        .and_then(|f| f.demo)
        .unwrap();
    let collected = |db: &Store| {
        let summit = testing::dsp_named(db, "Summit Delivery");
        timecards(db, &summit, "America/Denver").unwrap();
        db.write_pass(&summit, db.read_pass(&summit).unwrap())
            .unwrap();
        codes(db, "Summit Delivery")
    };
    let (summit, summit_again) = (collected(&first), collected(&again));
    assert!(!summit.is_empty());
    assert_eq!(summit.len(), summit_again.len());
    for ((source, id, code), (.., again)) in summit.iter().zip(&summit_again) {
        assert_ne!(code, again, "{source} {id}");
    }
    // Fixtures at another address, as a test server's, keep their DSPs and people apart from
    // these.
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
    let theirs = codes(&other, "Northline Logistics");
    for (source, id, code) in &northline {
        assert!(
            !theirs.iter().any(|(.., other)| other == code),
            "{source} {id}"
        );
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
