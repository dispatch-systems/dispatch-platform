use super::{device_label, quota_ip};

#[test]
fn user_agents_become_bounded_non_identifying_device_labels() {
    assert_eq!(
        device_label("Mozilla/5.0 (Macintosh) AppleWebKit Safari/605.1"),
        "Safari on macOS"
    );
    assert_eq!(
        device_label("arbitrary private detail"),
        "Browser on unknown device"
    );
}

#[test]
fn ipv6_throttles_share_their_network_prefix() {
    assert_eq!(
        quota_ip("2001:db8:12:34::1"),
        quota_ip("2001:db8:12:34:ffff::2")
    );
    assert_ne!(quota_ip("2001:db8:12:34::1"), quota_ip("2001:db8:12:35::1"));
    assert_eq!(quota_ip("::ffff:192.0.2.7"), "192.0.2.7");
    assert_eq!(quota_ip("not-an-address"), "invalid");
}

#[tokio::test]
async fn a_dsp_disabled_while_its_member_signs_in_gives_them_no_session() {
    crate::testing::install(&[], &[]);
    let (_root, db) = crate::testing::seeded();
    let north = db.dsp_at("nll").unwrap().unwrap().id;
    // Disabled as the first turn counts the attempt, after it found the DSP active: as the
    // platform owner disabling it while the password is checked.
    db.platform
        .exec(
            &format!(
                "CREATE TRIGGER disabling AFTER INSERT ON throttle \
                 BEGIN UPDATE dsps SET status='suspended' WHERE id='{north}'; END"
            ),
            [],
        )
        .unwrap();
    let config = db.config.clone();
    drop(db);
    let state = crate::State::new(config).unwrap();
    let refused = state
        .login(
            "member@dispatch.test".into(),
            "Dispatch-demo-2026!".into(),
            "127.0.0.1".into(),
            String::new(),
            crate::accounts::SessionLifetime::Standard,
            crate::foundation::config::Site::Dsp("nll".into()),
        )
        .await;
    assert_eq!(refused.unwrap_err().code, "dsp_disabled");
    let sessions = state
        .read(move |db| db.dsp(&north)?.count("SELECT count(*) FROM sessions", []))
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}
