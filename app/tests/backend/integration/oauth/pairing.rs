use super::harness::{
    CHATGPT, CHATGPT_REDIRECT, CLAUDE_CODE, Server, VERIFIER, asked, everything, live_apps, logged,
    registered,
};
use crate::support::{Answer, OWNER};
use dispatch_core::{
    State,
    db::{self, s},
    foundation::crypto,
};
use dispatch_mcp::GuardStore;
use serde_json::json;

const DAY: i64 = 24 * 60 * 60 * 1000;

/// The name of a browser's cookie for an authorization request, as development names it.
fn browser_cookie(request: &str) -> String {
    format!("dispatch_oauth_request_{request}")
}

#[tokio::test]
async fn the_owner_can_deny_and_the_app_is_told() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["app"]["redirectHost"], "chatgpt.com");
    let denied = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/deny"),
            json!({}),
        )
        .await;
    assert_eq!(denied.status, 200, "{}", denied.body);
    assert_eq!(
        s(&denied.body, "redirect"),
        format!(
            "{CHATGPT_REDIRECT}?error=access_denied&state=st%2Fate+1&iss={}",
            url::form_urlencoded::byte_serialize(server.origin.as_bytes()).collect::<String>()
        )
    );
    // Refused, the app is in the audit log by the name it asked under.
    assert_eq!(
        logged(&server, "agent.app_denied").await,
        [("agent.app_denied".to_owned(), "ChatGPT".to_owned())]
    );
}

/// Ends the pairing window, as ten minutes passing would.
async fn close_pairing(server: &Server) {
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_pairing SET open_until=?",
                [db::at(db::now() - 1000)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn apps_ask_to_connect_only_while_the_pairing_window_is_open() {
    let server = Server::start().await;
    let owner = server.owner().await;
    let local = "http://localhost:50010/callback";
    let closed = format!("{}/#authorize?error=pairing_closed", server.origin);
    let pairing = "/api/platform/oauth/pairing";
    let requests = || {
        server
            .state
            .read(|db| db.platform.count("SELECT count(*) FROM oauth_requests", []))
    };
    // Closed until the owner opens it: the browser goes to Dispatch's page, never to the
    // app, and nothing is stored.
    let shown = server.as_owner(&owner, "GET", pairing, json!({})).await;
    assert_eq!(
        (shown.status, &shown.body),
        (200, &json!({"openUntil":null}))
    );
    assert_eq!(asked(&server, CLAUDE_CODE, local).await, closed);
    assert_eq!(requests().await.unwrap(), 0);
    // Only a platform owner opens it, and with no fresh verification: a session signed in a
    // day ago does.
    assert_eq!(server.get(pairing).await.status, 401);
    let earlier = server.signed_in(OWNER, db::now() - DAY).await;
    let opened = server.as_owner(&earlier, "POST", pairing, json!({})).await;
    assert_eq!(opened.status, 200, "{}", opened.body);
    let until = s(&opened.body, "openUntil").to_owned();
    let left = chrono::DateTime::parse_from_rfc3339(&until)
        .unwrap()
        .timestamp_millis()
        - db::now();
    assert!(left > 9 * 60_000 && left <= 10 * 60_000, "{until}");
    let shown = server.as_owner(&owner, "GET", pairing, json!({})).await;
    assert_eq!(shown.body, json!({"openUntil":until}));
    let refused = server
        .as_owner(&owner, "POST", pairing, json!({"minutes":60}))
        .await;
    assert_eq!(refused.status, 400);
    // Open, an app asks and its request waits for the owner.
    let request = server.requested(CLAUDE_CODE, local).await;
    assert_eq!(requests().await.unwrap(), 1);
    // Opening it again keeps it open ten minutes from then; the log has the one opening.
    let again = server.as_owner(&owner, "POST", pairing, json!({})).await;
    let extended = s(&again.body, "openUntil").to_owned();
    assert!(extended >= until, "{extended} {until}");
    assert_eq!(
        logged(&server, "agent.pairing").await,
        [("agent.pairing_opened".to_owned(), String::new())]
    );
    // It is stored, so a restart keeps it open.
    let restarted = State::new(server.state.config.clone()).unwrap();
    let kept = restarted.read(|db| db.oauth_pairing()).await.unwrap();
    assert_eq!(kept.open_until, Some(extended));
    // Closed again, nothing more may ask; but the request made while it was open is still
    // approved, and the app signs in, renews and signs out without the window.
    close_pairing(&server).await;
    assert_eq!(asked(&server, CLAUDE_CODE, local).await, closed);
    assert_eq!(requests().await.unwrap(), 1);
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let renewed = server
        .refresh(CLAUDE_CODE, s(&tokens.body, "refresh_token"))
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    let access = s(&renewed.body, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
    let signed_out = server
        .form(
            "/oauth/revoke",
            &[("token", access), ("client_id", CLAUDE_CODE)],
        )
        .await;
    assert_eq!(signed_out.status, 200);
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 401);
    // Reopened after closing, the opening is logged again.
    server.as_owner(&owner, "POST", pairing, json!({})).await;
    assert_eq!(logged(&server, "agent.pairing").await.len(), 2);
}

#[tokio::test]
async fn the_owner_chooses_which_kinds_of_app_may_connect() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let path = "/api/platform/oauth/apps";
    let listed = server.as_owner(&owner, "GET", path, json!({})).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let defaults = json!({"apps":[
        {"id":"chatgpt","name":"ChatGPT","allowed":true},
        {"id":"codex","name":"Codex","allowed":true},
        {"id":"claude-code","name":"Claude Code","allowed":true},
        {"id":"hermes","name":"Hermes Agent","allowed":true},
        {"id":"local","name":"Apps on this computer","allowed":true},
        {"id":"web","name":"Websites and other apps","allowed":false},
    ]});
    assert_eq!(listed.body, defaults);
    assert_eq!(server.get(path).await.status, 401);
    // Choosing is routine, as approving is: a session signed in a day ago chooses below.
    let earlier = server.signed_in(OWNER, db::now() - DAY).await;
    for body in [
        json!({"id":"cursor","allowed":false}),
        json!({"id":"codex"}),
        json!({"id":"codex","allowed":"no"}),
        json!({"id":"codex","allowed":false,"until":1}),
    ] {
        let refused = server.as_owner(&owner, "POST", path, body.clone()).await;
        assert_eq!(
            (refused.status, s(&refused.body, "error")),
            (400, "invalid_input"),
            "{body}"
        );
    }
    assert_eq!(
        server.as_owner(&owner, "GET", path, json!({})).await.body,
        defaults
    );
    // An app connected before its kind is turned off stays connected, and renews.
    let local = "http://localhost:50011/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let registered_app = registered(&server, "Cursor", cursor).await;
    for id in ["claude-code", "local"] {
        let changed = server
            .as_owner(&earlier, "POST", path, json!({"id":id,"allowed":false}))
            .await;
        assert_eq!(changed.status, 200, "{}", changed.body);
        let app = changed.body["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap()
            .clone();
        assert_eq!(app["allowed"], false);
    }
    // Turned off, an app is refused on Dispatch's page, which names it; the others ask.
    let page = |app: &str| {
        format!(
            "{}/#authorize?error=app_not_allowed&app={app}",
            server.origin
        )
    };
    assert_eq!(
        asked(&server, CLAUDE_CODE, local).await,
        page("claude-code")
    );
    assert_eq!(asked(&server, &registered_app, cursor).await, page("local"));
    server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let renewed = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    // Turned on again, it asks again. Choosing what is already chosen changes nothing.
    for _ in 0..2 {
        let changed = server
            .as_owner(
                &owner,
                "POST",
                path,
                json!({"id":"claude-code","allowed":true}),
            )
            .await;
        assert_eq!(changed.status, 200, "{}", changed.body);
    }
    server.requested(CLAUDE_CODE, local).await;
    let changes: Vec<(String, String)> = logged(&server, "agent.app_")
        .await
        .into_iter()
        .filter(|(action, _)| action.ends_with("allowed"))
        .collect();
    assert_eq!(
        changes,
        [
            ("agent.app_disallowed".to_owned(), "Claude Code".to_owned()),
            (
                "agent.app_disallowed".to_owned(),
                "Apps on this computer".to_owned()
            ),
            ("agent.app_allowed".to_owned(), "Claude Code".to_owned()),
        ]
    );
}

/// The request an authorization's redirect names.
fn request_id(answer: &Answer) -> String {
    let location = answer.header("location");
    location.rsplit_once("request=").unwrap().1.to_owned()
}

#[tokio::test]
async fn only_the_browser_that_brought_a_request_sees_or_answers_it() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50013/callback";
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(CLAUDE_CODE, local, &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let started = server.authorize(&pairs).await;
    let first = request_id(&started);
    // The browser the app sent is given a nonce for the request, in a cookie named for it,
    // for ten minutes.
    let cookie = started.header("set-cookie").to_owned();
    let (value, attributes) = cookie.split_once(';').unwrap();
    let nonce = value
        .strip_prefix(&format!("{}=", browser_cookie(&first)))
        .unwrap()
        .to_owned();
    assert_eq!(nonce.len(), 43);
    assert!(
        nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{nonce}"
    );
    assert_eq!(attributes, " Path=/; HttpOnly; SameSite=Lax; Max-Age=600");
    let stored = server
        .state
        .read(|db| {
            db.platform
                .one_as::<(String,)>("SELECT browser FROM oauth_requests", [])
        })
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(stored, crypto::sha(&nonce));
    let path = |id: &str, suffix: &str| format!("/api/platform/oauth/requests/{id}{suffix}");
    let refused = |answer: Answer| {
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (403, "wrong_browser"),
            "{}",
            answer.body
        );
    };
    let answers = [
        ("GET", "", json!({})),
        ("POST", "/approve", everything("Claude Code")),
        ("POST", "/deny", json!({})),
    ];
    // The owner signed in elsewhere, such as from a link someone sent them, cannot.
    let held = server.browser();
    server.set_browser(&[]);
    for (method, suffix, body) in &answers {
        refused(
            server
                .as_owner(&owner, method, &path(&first, suffix), body.clone())
                .await,
        );
    }
    // Nor can a browser holding only another request's cookie, that request's nonce under
    // this request's name, an empty one, or this one twice.
    let second = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let other = server.browser();
    let other_nonce = other[0].split_once('=').unwrap().1.to_owned();
    let name = browser_cookie(&first);
    for cookies in [
        other.clone(),
        vec![format!("{name}={other_nonce}")],
        vec![format!("{name}=")],
        vec![value.to_owned(), value.to_owned()],
    ] {
        server.set_browser(&cookies);
        for (method, suffix, body) in &answers {
            refused(
                server
                    .as_owner(&owner, method, &path(&first, suffix), body.clone())
                    .await,
            );
        }
    }
    // Both requests still wait: refusing the wrong browser changed nothing.
    server.set_browser(&held);
    let shown = server
        .as_owner(&owner, "GET", &path(&first, ""), json!({}))
        .await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    // Answered, its cookie is cleared; the request is gone, and its old cookie finds nothing.
    let approved = server
        .as_owner(
            &owner,
            "POST",
            &path(&first, "/approve"),
            everything("Claude Code"),
        )
        .await;
    assert_eq!(approved.status, 200, "{}", approved.body);
    let redirect = s(&approved.body, "redirect");
    assert!(redirect.starts_with(&format!("{local}?")), "{redirect}");
    assert_eq!(
        approved.header("set-cookie"),
        format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    );
    assert!(server.browser().is_empty());
    refused(
        server
            .as_owner(&owner, "GET", &path(&first, ""), json!({}))
            .await,
    );
    server.set_browser(&held);
    let gone = server
        .as_owner(&owner, "GET", &path(&first, ""), json!({}))
        .await;
    assert_eq!(
        (gone.status, s(&gone.body, "error")),
        (404, "authorization_not_found")
    );
    // Denying clears its own cookie too.
    server.set_browser(&other);
    let denied = server
        .as_owner(&owner, "POST", &path(&second, "/deny"), json!({}))
        .await;
    assert_eq!(denied.status, 200, "{}", denied.body);
    assert!(server.browser().is_empty());
    // A refused authorization sets no cookie.
    let closed = server
        .authorize(&[("client_id", "evil"), ("redirect_uri", local)])
        .await;
    assert_eq!(closed.header("set-cookie"), "");
}

#[tokio::test]
async fn apps_started_together_in_one_browser_are_each_approved_in_either_order() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50015/callback";
    for first_answered in [true, false] {
        // The owner copies one app's sign-in command, then another's, before approving either.
        let first = server.requested(CLAUDE_CODE, local).await;
        let second = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
        assert_eq!(server.browser().len(), 2);
        for id in [&first, &second] {
            let shown = server
                .as_owner(
                    &owner,
                    "GET",
                    &format!("/api/platform/oauth/requests/{id}"),
                    json!({}),
                )
                .await;
            assert_eq!(shown.status, 200, "{}", shown.body);
        }
        let order = if first_answered {
            [&first, &second]
        } else {
            [&second, &first]
        };
        // Each approval clears its own cookie and leaves the other one's.
        for (answered, id) in order.into_iter().enumerate() {
            let suffix = if answered == 0 { "a" } else { "b" };
            let name = format!("App {first_answered} {suffix}");
            let approved = server
                .as_owner(
                    &owner,
                    "POST",
                    &format!("/api/platform/oauth/requests/{id}/approve"),
                    everything(&name),
                )
                .await;
            assert_eq!(approved.status, 200, "{}", approved.body);
            let left: Vec<String> = server.browser();
            assert_eq!(left.len(), 1 - answered, "{left:?}");
            assert!(left.iter().all(|cookie| !cookie.contains(id.as_str())));
        }
    }
}

#[tokio::test]
async fn turning_a_kind_of_app_off_stops_its_waiting_requests_and_codes() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50014/callback";
    let choose = |allowed: bool| {
        server.as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"claude-code","allowed":allowed}),
        )
    };
    // A request waiting when its kind is turned off cannot be approved; it can still be
    // approved once the kind is on again.
    let request = server.requested(CLAUDE_CODE, local).await;
    assert_eq!(choose(false).await.status, 200);
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Claude Code"),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (403, "app_not_allowed")
    );
    assert_eq!(choose(true).await.status, 200);
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    // A code not yet exchanged when its kind is turned off is refused, and makes nothing.
    assert_eq!(choose(false).await.status, 200);
    let exchanged = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(
        (exchanged.status, s(&exchanged.body, "error")),
        (400, "invalid_grant")
    );
    assert!(live_apps(&server, &owner).await.is_empty());
    assert_eq!(choose(true).await.status, 200);
    let exchanged = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(exchanged.status, 200, "{}", exchanged.body);
    // An app that registered itself is held to its own kind the same way.
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let registered_app = registered(&server, "Cursor", cursor).await;
    let request = server.requested(&registered_app, cursor).await;
    let local_off = server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"local","allowed":false}),
        )
        .await;
    assert_eq!(local_off.status, 200);
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Cursor"),
        )
        .await;
    assert_eq!(s(&refused.body, "error"), "app_not_allowed");
}

#[tokio::test]
async fn only_disabling_or_enabling_a_dsp_asks_the_owner_to_verify_again() {
    let server = Server::paired().await;
    let earlier = server.signed_in(OWNER, db::now() - DAY).await;
    let north = server.dsp("Northline Logistics").await;
    let key = |name: &str| json!({"name":name,"allDsps":true,"dsps":[],"access":"read","allTools":true,"tools":{},"expiresAt":null});
    let request = server
        .requested(CLAUDE_CODE, "http://localhost:50022/callback")
        .await;
    // A day after signing in, the owner still makes and changes a key, and approves an app.
    let made = server
        .as_owner(&earlier, "POST", "/api/platform/agents/keys", key("Desk"))
        .await;
    assert_eq!(made.status, 200, "{}", made.body);
    let id = s(&made.body["key"], "id").to_owned();
    for (path, body) in [
        (format!("/api/platform/agents/keys/{id}"), key("Desk")),
        (
            format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Laptop"),
        ),
    ] {
        let answer = server.as_owner(&earlier, "POST", &path, body).await;
        assert_eq!(answer.status, 200, "{path}: {}", answer.body);
    }
    // Disabling or enabling a DSP waits until they verify, as the account's own security does.
    for (path, body) in [
        (format!("/api/platform/dsps/{north}/disable"), json!({})),
        (format!("/api/platform/dsps/{north}/enable"), json!({})),
        (
            "/api/auth/security/authenticator/register/start".to_owned(),
            json!({}),
        ),
    ] {
        let stale = server.as_owner(&earlier, "POST", &path, body).await;
        assert_eq!(
            (stale.status, s(&stale.body, "error")),
            (403, "sign_in_again"),
            "{path}"
        );
    }
}
