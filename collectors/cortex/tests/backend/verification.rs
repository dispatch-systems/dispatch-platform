use super::*;
use dispatch_core::manifest::Collector;

#[test]
fn verification_binds_each_collection_to_the_selected_account_scope() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let (_root, db, dsp) = dispatch_core::testing::bootstrapped();
    db.set_profile(&dsp, json!({"abbreviation":"TEST","stationCode":"TST1"}))?;
    let account = db.find_dsp(&dsp)?;
    let routes = crate::COLLECTOR.verification_request(
        &db,
        &dsp,
        &json!({"collection":"routes","date":"2020-09-20"}),
    )?;
    assert_eq!(routes["station"], "TST1");
    assert_eq!(routes["dspAbbreviation"], "TEST");
    assert_eq!(routes["dspName"], account.name);
    assert_eq!(routes["timezone"], account.timezone);
    assert_eq!(routes["mode"], "final");
    assert_eq!(
        crate::COLLECTOR.job_kind_for(&routes),
        crate::routes::JOB_KIND
    );
    let scorecard = crate::COLLECTOR.verification_request(
        &db,
        &dsp,
        &json!({"collection":"weekly_scorecard","week":"2020-W38"}),
    )?;
    assert_eq!(scorecard["week"], "2020-W38");
    assert_eq!(scorecard["station"], "TST1");
    assert!(scorecard.get("date").is_none());
    let dvic = crate::COLLECTOR.verification_request(
        &db,
        &dsp,
        &json!({"collection":"dvic","week":"2020-W38"}),
    )?;
    assert_eq!(dvic["weeks"], json!(["2020-W38"]));
    for input in [
        json!({"collection":"routes","week":"2020-W38"}),
        json!({"collection":"unrelated"}),
        json!({"date":"2999-01-01"}),
        json!({"station":"TST2"}),
        json!({"date":false}),
    ] {
        assert!(
            crate::COLLECTOR
                .verification_request(&db, &dsp, &input)
                .is_err()
        );
    }
    Ok(())
}
