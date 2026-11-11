//! Every kind of data a feature connects to Driver Match reads from the storage its feature
//! keeps, so a new connection is tried here without a test of its own.
use dispatch_core::{
    manifest::{people, registry},
    testing as common,
};

#[test]
fn every_connection_reads_its_people_from_a_new_dsp() {
    crate::install();
    let (_root, db, id) = common::bootstrapped();
    db.enable_all_features(&id).unwrap();
    let actor = common::platform_owner(&db);
    let mut owners = std::collections::BTreeMap::new();
    for feature in registry().features {
        if let Some(code) = feature.owner
            && !owners.contains_key(code)
        {
            let dsp = db.new_dsp("People schema", "UTC", &actor, false).unwrap();
            db.set_code(&dsp.id, code).unwrap();
            db.enable_all_features(&dsp.id).unwrap();
            owners.insert(code, dsp.id);
        }
    }
    for feature in registry().features {
        let id = feature
            .owner
            .and_then(|code| owners.get(code))
            .unwrap_or(&id);
        for people in feature.people {
            let kind = people.kind.id;
            let named = (people.read)(&db, id).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
            assert!(named.is_empty(), "{kind} names someone a new DSP lacks");
            if let Some((_, records)) = people.records {
                assert!(
                    records(&db, id).unwrap().is_empty(),
                    "{kind} counts records"
                );
            }
            if let Some(workdays) = people.workdays {
                assert!(
                    workdays(&db, id).unwrap().worked.is_empty(),
                    "{kind} has days"
                );
            }
            if let Some(departments) = people.driver_departments {
                departments(&db, id).unwrap();
            }
            if let Some(photos) = people.photos {
                let none = photos(&db, id, &["nobody".into()]).unwrap();
                assert!(none.is_none(), "{kind} has a photo of nobody");
            }
        }
    }
}

#[test]
fn a_people_query_with_a_misspelt_column_is_refused() {
    crate::install();
    let (_root, db, id) = common::bootstrapped();
    let dsp = db.dsp(&id).unwrap();
    let read = |sql: &str| people::named(&dsp, sql).map_err(|e| e.code);
    assert_eq!(
        read("SELECT 'T1' id,'Jordan Ellis' name,'2026-09-01' last")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read("SELECT 'T1' id,'Jordan Ellis' name,'2026-09-01' fist").unwrap_err(),
        "people_columns"
    );
    assert_eq!(
        read("SELECT 'T1' id,'2026-09-01' last").unwrap_err(),
        "people_columns"
    );
}
