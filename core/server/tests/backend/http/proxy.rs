use super::*;
#[test]
fn trust_is_explicit_and_headers_are_single_addresses() {
    let mut headers = HeaderMap::new();
    headers.insert("cf-connecting-ip", "203.0.113.5".parse().unwrap());
    for (mode, peer, expected) in [
        (TrustedProxy::None, "127.0.0.1", "127.0.0.1"),
        (TrustedProxy::Cloudflare, "192.0.2.1", "192.0.2.1"),
        (TrustedProxy::Cloudflare, "127.0.0.1", "203.0.113.5"),
    ] {
        assert_eq!(
            mode.client_ip(peer.parse().unwrap(), &headers).unwrap(),
            expected
        );
    }
    let peer = "127.0.0.1".parse().unwrap();
    for value in ["", "unknown", "203.0.113.5, 203.0.113.6", "203.0.113.5:80"] {
        headers.insert("cf-connecting-ip", value.parse().unwrap());
        assert!(TrustedProxy::Cloudflare.client_ip(peer, &headers).is_err());
    }
    headers.insert("cf-connecting-ip", "::ffff:203.0.113.5".parse().unwrap());
    assert_eq!(
        TrustedProxy::Cloudflare.client_ip(peer, &headers).unwrap(),
        "203.0.113.5"
    );
    headers.append("cf-connecting-ip", "203.0.113.6".parse().unwrap());
    assert!(TrustedProxy::Cloudflare.client_ip(peer, &headers).is_err());
}
