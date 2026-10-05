#[test]
fn only_production_names_the_release_its_source_came_from() {
    let bundle = tempfile::tempdir().unwrap();
    let unbuilt = super::source(bundle.path(), true);
    assert_eq!((unbuilt.version, unbuilt.commit), (None, None));
    std::fs::create_dir(bundle.path().join("tooling")).unwrap();
    std::fs::write(
        bundle.path().join("release.json"),
        r#"{"version":"0.0.23"}"#,
    )
    .unwrap();
    std::fs::write(
        bundle.path().join("tooling/build-info.json"),
        r#"{"commit":"c3f898be7c709f65c9d6e69a391fffbe03ecb5b6"}"#,
    )
    .unwrap();
    let commit = Some("c3f898be7c709f65c9d6e69a391fffbe03ecb5b6".to_owned());
    let dev = super::source(bundle.path(), false);
    assert_eq!((dev.version, dev.commit), (None, commit.clone()));
    let production = super::source(bundle.path(), true);
    assert_eq!(
        (production.version, production.commit),
        (Some("0.0.23".into()), commit)
    );
}

#[test]
fn smtp_requires_transport_security() {
    for endpoint in [
        "smtps://mail.example.com",
        "smtps://user:password@mail.example:465",
        "smtp://mail.example.com:587?tls=required",
    ] {
        assert!(super::secure_smtp_url(
            &url::Url::parse(endpoint).unwrap(),
            false
        ));
    }
    for endpoint in [
        "smtp://mail.example.com",
        "smtp://mail.example.com?tls=opportunistic",
        "smtp://mail.example.com?tls=required&tls=none",
        "smtp://mail.example.com?tls=required#fragment",
    ] {
        assert!(!super::secure_smtp_url(
            &url::Url::parse(endpoint).unwrap(),
            false
        ));
    }
    let loopback = url::Url::parse("smtp://127.0.0.1:2525").unwrap();
    assert!(super::secure_smtp_url(&loopback, true));
    assert!(!super::secure_smtp_url(&loopback, false));
}
