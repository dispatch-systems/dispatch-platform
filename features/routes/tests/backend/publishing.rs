use super::*;
use crate::backend::{RoutesStore, shaping::Shaper, tests::request};
use dispatch_cortex::routes::{Mode, fixture};

#[test]
fn publication_merges_skip_identical_shared_rows_but_keep_changes_and_seen_day_bounds() {
    crate::testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = crate::testing::bootstrapped();
    let mut capture = fixture(&request(Mode::Final)).unwrap();
    let shaper = Shaper::new(&capture);
    let itineraries = std::mem::take(&mut capture.itineraries);
    let staged = store
        .stage_routes_start(&dsp, "shared-merge", &capture, itineraries.len())
        .unwrap();
    for itinerary in itineraries {
        store
            .stage_routes_itinerary(&dsp, &staged, &shaper.shape(&itinerary).unwrap())
            .unwrap();
    }
    let db = store.routes_db(&dsp).unwrap();
    db.transaction(|| share(&db, &staged.publication, &staged.day))
        .unwrap();
    assert!(db.count("SELECT count(*) FROM addresses", []).unwrap() > 0);
    assert!(db.count("SELECT count(*) FROM drivers", []).unwrap() > 0);
    db.0.execute_batch(
        "CREATE TEMP TABLE rewritten(table_name TEXT NOT NULL);
         CREATE TEMP TRIGGER rewritten_address AFTER UPDATE ON addresses
         BEGIN INSERT INTO rewritten VALUES ('addresses'); END;
         CREATE TEMP TRIGGER rewritten_driver AFTER UPDATE ON drivers
         BEGIN INSERT INTO rewritten VALUES ('drivers'); END;",
    )
    .unwrap();
    let stage_again = || {
        for (table, key, columns) in [
            ("addresses", "address_id", ADDRESS_COLUMNS),
            ("drivers", "transporter_id", DRIVER_COLUMNS),
        ] {
            let columns = std::iter::once(key)
                .chain(columns.iter().copied())
                .collect::<Vec<_>>()
                .join(",");
            db.exec(&format!(
                "INSERT INTO staged_{table}(publication_id,{columns}) SELECT ?1,{columns} FROM {table}"
            ), [&staged.publication]).unwrap();
        }
    };
    stage_again();
    db.transaction(|| share(&db, &staged.publication, &staged.day))
        .unwrap();
    assert_eq!(db.count("SELECT count(*) FROM rewritten", []).unwrap(), 0);
    stage_again();
    db.exec("UPDATE staged_addresses SET city='Changed city'", [])
        .unwrap();
    db.transaction(|| share(&db, &staged.publication, "2026-09-24"))
        .unwrap();
    assert!(db.count("SELECT count(*) FROM rewritten", []).unwrap() > 0);
    let address = db
        .one(
            "SELECT city,first_seen_day,last_seen_day FROM addresses LIMIT 1",
            [],
        )
        .unwrap()
        .unwrap();
    assert_eq!(s(&address, "city"), "Changed city");
    assert_eq!(s(&address, "first_seen_day"), "2026-09-24");
    assert_eq!(s(&address, "last_seen_day"), staged.day);
    let driver = db
        .one(
            "SELECT first_seen_day,last_seen_day FROM drivers LIMIT 1",
            [],
        )
        .unwrap()
        .unwrap();
    assert_eq!(s(&driver, "first_seen_day"), "2026-09-24");
    assert_eq!(s(&driver, "last_seen_day"), staged.day);
}
