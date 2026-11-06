use super::{LONGEST_REDIRECT, allowed, destination, private_scheme, registrable};

#[test]
fn loopback_redirects_match_on_any_port_and_nothing_else_does() {
    let registered = |uris: &[&str]| uris.iter().map(|u| (*u).to_owned()).collect::<Vec<_>>();
    let portless = registered(&["http://localhost/callback", "http://127.0.0.1/callback"]);
    assert!(allowed(&portless, "http://localhost:53122/callback"));
    assert!(allowed(&portless, "http://127.0.0.1:61001/callback"));
    assert!(allowed(&portless, "http://127.0.0.1/callback"));
    for wrong in [
        "http://127.0.0.1:61001/callback2",
        "http://127.0.0.1:61001/callback?x=1",
        "https://127.0.0.1:61001/callback",
        "http://127.0.0.2:61001/callback",
        "http://127.0.0.1:99999/callback",
        "http://127.0.0.1:/callback",
        "http://localhost.evil.example/callback",
        "http://localhost@evil.example/callback",
    ] {
        assert!(!allowed(&portless, wrong), "{wrong}");
    }
    // localhost and 127.0.0.1 are different hosts.
    assert!(!allowed(
        &registered(&["http://localhost/cb"]),
        "http://127.0.0.1:5/cb"
    ));
    assert!(allowed(
        &registered(&["http://[::1]:7/cb"]),
        "http://[::1]:9/cb"
    ));
    // Anything else matches only exactly, port and all.
    let web = registered(&["https://chatgpt.com/connector_platform_oauth_redirect"]);
    assert!(allowed(
        &web,
        "https://chatgpt.com/connector_platform_oauth_redirect"
    ));
    assert!(!allowed(
        &web,
        "https://chatgpt.com:443/connector_platform_oauth_redirect"
    ));
    assert!(!allowed(
        &web,
        "https://chatgpt.com/connector_platform_oauth_redirect/"
    ));
    let app = registered(&["cursor://anysphere.cursor-retrieval/oauth/callback"]);
    assert!(allowed(
        &app,
        "cursor://anysphere.cursor-retrieval/oauth/callback"
    ));
    assert!(!allowed(
        &app,
        "cursor://anysphere.cursor-retrieval:1/oauth/callback"
    ));
}

#[test]
fn apps_register_only_redirects_to_this_computer() {
    for uri in [
        "http://127.0.0.1:27890/callback",
        "http://localhost/callback",
        "http://[::1]:8080/cb?x=1",
        "cursor://anysphere.cursor-retrieval/oauth/callback",
        "vscode://vscode.github-authentication/did-authenticate",
        "vscode-insiders://callback",
        "windsurf://codeium.windsurf/callback",
        "com.example.app:/oauth2redirect",
    ] {
        assert!(registrable(uri), "{uri}");
    }
    // A scheme is a native app's only when reverse-domain or a known app's; a web+ scheme
    // is a website's protocol handler.
    for uri in [
        "web+dsp://cb",
        "web+dsp.example://cb",
        "magnet:?xt=urn:btih:abc",
        "myapp://callback",
        "sms:5550100",
    ] {
        assert!(private_scheme(uri).is_none(), "{uri}");
        assert!(!registrable(uri), "{uri}");
    }
    let long = format!("http://127.0.0.1:1/{}", "a".repeat(LONGEST_REDIRECT));
    assert!(!registrable(&long));
    for uri in [
        "https://evil.example/callback",
        "http://evil.example/callback",
        "http://127.0.0.1:27890",
        "http://127.0.0.1:27890/callback#here",
        "javascript:alert(1)",
        "data:text/html,hi",
        "file:///etc/passwd",
        "JavaScript:alert(1)",
        "Cursor://x/y",
        "1app://x",
        "cursor:",
        "cursor://x/ y",
    ] {
        assert!(!registrable(uri), "{uri}");
    }
    assert_eq!(
        destination("http://127.0.0.1:5/callback"),
        ("this computer".into(), None)
    );
    assert_eq!(
        destination("cursor://anysphere.cursor-retrieval/oauth/callback"),
        ("this computer".into(), Some("cursor".into()))
    );
    assert_eq!(
        destination("https://chatgpt.com/connector_platform_oauth_redirect"),
        ("chatgpt.com".into(), None)
    );
}
