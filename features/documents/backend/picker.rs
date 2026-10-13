//! Adding files and folders someone made directly in Google Drive. `drive.file` reaches only
//! what Dispatch made or was given, so Google's own file picker gives them: whoever holds the
//! DSP's Google account picks them on a page of its own, in the same tab, signed in to Google
//! there. The dashboard never loads Google's scripts, and Dispatch never hands the account's
//! token to a browser. The page goes back to Documents with what was picked, and Documents
//! adds it.
use super::{
    drive,
    google::{Google, RETURN_PATH},
    storage::Connection,
};
use crate::api::types::{DriveFile, PickerSetup};
use axum::{
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Result,
    accounts::Context,
    foundation::{
        config::{Config, Site},
        crypto,
    },
};

/// The picker's page, at the address of the Documents page that sent the browser to it.
pub const PICKER_PATH: &str = "/api/documents/google/picker";

/// What adding files from Drive needs, for those who manage the DSP's connections while
/// Google is connected, since only they hold its account: none on a server without the
/// picker's keys.
pub fn setup(c: &Context, google: &Google, connection: &Connection) -> Result<Option<PickerSetup>> {
    if !c.allows("connections.manage") || connection.broken {
        return Ok(None);
    }
    let account = connection.account.email.clone();
    let folder = connection.folder_id.clone();
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
                folder,
            })
        }
        Google::Live(_) => google.picker_keys().map(|keys| PickerSetup {
            account,
            folder,
            google: Some(keys),
            made_in_drive: Vec::new(),
        }),
    })
}

/// The picker's page, at `site`'s address. Google sends the browser back to the platform's
/// own return address, the one Google knows, and the page names its DSP's address in what it
/// asks Google to say back, so that address takes the browser on to it.
pub fn page(config: &Config, site: Option<&Site>) -> Response {
    let code = match site {
        Some(Site::Dsp(code)) => code.as_str(),
        _ => "",
    };
    let settings = serde_json::json!({
        "back": format!("{}{RETURN_PATH}", config.origin),
        "code": code,
    });
    let mut response = html(
        PAGE,
        &settings,
        "script-src 'nonce-{nonce}' https://apis.google.com; style-src 'unsafe-inline'; \
         frame-src https://accounts.google.com https://docs.google.com https://drive.google.com \
         https://content.googleapis.com; connect-src https://www.googleapis.com; \
         img-src https: data:; font-src https://fonts.gstatic.com",
    );
    // A window Google's picker opens can still answer it.
    response.headers_mut().insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin-allow-popups"),
    );
    response
}

/// What Google's return address shows when Google sends the browser back to the picker, its
/// answer in the address's fragment, which never reaches a server: a page that takes it on
/// to the picker's page at the address it started at, the platform's or a DSP's.
pub fn relay(config: &Config) -> Response {
    let settings = serde_json::json!({
        "origin": config.origin,
        "dsp": config.dsp_origin,
        "picker": PICKER_PATH,
    });
    html(RELAY, &settings, "script-src 'nonce-{nonce}'")
}

/// `page` with its settings and a nonce for its scripts, under rules of its own that allow
/// `allowed` and nothing else; the dashboard's rules stay as they are.
fn html(page: &str, settings: &serde_json::Value, allowed: &str) -> Response {
    let nonce = crypto::token().unwrap_or_default();
    let policy = format!(
        "default-src 'none'; {}; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        allowed.replace("{nonce}", &nonce)
    );
    let settings = settings.to_string().replace('<', "\\u003c");
    let mut response = (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        page.replace("{nonce}", &nonce)
            .replace("{settings}", &settings),
    )
        .into_response();
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

/// The picker's page. The Documents page left it, in the tab's session storage, what to pick
/// and where to go back to. It sends the browser to Google to sign in as the account that holds
/// Documents, which comes straight back when the browser is signed in already, and opens
/// Google's picker on the files a link named, or on the folder open in Documents. It checks
/// Google signed in that account first, since a file picked as another can't be added. Then it
/// goes back to Documents, leaving what was picked for it there. It names this address to
/// Google, though not the page, since Google's picker key works only from Dispatch's own.
/// Beside the folder open in Documents, the picker shows what's shared with the account, and
/// the shared drives it's in.
const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="strict-origin">
<title>Google Drive · Dispatch</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center;
    font: 15px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    background: #f6f7f9; color: #1f2430; }
  main { max-width: 440px; padding: 32px; text-align: center; }
  p { margin: 0 0 20px; }
  .actions { display: flex; gap: 10px; justify-content: center; }
  button { font: inherit; font-weight: 600; padding: 10px 18px; border: 0; border-radius: 8px;
    background: #1d4ed8; color: #fff; cursor: pointer; }
  button.quiet { background: transparent; color: inherit; box-shadow: inset 0 0 0 1px #c9ced6; }
  @media (prefers-color-scheme: dark) {
    body { background: #14161b; color: #e8eaee; }
    button.quiet { box-shadow: inset 0 0 0 1px #3a404c; }
  }
</style>
</head>
<body>
<main hidden>
  <p id="note"></p>
  <div class="actions">
    <button id="back" class="quiet">Back to Documents</button>
    <button id="again"></button>
  </div>
</main>
<script nonce="{nonce}" src="https://apis.google.com/js/api.js"></script>
<script nonce="{nonce}">
  const settings = {settings};
  const ASKED = 'dispatch-documents-picker';
  const PICKED = 'dispatch-documents-picked';
  const answer = new URLSearchParams(location.hash.slice(1));
  history.replaceState(null, '', location.pathname);
  // What Documents asked for, and the page to go back to; nothing if either can't be read.
  let asked = null;
  let home = null;
  try {
    asked = JSON.parse(sessionStorage.getItem(ASKED) || 'null');
    home = new URL(asked.back);
  } catch {
    asked = null;
  }

  // Back to Documents, with what was picked if anything was.
  function back(files) {
    sessionStorage.removeItem(ASKED);
    if (files) sessionStorage.setItem(PICKED, JSON.stringify({ files, at: asked.back }));
    location.replace(home.href);
  }
  function fail(words, action, then) {
    document.getElementById('note').textContent = words;
    const again = document.getElementById('again');
    again.textContent = action;
    again.onclick = then;
    document.getElementById('back').onclick = () => back();
    document.querySelector('main').hidden = false;
  }
  function signIn(choosing) {
    const word = Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) =>
      b.toString(16).padStart(2, '0'),
    ).join('');
    const state = `picker.${settings.code}.${word}`;
    sessionStorage.setItem(ASKED, JSON.stringify({ ...asked, state }));
    const query = new URLSearchParams({
      client_id: asked.clientId,
      redirect_uri: settings.back,
      response_type: 'token',
      scope: 'https://www.googleapis.com/auth/drive.file',
      login_hint: asked.account,
      state,
    });
    if (choosing) query.set('prompt', 'select_account');
    location.replace(`https://accounts.google.com/o/oauth2/v2/auth?${query}`);
  }
  async function open(token) {
    sessionStorage.setItem(ASKED, JSON.stringify({ ...asked, state: undefined }));
    const signedIn = await fetch(
      'https://www.googleapis.com/drive/v3/about?fields=user(emailAddress)',
      { headers: { authorization: `Bearer ${token}` } },
    )
      .then((response) => (response.ok ? response.json() : undefined))
      .then((about) => about?.user?.emailAddress, () => undefined);
    if (signedIn && signedIn.toLowerCase() !== asked.account.toLowerCase())
      return fail(`Sign in to Google as ${asked.account}.`, 'Switch account', () => signIn(true));
    const failed = () => fail("Google Drive didn't open.", 'Try again', () => signIn(false));
    if (!window.gapi) return failed();
    gapi.load('picker', { callback: () => choose(token), onerror: failed });
  }
  function choose(token) {
    const view = () =>
      new google.picker.DocsView(google.picker.ViewId.DOCS)
        .setIncludeFolders(true)
        .setSelectFolderEnabled(true);
    const builder = new google.picker.PickerBuilder();
    // A link's files alone; else the folder open in Documents, then what's shared with the
    // account, and the shared drives it's in.
    if (asked.files) builder.addView(view().setFileIds(asked.files));
    else
      builder
        .addView(view().setParent(asked.folder))
        .addView(view().setOwnedByMe(false))
        .addView(view().setEnableDrives(true));
    builder
      .enableFeature(google.picker.Feature.MULTISELECT_ENABLED)
      .setOAuthToken(token)
      .setDeveloperKey(asked.apiKey)
      .setAppId(asked.appId)
      .setCallback((data) => {
        if (data.action === google.picker.Action.PICKED) back(data.docs.map((doc) => doc.id));
        else if (data.action === google.picker.Action.CANCEL) back();
      })
      .build()
      .setVisible(true);
  }

  if (!asked || home.origin !== location.origin) location.replace('/');
  else if (!answer.has('state')) signIn(false);
  else if (answer.get('state') !== asked.state || answer.get('error') === 'access_denied') back();
  else if (!answer.get('access_token'))
    fail("Google didn't sign you in.", 'Try again', () => signIn(false));
  else open(answer.get('access_token'));
</script>
</body>
</html>
"#;

/// Google's return address, when Google sends the browser back to the picker: on to the
/// picker's page at the address the state names, the platform's or a DSP's, and with the
/// fragment, which carries Google's answer.
const RELAY: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Dispatch</title>
</head>
<body>
<script nonce="{nonce}">
  const settings = {settings};
  const [kind, code] = (new URLSearchParams(location.hash.slice(1)).get('state') || '').split('.');
  const at = /^[a-z0-9]{2,16}$/.test(code || '')
    ? settings.dsp.replace('{code}', code)
    : settings.origin;
  location.replace(kind === 'picker' ? `${at}${settings.picker}${location.hash}` : `${settings.origin}/`);
</script>
</body>
</html>
"#;
