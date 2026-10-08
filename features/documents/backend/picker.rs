//! Adding files someone made directly in Google Drive. `drive.file` reaches only what Dispatch
//! made or was given, so Google's own file picker gives them: whoever holds the DSP's Google
//! account picks them in a window of its own, signed in to Google there. The dashboard never
//! loads Google's scripts, and Dispatch never hands the account's token to a browser. The
//! window tells the Documents page that opened it which files were picked, and that page adds
//! them.
use super::{drive, google::Google, storage::Connection};
use crate::api::types::{DriveFile, PickerSetup};
use axum::{
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use dispatch_core::{Result, accounts::Context, foundation::crypto};

/// The picker's window, on this server's own origin.
pub const PICKER_PATH: &str = "/api/documents/google/picker";

/// What adding files from Drive needs, for those who manage Documents while Google is
/// connected: none on a server without the picker's keys.
pub fn setup(c: &Context, google: &Google, connection: &Connection) -> Result<Option<PickerSetup>> {
    if !c.allows("documents.manage") || connection.broken {
        return Ok(None);
    }
    let account = connection.account.email.clone();
    Ok(match google {
        Google::Fixture => {
            let made = drive::fixture_hidden(&format!("fixture:{account}"))?;
            Some(PickerSetup {
                made_in_drive: made
                    .into_iter()
                    .map(|item| DriveFile {
                        kind: super::files::kind(&item.mime_type),
                        id: item.id,
                        name: item.name,
                    })
                    .collect(),
                google: None,
                account,
            })
        }
        Google::Live(_) => google.picker_keys().map(|keys| PickerSetup {
            account,
            google: Some(keys),
            made_in_drive: Vec::new(),
        }),
    })
}

/// The picker's window. Its own rules let only Google's sign-in and picker run in it, and
/// let Google's sign-in open its popup; the dashboard's rules stay as they are.
pub fn page() -> Response {
    let nonce = crypto::token().unwrap_or_default();
    let policy = format!(
        "default-src 'none'; script-src 'nonce-{nonce}' https://accounts.google.com \
         https://apis.google.com; style-src 'unsafe-inline' https://accounts.google.com; \
         frame-src https://accounts.google.com https://docs.google.com https://drive.google.com \
         https://content.googleapis.com; connect-src https://accounts.google.com \
         https://www.googleapis.com; img-src https: data:; font-src https://fonts.gstatic.com; \
         base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    );
    let mut response = (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        PAGE.replace("{nonce}", &nonce),
    )
        .into_response();
    let headers = response.headers_mut();
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        headers.insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin-allow-popups"),
    );
    response
}

/// The window: it signs in to Google as the account that holds Documents, opens Google's
/// picker, and tells the Documents page what was picked, on a channel only this origin
/// reaches, with the word the page gave it. Its settings come in the address's fragment,
/// which never reaches a server.
const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Add from Google Drive · Dispatch</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center;
    font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; background: #f6f7f9; color: #1f2430; }
  main { max-width: 440px; padding: 32px; text-align: center; }
  h1 { margin: 0 0 8px; font-size: 22px; }
  p { margin: 0 0 20px; color: #5b6472; }
  button { font: inherit; font-weight: 600; padding: 10px 18px; border: 0; border-radius: 8px;
    background: #1d4ed8; color: #fff; cursor: pointer; }
  button:disabled { opacity: 0.6; cursor: default; }
  @media (prefers-color-scheme: dark) {
    body { background: #14161b; color: #e8eaee; } p { color: #a3a9b4; }
  }
</style>
</head>
<body>
<main>
  <h1>Add from Google Drive</h1>
  <p id="note">Sign in to Google as <strong id="account"></strong>, the account that holds your team's Documents, then choose the files to add.</p>
  <button id="choose" disabled>Choose files</button>
</main>
<script nonce="{nonce}" src="https://accounts.google.com/gsi/client"></script>
<script nonce="{nonce}" src="https://apis.google.com/js/api.js"></script>
<script nonce="{nonce}">
  const sent = new URLSearchParams(location.hash.slice(1));
  history.replaceState(null, '', location.pathname);
  const button = document.getElementById('choose');
  const note = document.getElementById('note');
  document.getElementById('account').textContent = sent.get('account') || '';
  const channel = new BroadcastChannel('dispatch-documents-picker');
  const tell = (message) => channel.postMessage({ word: sent.get('word'), ...message });
  const say = (words) => { note.textContent = words; };
  let token;
  const client = google.accounts.oauth2.initTokenClient({
    client_id: sent.get('clientId'),
    scope: 'https://www.googleapis.com/auth/drive.file',
    login_hint: sent.get('account'),
    callback: (answer) => {
      if (answer.error) return say("Google didn't sign you in. Try again.");
      token = answer.access_token;
      choose();
    },
  });
  function choose() {
    const view = new google.picker.DocsView(google.picker.ViewId.DOCS)
      .setIncludeFolders(true)
      .setSelectFolderEnabled(false);
    new google.picker.PickerBuilder()
      .addView(view)
      .enableFeature(google.picker.Feature.MULTISELECT_ENABLED)
      .setOAuthToken(token)
      .setDeveloperKey(sent.get('apiKey'))
      .setAppId(sent.get('appId'))
      .setCallback((data) => {
        if (data.action === google.picker.Action.PICKED) {
          tell({ files: data.docs.map((doc) => doc.id) });
          say('Adding them to Documents. You can close this window.');
          window.close();
        } else if (data.action === google.picker.Action.CANCEL) {
          window.close();
        }
      })
      .build()
      .setVisible(true);
  }
  gapi.load('picker', () => { button.disabled = false; });
  button.addEventListener('click', () => (token ? choose() : client.requestAccessToken({ prompt: '' })));
</script>
</body>
</html>
"#;
