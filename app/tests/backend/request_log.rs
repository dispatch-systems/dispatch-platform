//! The route the request log names each request by until its route names it by its
//! pattern, as it stood before routes declared theirs.

/// A path, and the route the request log names it by.
const LABELS: &[(&str, &str)] = &[
    ("/api/health", "/api/health"),
    ("/api/auth/login", "/api/auth/login"),
    ("/api/auth/logout", "/api/auth/logout"),
    ("/api/auth/password", "/api/auth/password"),
    ("/api/auth/forgot-password", "/api/auth/forgot-password"),
    ("/api/auth/reset-password", "/api/auth/reset-password"),
    ("/api/session", "/api/session"),
    ("/api/session/dsp", "/api/session/dsp"),
    ("/api/dsp/jobs", "/api/dsp/jobs"),
    ("/api/dsp/paycom/meal-breaks", "/api/dsp/paycom/meal-breaks"),
    ("/api/dsp/paycom/settings", "/api/dsp/paycom/settings"),
    ("/api/dsp/timecards", "/api/dsp/timecards"),
    ("/api/dsp/employees", "/api/dsp/employees"),
    ("/api/dsp/collection-updates", "/api/dsp/collection-updates"),
    ("/api/platform/health", "/api/platform/health"),
    ("/api/invitations/token", "/api/invitations/:token/*"),
    ("/api/invitations/token/accept", "/api/invitations/:token/*"),
    ("/api/invitations/", "/api/invitations/:token/*"),
    ("/api/dsp/employees/E001", "/api/dsp/employees/:code"),
    ("/api/dsp/employees/E001/sync", "/api/dsp/employees/:code"),
    ("/api/dsp/employees/", "/api/dsp/employees/:code"),
    (
        "/api/dsp/employees/E001/more/segments",
        "/api/dsp/employees/:code",
    ),
    ("/api/dsp/employeesE001", "/api/dsp/*"),
    ("/api/dsp/timecards/daily", "/api/dsp/*"),
    ("/api/dsp/paycom/meal-breaks/day", "/api/dsp/*"),
    ("/api/dsp/paycom", "/api/dsp/*"),
    ("/api/dsp/routes", "/api/dsp/*"),
    ("/api/dsp/jobs/meal-breaks", "/api/dsp/*"),
    ("/api/dsp/", "/api/dsp/*"),
    ("/api/dsp", "/unmatched"),
    ("/api/platform/dsps", "/api/platform/*"),
    ("/api/platform/", "/api/platform/*"),
    ("/", "/"),
    ("/assets/index.js", "/assets/*"),
    ("/.well-known/oauth-authorization-server", "/.well-known/*"),
    ("/oauth/token", "/oauth/*"),
    ("/api/browser-update", "/unmatched"),
    ("/api/v1/mcp", "/unmatched"),
    ("/api/auth/security/status", "/unmatched"),
    ("/api/session/", "/unmatched"),
    ("/API/health", "/unmatched"),
    ("/elsewhere", "/unmatched"),
    ("", "/unmatched"),
];

fn label(path: &str) -> &'static str {
    crate::observability::route(path)
}

#[test]
fn each_path_keeps_its_name_in_the_request_log() {
    let found: Vec<_> = LABELS
        .iter()
        .map(|(path, _)| (*path, label(path)))
        .collect();
    assert_eq!(found, LABELS);
}
