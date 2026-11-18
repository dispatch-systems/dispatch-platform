use super::*;

#[test]
fn clients_are_named_without_their_whole_user_agent() {
    assert_eq!(
        client_label("claude-code/2.1.283 (external, cli)"),
        "Claude Code 2.1"
    );
    assert_eq!(
        client_label("codex_cli_rs/0.157.1 (Ubuntu 24.04; x86_64) xterm"),
        "Codex 0.157"
    );
    assert_eq!(client_label("curl/8.5.0"), "curl 8.5");
    assert_eq!(client_label("python-httpx2/2.7.0"), "HTTPX2 2.7");
    assert_eq!(client_label("python-httpx/0.28.1"), "httpx 0.28");
    assert_eq!(client_label("python-requests/2.32.3"), "requests 2.32");
    assert_eq!(client_label("Python-urllib/3.12"), "Python 3.12");
    assert_eq!(client_label("Mozilla/5.0 (X11; Linux x86_64)"), "Browser");
    assert_eq!(client_label("my-agent/1.0-beta"), "my-agent 1");
    assert_eq!(client_label("<script>/1"), "script 1");
    assert_eq!(client_label(""), "Unknown");
}
#[test]
fn expiries_are_a_minute_to_five_years_away() {
    assert_eq!(expiry(None).unwrap(), None);
    // A time days from now, written with its offset as a client may.
    let after = |days: i64| at(now() + days * 86_400_000).replace('Z', "+00:00");
    let kept = expiry(Some(&after(30))).unwrap().unwrap();
    assert!(kept.ends_with('Z') && kept > iso());
    assert!(expiry(Some(&after(-1))).is_err());
    assert!(expiry(Some(&after(6 * 366))).is_err());
    assert!(expiry(Some("next tuesday")).is_err());
}
