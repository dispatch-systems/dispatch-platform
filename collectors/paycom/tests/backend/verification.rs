use super::*;
use dispatch_core::manifest::Collector;

#[test]
fn verification_selects_historical_timecards_and_refuses_unrelated_requests() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let (_root, db, dsp) = dispatch_core::testing::bootstrapped();
    assert_eq!(
        crate::COLLECTOR.verification_request(&db, &dsp, &json!({"date":"2020-09-20"}))?,
        json!({"date":"2020-09-20"})
    );
    for input in [
        json!({"date":"2999-01-01"}),
        json!({"date":false}),
        json!({"date":"2020-09-20","collection":"routes"}),
        json!({"week":"2020-W38"}),
    ] {
        assert!(
            crate::COLLECTOR
                .verification_request(&db, &dsp, &input)
                .is_err()
        );
    }
    Ok(())
}
