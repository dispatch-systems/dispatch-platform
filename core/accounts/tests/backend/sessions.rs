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
