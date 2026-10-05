use super::*;

#[test]
fn requests_are_sorted_into_the_kinds_the_owner_chooses_among() {
    let local = Some("http://127.0.0.1:5/cb");
    for (client, redirect, app) in [
        (
            "https://chatgpt.com/oauth/client.json",
            Some("https://chatgpt.com/connector_platform_oauth_redirect"),
            Some(OAuthAppId::Chatgpt),
        ),
        (
            "https://chatgpt.com/oauth/codex/client.json",
            local,
            Some(OAuthAppId::Codex),
        ),
        (
            "https://claude.ai/oauth/claude-code-client-metadata",
            local,
            Some(OAuthAppId::ClaudeCode),
        ),
        (
            "https://nousresearch.github.io/hermes-agent/docs/oauth/client-metadata.json",
            local,
            Some(OAuthAppId::Hermes),
        ),
        ("dcr_x", local, Some(OAuthAppId::Local)),
        ("dcr_x", Some("cursor://a/cb"), Some(OAuthAppId::Local)),
        ("dcr_x", None, Some(OAuthAppId::Local)),
        (
            "dcr_x",
            Some("https://app.example/cb"),
            Some(OAuthAppId::Web),
        ),
        (
            "https://app.example/client.json",
            local,
            Some(OAuthAppId::Web),
        ),
        // A known app's lookalike is a website, never the known app.
        (
            "https://claude.ai/oauth/claude-code-client-metadata/",
            local,
            Some(OAuthAppId::Web),
        ),
        ("https://127.0.0.1/client.json", local, None),
        ("http://app.example/client.json", local, None),
        ("evil", local, None),
    ] {
        assert_eq!(app_of(client, redirect), app, "{client} {redirect:?}");
    }
    assert_eq!(APPS.len(), 6);
    assert_eq!(named(OAuthAppId::Web), ("Websites and other apps", false));
}
