use super::*;
#[test]
fn policy_accepts_partial_tuning_but_rejects_disabled_controls_and_typos() {
    assert_eq!(
        SecurityPolicy::parse(r#"{"mailActorHourly":120}"#)
            .unwrap()
            .mail_actor_hourly,
        120
    );
    for value in [
        r#"{"mailActorHourly":0}"#,
        r#"{"freshAuthSeconds":86400}"#,
        r#"{"disableMfa":true}"#,
        r#"{"mailActorHourly":"120"}"#,
    ] {
        assert!(SecurityPolicy::parse(value).is_err());
    }
}
