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

#[test]
fn a_host_names_the_admin_the_invite_page_or_a_dsp_by_its_short_code() {
    use super::Site;
    let mut config = super::Config::load().unwrap();
    config.origin = "https://admin.dispatch.example".into();
    config.invite_origin = "https://invite.dispatch.example".into();
    config.dsp_origin = "https://{code}.dispatch.example".into();
    let dsp = |code: &str| Some(Site::Dsp(code.into()));
    for (host, site) in [
        ("admin.dispatch.example", Some(Site::Admin)),
        ("ADMIN.dispatch.example", Some(Site::Admin)),
        (&format!("127.0.0.1:{}", config.port), Some(Site::Admin)),
        ("invite.dispatch.example", Some(Site::Invite)),
        ("nstl.dispatch.example", dsp("nstl")),
        ("NSTL.dispatch.example", dsp("nstl")),
        ("dsp2.dispatch.example", dsp("dsp2")),
        // A short code is 2 to 16 letters and digits, and never a name kept for the platform.
        ("f.dispatch.example", None),
        ("a234567890123456x.dispatch.example", None),
        ("ns-tl.dispatch.example", None),
        ("www.dispatch.example", None),
        ("dev.dispatch.example", None),
        ("nstl.dev.dispatch.example", None),
        ("dispatch.example", None),
        ("nstl.dispatch.example.evil.test", None),
        ("evil.test", None),
        ("", None),
    ] {
        assert_eq!(config.site(host), site, "{host}");
    }
    assert_eq!(config.dsp_url("NSTL"), "https://nstl.dispatch.example");
    assert_eq!(
        config.site_origin(&Site::Dsp("nstl".into())),
        "https://nstl.dispatch.example"
    );
    assert!(config.reserved_code("admin") && config.reserved_code("Invite"));
    assert!(!config.reserved_code("nstl"));
    // A DSP's address can never be the admin's or the invite page's, whatever they are named.
    config.origin = "https://boss.dispatch.example".into();
    assert!(config.reserved_code("boss"));
    assert_eq!(config.site("boss.dispatch.example"), Some(Site::Admin));
}

#[test]
fn a_deployed_server_names_every_address_and_development_runs_on_localhost() {
    let parse = |origin: &str| url::Url::parse(origin).unwrap();
    let local = super::addresses(&parse("http://127.0.0.1:4100"), false, None, None).unwrap();
    assert_eq!(
        local,
        (
            "http://invite.localhost:4100".into(),
            "http://{code}.localhost:4100".into()
        )
    );
    let preview = super::addresses(
        &parse("http://preview.dispatch.example:4100"),
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(preview, local);
    let admin = parse("https://admin.dispatch.example");
    let named = |invite: &str, dsp: &str| {
        super::addresses(&admin, false, Some(invite.into()), Some(dsp.into()))
    };
    assert_eq!(
        named(
            "https://invite.dispatch.example",
            "https://{code}.dispatch.example"
        )
        .unwrap(),
        (
            "https://invite.dispatch.example".into(),
            "https://{code}.dispatch.example".into()
        )
    );
    assert!(super::addresses(&admin, false, None, None).is_err());
    assert!(
        super::addresses(
            &admin,
            false,
            Some("https://invite.dispatch.example".into()),
            None
        )
        .is_err()
    );
    for (invite, dsp) in [
        (
            "http://invite.dispatch.example",
            "https://{code}.dispatch.example",
        ),
        (
            "https://admin.dispatch.example",
            "https://{code}.dispatch.example",
        ),
        (
            "https://invite.dispatch.example/",
            "https://{code}.dispatch.example",
        ),
        (
            "https://invite.dispatch.example",
            "https://dsp.dispatch.example",
        ),
        (
            "https://invite.dispatch.example",
            "https://nstl{code}.dispatch.example",
        ),
        (
            "https://invite.dispatch.example",
            "https://{code}.{code}.dispatch.example",
        ),
        (
            "https://invite.dispatch.example",
            "https://dispatch.example/{code}",
        ),
        (
            "https://invite.dispatch.example",
            "http://{code}.dispatch.example",
        ),
    ] {
        assert!(named(invite, dsp).is_err(), "{invite} {dsp}");
    }
}
