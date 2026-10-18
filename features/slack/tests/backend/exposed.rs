//! What Slack exposes, as a feature using it finds it: only while it's on for the
//! DSP.
use dispatch_core::testing;

#[test]
fn chat_accounts_is_offered_only_while_slack_is_on() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let offered = dispatch_slack_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Slack is on");
    // TODO(scaffold): prove each of its readers and actions against what it keeps.
    assert!(offered.ready(&db, &dsp).unwrap());
    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "slack", false, &owner).unwrap();
    assert!(
        dispatch_slack_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}
