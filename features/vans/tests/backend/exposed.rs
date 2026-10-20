//! What Vans exposes, as a feature using it finds it: only while it's on for the DSP, and what
//! the fleet it keeps says of the VINs asked for.
use super::*;
use crate::backend::support::fleet;
use dispatch_core::testing;
use serde_json::json;

fn vins(vins: &[&str]) -> Vec<String> {
    vins.iter().map(|vin| vin.to_string()).collect()
}

#[test]
fn amazon_fleet_is_offered_only_while_vans_is_on() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    assert!(dispatch_vans_contract::for_dsp(&db, &dsp).unwrap().is_some());
    db.set_feature(&dsp, "vans", false, &testing::a_user(&db))
        .unwrap();
    assert!(dispatch_vans_contract::for_dsp(&db, &dsp).unwrap().is_none());
}

#[test]
fn amazon_fleet_names_the_vans_asked_for_by_vin_in_their_order() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(&dsp, json!({"abbreviation":"FXTR"})).unwrap();
    let fleet_of = dispatch_vans_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Vans is on");
    // Before the first collection, unknown rather than empty.
    assert_eq!(fleet_of.collected(&db, &dsp).unwrap(), None);
    db.publish_fleet(&dsp, "job-1", &fleet(&db, &dsp, 1_790_830_800_000))
        .unwrap();
    let mut next = fleet(&db, &dsp, 1_790_917_200_000);
    next.vans.retain(|van| van.name.as_deref() != Some("214"));
    db.publish_fleet(&dsp, "job-2", &next).unwrap();
    assert_eq!(
        fleet_of.collected(&db, &dsp).unwrap().as_deref(),
        Some("2026-10-02T05:00:00.000Z")
    );
    // A VIN reads in capitals, without the spaces around it; one never listed is left out.
    let asked = vins(&[
        "7fctgaaa1nn000214",
        "1FTBW3XM5LKA00999",
        " 3C6LRVDG0ME000301 ",
    ]);
    let said: Vec<String> = fleet_of
        .vans(&db, &dsp, &asked)
        .unwrap()
        .iter()
        .map(|van| {
            format!(
                "{} {} {} {}",
                van.vin,
                van.name.as_deref().unwrap_or("none"),
                van.listed,
                van.last_listed
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            "7FCTGAAA1NN000214 214 false 2026-10-01T05:00:00.000Z",
            "3C6LRVDG0ME000301 Rental 301 true 2026-10-02T05:00:00.000Z",
        ]
    );
    // Up to a hundred at once; one more refuses the whole question.
    let many: Vec<String> = (0..101).map(|n| format!("1FTBW3XM5LKA{n:05}")).collect();
    assert!(fleet_of.vans(&db, &dsp, &many[..100]).is_ok());
    assert_eq!(
        fleet_of.vans(&db, &dsp, &many).unwrap_err().code,
        "invalid_input"
    );
}
