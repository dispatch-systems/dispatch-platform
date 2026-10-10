use super::*;

#[test]
fn only_public_addresses_are_public() {
    // IPv4 by its octets: private, loopback, unspecified, link-local and metadata,
    // shared, multicast, broadcast, documentation and benchmarking.
    for octets in [
        [10, 0, 0, 1],
        [172, 16, 0, 1],
        [192, 168, 1, 1],
        [127, 0, 0, 1],
        [0, 0, 0, 0],
        [169, 254, 169, 254],
        [169, 254, 0, 1],
        [100, 64, 0, 1],
        [100, 127, 255, 254],
        [224, 0, 0, 1],
        [239, 255, 255, 250],
        [255, 255, 255, 255],
        [192, 0, 2, 1],
        [198, 51, 100, 7],
        [203, 0, 113, 9],
        [198, 18, 0, 1],
    ] {
        assert!(!public(IpAddr::from(octets)), "{octets:?}");
    }
    // IPv6: unspecified, loopback, unique-local (a cloud's metadata among them),
    // link-local, multicast, IPv4-mapped, NAT64, documentation, Teredo and 6to4.
    for address in [
        "::",
        "::1",
        "fc00::1",
        "fd00:ec2::254",
        "fe80::1",
        "ff02::1",
        "::ffff:808:808",
        "::ffff:7f00:1",
        "64:ff9b::a00:1",
        "2001:db8::1",
        "2001::1",
        "2002:7f00:1::1",
        "3fff::1",
    ] {
        assert!(!public(address.parse().unwrap()), "{address}");
    }
    assert!(public(IpAddr::from([8, 8, 8, 8])));
    assert!(public(IpAddr::from([93, 184, 215, 14])));
    for address in ["2606:4700::6810:84e5", "2a00:1450::1"] {
        assert!(public(address.parse().unwrap()), "{address}");
    }
}

#[test]
fn a_website_client_id_is_a_plain_https_url_with_a_path() {
    for id in [
        "https://app.example.com/oauth/client.json",
        "https://xn--bcher-kva.example/client",
        FIXTURE_APP,
    ] {
        assert!(web_client_id(id).is_some(), "{id}");
    }
    for id in [
        "http://app.example.com/client.json",
        "https://app.example.com",
        "https://app.example.com/",
        "https://app.example.com:443/client.json",
        "https://app.example.com:8443/client.json",
        "https://APP.example.com/client.json",
        "https://app.example.com/a/../client.json",
        "https://app.example.com/client.json?x=1",
        "https://app.example.com/client.json#x",
        "https://user@app.dispatch.test/client.json",
        "https://127.0.0.1/client.json",
        "https://[::1]/client.json",
        "https://2130706433/client.json",
        "https://localhost/client.json",
        "https://metadata/computeMetadata",
        " https://app.example.com/client.json",
        "dcr_00000000000000000000000000000000",
        "evil",
    ] {
        assert!(web_client_id(id).is_none(), "{id}");
    }
    let long = format!("https://app.example.com/{}", "a".repeat(LONGEST));
    assert!(web_client_id(&long).is_none());
}

/// A website that answers its address but never its document.
struct Silent;
impl Network for Silent {
    fn resolve<'a>(&'a self, _: &'a str) -> Pending<'a, Result<Vec<IpAddr>>> {
        Box::pin(async { Ok(vec![IpAddr::V4(Ipv4Addr::new(93, 184, 215, 14))]) })
    }
    fn get<'a>(
        &'a self,
        _: &'a Url,
        _: SocketAddr,
        _: usize,
    ) -> Pending<'a, Result<(u16, Vec<u8>)>> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn a_fetch_gives_up_after_five_seconds() {
    let started = std::time::Instant::now();
    let url = web_client_id("https://slow.example.com/client.json").unwrap();
    let failed = fetch(&Silent, &url, 1024).await.unwrap_err();
    assert_eq!(failed.code, "app_unavailable");
    let took = started.elapsed();
    assert!(took >= TIMEOUT && took < TIMEOUT * 2, "{took:?}");
}
