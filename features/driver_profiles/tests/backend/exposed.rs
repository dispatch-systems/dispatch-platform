//! What Driver Profiles exposes, as a feature using it finds it: only while it's on for the
//! DSP, and what the roster it keeps says of the IDs asked for.
use super::*;
use crate::backend::storage::bind_roster_request;
use dispatch_core::{manifest::Collector, testing};
use dispatch_cortex::associates::Capture;
use serde_json::json;

/// Publishes the roster Cortex's fixture collects for the DSP as collected at `when`, less those
/// `gone` names.
fn publish(db: &Store, dsp: &str, job: &str, when: i64, gone: &[&str]) {
    let request = bind_roster_request(db, dsp).unwrap();
    let collected = dispatch_cortex::COLLECTOR
        .fixture("America/Chicago", &request)
        .unwrap();
    let mut capture: Capture = serde_json::from_value(collected.data).unwrap();
    capture
        .associates
        .retain(|a| !gone.contains(&a.transporter_id.as_str()));
    capture.photos.clear();
    (capture.started_at, capture.finished_at) = (when, when);
    db.publish_roster(dsp, job, &capture).unwrap();
}
fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn amazon_roster_is_offered_only_while_driver_profiles_is_on() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    assert!(
        dispatch_driver_profiles_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_some()
    );
    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "driver_profiles", false, &owner)
        .unwrap();
    assert!(
        dispatch_driver_profiles_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}

#[test]
fn amazon_roster_says_what_the_roster_listed_of_the_ids_asked_for_in_their_order() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    let roster = dispatch_driver_profiles_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Driver Profiles is on");
    // Before the first collection, unknown rather than empty.
    assert_eq!(roster.collected(&db, &dsp).unwrap(), None);
    assert!(
        roster
            .associates(&db, &dsp, &ids(&["driver-1"]))
            .unwrap()
            .is_empty()
    );
    let (first, second) = ("2026-10-01T05:00:00.000Z", "2026-10-02T05:00:00.000Z");
    publish(&db, &dsp, "job-1", 1_790_830_800_000, &[]);
    // Amazon stops listing driver-7: what the last roster to list them said stays.
    publish(&db, &dsp, "job-2", 1_790_917_200_000, &["driver-7"]);
    assert_eq!(
        roster.collected(&db, &dsp).unwrap().as_deref(),
        Some(second)
    );
    let asked = ids(&["driver-7", "driver-99", "driver-6", "driver-1"]);
    let said: Vec<String> = roster
        .associates(&db, &dsp, &asked)
        .unwrap()
        .iter()
        .map(|a| {
            format!(
                "{} {} {} [{}] [{}] {} {}",
                a.id,
                a.listed,
                a.status,
                a.roles.join(", "),
                a.qualifications.join(", "),
                a.license_expires.as_deref().unwrap_or("none"),
                a.last_listed
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            format!("driver-7 false ACTIVE [Driver] [CDV, EDV] 2027-07-15 {first}"),
            format!("driver-6 true INACTIVE [Driver] [CDV] 2027-06-15 {second}"),
            format!("driver-1 true ACTIVE [Driver, Helper] [CDV, EDV] 2027-01-15 {second}"),
        ]
    );
    // Any ID the directory could hold is matched as written, up to 128 characters and 32 IDs.
    let (longest, many) = (
        "d".repeat(128),
        (1..=33).map(|n| format!("driver-{n}")).collect::<Vec<_>>(),
    );
    assert!(
        roster
            .associates(&db, &dsp, &ids(&["driver 1", &longest]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(roster.associates(&db, &dsp, &many[..32]).unwrap().len(), 8);
    // An empty ID, a longer one, or one more ID refuses the whole question.
    let longer = "d".repeat(129);
    for refused in [ids(&["driver-1", ""]), ids(&["driver-1", &longer]), many] {
        assert_eq!(
            roster.associates(&db, &dsp, &refused).unwrap_err().code,
            "invalid_input"
        );
    }
}
