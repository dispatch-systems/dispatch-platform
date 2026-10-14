//! The one page Slack's install link ends on when it can't go on to Slack: whoever opened it
//! may have no Dispatch account, so the page stands alone, with rules of its own that let only
//! its own style run.
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use dispatch_core::foundation::crypto;

/// Why an install by link stopped, by the error it stopped on.
pub fn stopped(code: &str) -> Response {
    let (status, heading) = match code {
        "slack_link_expired" | "slack_connect_expired" => {
            (StatusCode::NOT_FOUND, "This install link has expired")
        }
        "slack_workspace_taken" => (
            StatusCode::CONFLICT,
            "This workspace is already connected to another DSP",
        ),
        "slack_cancelled" => (StatusCode::OK, "Dispatch wasn't added to Slack"),
        _ => (StatusCode::BAD_GATEWAY, "Slack didn't finish adding Dispatch"),
    };
    let nonce = crypto::token().unwrap_or_default();
    let page = PAGE
        .replace("{nonce}", &nonce)
        .replace("{heading}", heading);
    let mut response = (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        page,
    )
        .into_response();
    let policy = format!(
        "default-src 'none'; style-src 'nonce-{nonce}'; base-uri 'none'; form-action 'none'; \
         frame-ancestors 'none'"
    );
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Dispatch</title>
<style nonce="{nonce}">
body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f7f8fa;
  color: #171b25; font: 16px/1.5 Inter, ui-sans-serif, system-ui, sans-serif; }
main { display: grid; justify-items: center; gap: 20px; padding: 24px; text-align: center; }
svg { width: 40px; height: 40px; color: #2055ed; }
h1 { margin: 0; font-size: 20px; font-weight: 600; }
</style>
</head>
<body>
<main>
<svg viewBox="0 0 28 28" aria-hidden="true"><path fill="currentColor" d="M3 3h9C20 3 25 7.5 25 14s-5 11-13 11H3v-7h6v2h3c4.5 0 7-2.2 7-6s-2.5-6-7-6H9v6H3V3Z"/></svg>
<h1>{heading}</h1>
</main>
</body>
</html>
"##;
