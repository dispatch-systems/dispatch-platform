use super::harness::{
    CHATGPT, CHATGPT_REDIRECT, CLAUDE_CODE, Server, VERIFIER, everything, live_apps, logged,
    registered,
};
use crate::support::{Answer, Who};
use dispatch_core::{
    db::{self, s},
    foundation::crypto,
    testing as common,
};
use dispatch_mcp::{
    OAuthStore,
    api::types::ToolLevel,
    piece::Kept,
    toolbox::{Grants, Toolbox},
};
use serde_json::{Value, json};

/// How the owners' email words the tools of an app approved choosing none but those added
/// later: none of those there are, if any, and new ones, to read.
fn tools_line() -> String {
    let toolbox = Toolbox::installed();
    let chosen = toolbox
        .switchable()
        .map(|tool| (tool.name().to_owned(), ToolLevel::Off))
        .collect();
    format!("Tools: {}", toolbox.summary(&Grants { all: true, chosen }))
}

#[tokio::test]
async fn the_owner_approves_once_and_the_code_is_redeemed_once() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    // Claude Code's document lists portless loopback redirects; any port is the same one.
    let local = "http://localhost:61234/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{request}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    assert_eq!(
        shown.body["app"],
        json!({"name":"Claude Code","clientId":CLAUDE_CODE,"known":true,
            "redirectHost":"this computer","redirectScheme":null})
    );
    assert_eq!(shown.body["id"], request);
    assert_eq!(shown.body["replaces"], Value::Null);
    assert!(s(&shown.body, "expiresAt") > db::iso().as_str());
    // Nobody else sees or answers it.
    assert_eq!(server.get(&path).await.status, 401);
    let mut bad = everything("Claude Code");
    bad["access"] = json!("operator");
    let refused = server
        .as_owner(&owner, "POST", &format!("{path}/approve"), bad)
        .await;
    assert_eq!(refused.status, 400, "{}", refused.body);
    let held = server.browser();
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    // Answered, the request is gone: the browser no longer holds it, and even its old
    // cookie finds nothing.
    assert!(server.browser().is_empty());
    let answer = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(
        (answer.status, s(&answer.body, "error")),
        (403, "wrong_browser")
    );
    server.set_browser(&held);
    for (method, suffix, body) in [
        ("GET", "", json!({})),
        ("POST", "/approve", everything("Again")),
        ("POST", "/deny", json!({})),
    ] {
        let answer = server
            .as_owner(&owner, method, &format!("{path}{suffix}"), body)
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (404, "authorization_not_found"),
            "{method} {suffix}"
        );
    }
    // The code is bound to the client, the exact redirect and the verifier.
    for (client, redirect, verifier) in [
        (CHATGPT, local, VERIFIER),
        (CLAUDE_CODE, "http://localhost:61235/callback", VERIFIER),
        (CLAUDE_CODE, local, &"x".repeat(43)[..]),
    ] {
        let answer = server
            .form(
                "/oauth/token",
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", redirect),
                    ("code_verifier", verifier),
                    ("client_id", client),
                    ("resource", &server.resource()),
                ],
            )
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (400, "invalid_grant"),
            "{client} {redirect}"
        );
    }
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    assert_eq!(tokens.header("cache-control"), "no-store");
    assert_eq!(tokens.header("pragma"), "no-cache");
    assert!(
        tokens
            .header("content-type")
            .starts_with("application/json")
    );
    let access = s(&tokens.body, "access_token").to_owned();
    assert!(access.starts_with("dsa_dev_"));
    assert!(s(&tokens.body, "refresh_token").starts_with("dsr_dev_"));
    assert_eq!(
        (
            &tokens.body["token_type"],
            &tokens.body["expires_in"],
            &tokens.body["scope"]
        ),
        (&json!("Bearer"), &json!(3600), &json!("dispatch"))
    );
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
    // The connected app is listed with the keys, as an app, and logged on the platform.
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let app = &listed.body["keys"][0];
    assert_eq!(app["kind"], "app");
    assert_eq!(
        app["client"],
        json!({"name":"Claude Code","known":true,"status":"connected"})
    );
    assert_eq!(
        (app["name"].as_str(), app["hint"].as_str()),
        (Some("Claude Code"), Some(""))
    );
    let log = server
        .state
        .read(|db| common::audits(db, None))
        .await
        .unwrap();
    assert_eq!(s(&log[0], "action"), "agent.app_connected");
    assert_eq!(s(&log[0], "target"), "Claude Code");
    // Redeemed again with everything right, the code ends what it made.
    let again = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(s(&again.body, "error"), "invalid_grant");
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 401);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(listed.body["keys"][0]["revokedAt"].is_string());
}

#[tokio::test]
async fn connecting_an_app_again_replaces_its_earlier_connection() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50002/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    // Unused for 30 days, the app is signed out but still listed and counted.
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_tokens SET expires_at=? WHERE kind='refresh'",
                [db::at(db::now() - 1000)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let apps = |listed: &Answer| -> Vec<Value> {
        listed.body["keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|key| key["kind"] == "app")
            .cloned()
            .collect()
    };
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let earlier = apps(&listed)[0].clone();
    assert_eq!(earlier["client"]["status"], "signed_out");
    assert!(earlier["revokedAt"].is_null());
    // Connected again under the same name, whatever its case: the earlier one gives way.
    let second = server
        .connect(&owner, CLAUDE_CODE, local, everything("claude code"))
        .await;
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&first, "access_token"))
            .await
            .status,
        401
    );
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second, "access_token"))
            .await
            .status,
        200
    );
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let apps = apps(&listed);
    assert_eq!(apps.len(), 2);
    let live: Vec<&Value> = apps
        .iter()
        .filter(|app| app["revokedAt"].is_null())
        .collect();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0]["name"], "claude code");
    assert_eq!(live[0]["client"]["status"], "connected");
    assert_ne!(live[0]["id"], earlier["id"]);
    let ended = apps.iter().find(|app| app["id"] == earlier["id"]).unwrap();
    assert!(ended["revokedAt"].is_string());
    // The log says the owner replaced it, in the step that connected its successor.
    let log = server
        .state
        .read(|db| common::audits(db, None))
        .await
        .unwrap();
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|event| s(event, "action"))
        .filter(|action| action.starts_with("agent."))
        .collect();
    // Newest first, after the owner opened the pairing window for the first connection: each
    // approval as the owner gave it, and each connection as the app collected its tokens.
    assert_eq!(
        actions,
        [
            "agent.app_connected",
            "agent.app_revoked",
            "agent.app_approved",
            "agent.app_connected",
            "agent.app_approved",
            "agent.pairing_opened"
        ]
    );
    let revoked = log
        .as_array()
        .unwrap()
        .iter()
        .find(|event| s(event, "action") == "agent.app_revoked")
        .unwrap();
    assert_eq!(s(revoked, "target"), "Claude Code");
    assert!(revoked["actorId"].is_string());
    assert_eq!(
        revoked["changes"],
        json!([{"field":"reason","from":null,"to":"replaced"}])
    );
}

#[tokio::test]
async fn a_name_a_key_or_another_app_holds_stays_taken() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50003/callback";
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Laptop agent"))
        .await;
    let key = server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/agents/keys",
            json!({"name":"Desk key","allDsps":true,"dsps":[],"access":"read",
                "allTools":true,"tools":{},"expiresAt":null}),
        )
        .await;
    assert_eq!(key.status, 200, "{}", key.body);
    for (client, redirect, name) in [
        (CHATGPT, CHATGPT_REDIRECT, "laptop agent"),
        (CLAUDE_CODE, local, "Desk key"),
    ] {
        let request = server.requested(client, redirect).await;
        let refused = server
            .as_owner(
                &owner,
                "POST",
                &format!("/api/platform/oauth/requests/{request}/approve"),
                everything(name),
            )
            .await;
        assert_eq!(
            (refused.status, s(&refused.body, "error")),
            (409, "agent_key_name_taken"),
            "{client} {name}"
        );
    }
    // Nothing was replaced.
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(
        listed.body["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|key| key["revokedAt"].is_null())
    );
}

#[tokio::test]
async fn an_app_that_registers_again_replaces_its_earlier_connection() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let first = registered(&server, "Cursor", cursor).await;
    let tokens = server
        .connect(&owner, &first, cursor, everything("Cursor"))
        .await;
    // Each registration is a new client; the same name makes it the same app.
    let second = registered(&server, " cursor ", cursor).await;
    assert_ne!(first, second);
    let again = server
        .connect(&owner, &second, cursor, everything("Cursor"))
        .await;
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&tokens, "access_token"))
            .await
            .status,
        401
    );
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&again, "access_token"))
            .await
            .status,
        200
    );
    assert_eq!(
        live_apps(&server, &owner).await,
        [("Cursor".to_owned(), "cursor".to_owned())]
    );
    // Another self-registered app keeps the name taken.
    let other = registered(&server, "Windsurf", cursor).await;
    let request = server.requested(&other, cursor).await;
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Cursor"),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (409, "agent_key_name_taken")
    );
}

#[tokio::test]
async fn an_unrecognized_app_never_replaces_a_known_one() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50004/callback";
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    // It says it is Claude Code, but only a known app's own client replaces Claude Code.
    let pretender = registered(&server, "Claude Code", local).await;
    let request = server.requested(&pretender, local).await;
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
        (409, "agent_key_name_taken")
    );
    // Under a name of its own it connects beside the known one.
    let request = server.requested(&pretender, local).await;
    let code = server
        .approved(&owner, &request, everything("Claude Code (other)"))
        .await;
    assert_eq!(server.exchange(&pretender, local, &code).await.status, 200);
    let mut live = live_apps(&server, &owner).await;
    live.sort();
    assert_eq!(
        live,
        [
            ("Claude Code".to_owned(), "Claude Code".to_owned()),
            ("Claude Code (other)".to_owned(), "Claude Code".to_owned()),
        ]
    );
    // Nor does a known app replace an unrecognized one that took a name.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Claude Code (other)"),
        )
        .await;
    assert_eq!(s(&refused.body, "error"), "agent_key_name_taken");
}

#[tokio::test]
async fn the_approval_page_says_what_approving_would_replace() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50005/callback";
    let fresh = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{fresh}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(shown.body["replaces"], Value::Null);
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let connected_at = {
        let listed = server
            .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
            .await;
        s(&listed.body["keys"][0], "createdAt").to_owned()
    };
    // Under the app's own name, approving replaces that connection; under another, nothing.
    let request = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{request}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(
        shown.body["replaces"],
        json!({"name":"Claude Code","connectedAt":connected_at})
    );
    for (name, replaces) in [
        (
            "claude%20code",
            json!({"name":"Claude Code","connectedAt":connected_at}),
        ),
        ("Laptop", Value::Null),
        ("", json!({"name":"Claude Code","connectedAt":connected_at})),
    ] {
        let asked = server
            .as_owner(&owner, "GET", &format!("{path}?name={name}"), json!({}))
            .await;
        assert_eq!(asked.status, 200, "{}", asked.body);
        assert_eq!(asked.body["replaces"], replaces, "{name}");
    }
    // Another app under that name replaces nothing; approving it is refused instead.
    let other = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{other}?name=Claude%20Code"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["replaces"], Value::Null);
}

#[tokio::test]
async fn a_replay_with_the_wrong_verifier_ends_nothing() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server
        .approved(&owner, &request, everything("ChatGPT"))
        .await;
    let tokens = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let replay = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", CHATGPT_REDIRECT),
                ("code_verifier", &"y".repeat(43)),
                ("client_id", CHATGPT),
                ("resource", &server.resource()),
            ],
        )
        .await;
    assert_eq!(s(&replay.body, "error"), "invalid_grant");
    let access = s(&tokens.body, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
}

#[tokio::test]
async fn refresh_tokens_are_single_use_and_replay_ends_the_family() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://127.0.0.1:61001/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let refresh = s(&first, "refresh_token");
    // Only by the client it was issued to, and only as a refresh token.
    let wrong = server.refresh(CHATGPT, refresh).await;
    assert_eq!(s(&wrong.body, "error"), "invalid_grant");
    let access = server.refresh(CLAUDE_CODE, s(&first, "access_token")).await;
    assert_eq!(s(&access.body, "error"), "invalid_grant");
    let unknown = server.refresh("https://evil.example/c.json", refresh).await;
    assert_eq!(
        (unknown.status, s(&unknown.body, "error")),
        (401, "invalid_client")
    );
    let target = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", CLAUDE_CODE),
                ("resource", &server.origin),
            ],
        )
        .await;
    assert_eq!(s(&target.body, "error"), "invalid_target");
    // Rotated: a new pair, and the old access token still works until it expires.
    let second = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(second.status, 200, "{}", second.body);
    assert_ne!(s(&second.body, "refresh_token"), refresh);
    assert_eq!(second.body["scope"], "dispatch");
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second.body, "access_token"))
            .await
            .status,
        200
    );
    // Reuse is a compromise signal even immediately after rotation: it revokes the connected
    // app and the successor pair instead of minting an independent branch.
    let replay = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(s(&replay.body, "error"), "invalid_grant");
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second.body, "access_token"))
            .await
            .status,
        401
    );
    let gone = server
        .refresh(CLAUDE_CODE, s(&second.body, "refresh_token"))
        .await;
    assert_eq!(s(&gone.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn simultaneous_refreshes_mint_once_and_then_end_the_compromised_family() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://127.0.0.1:61002/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let refresh = s(&first, "refresh_token").to_owned();

    let (left, right) = tokio::join!(
        server.refresh(CLAUDE_CODE, &refresh),
        server.refresh(CLAUDE_CODE, &refresh),
    );
    let mut answers = [left, right];
    answers.sort_by_key(|answer| answer.status);
    assert_eq!(answers[0].status, 200, "{}", answers[0].body);
    assert_eq!(
        (answers[1].status, s(&answers[1].body, "error")),
        (400, "invalid_grant")
    );

    // The losing replay is a compromise signal. Even the pair the winning request briefly
    // minted is unusable by the time both calls return.
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&answers[0].body, "access_token"))
            .await
            .status,
        401
    );
    let successor = server
        .refresh(CLAUDE_CODE, s(&answers[0].body, "refresh_token"))
        .await;
    assert_eq!(s(&successor.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn expired_or_revoked_access_ends_with_invalid_token() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50000/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let access = s(&tokens, "access_token").to_owned();
    // Never as an API key, and never a refresh token as a bearer.
    let keyed = server
        .answer(
            server
                .client
                .get(server.url("/api/v1/whoami"))
                .header("x-api-key", &access),
        )
        .await;
    assert_eq!(keyed.status, 401);
    let refresh = server
        .bearer("/api/v1/whoami", s(&tokens, "refresh_token"))
        .await;
    assert_eq!(refresh.status, 401);
    let hash = crypto::sha(&access);
    server
        .state
        .run(move |db| {
            db.platform.exec(
                "UPDATE oauth_tokens SET expires_at=? WHERE hash=?",
                [db::at(db::now() - 1000), hash],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let expired = server.bearer("/api/v1/mcp", &access).await;
    assert_eq!(
        (expired.status, s(&expired.body, "error")),
        (401, "access_token_expired")
    );
    assert!(
        expired
            .header("www-authenticate")
            .ends_with(", error=\"invalid_token\"")
    );
    // A fresh pair works until the owner revokes the app on the Agents page.
    let fresh = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    let access = s(&fresh.body, "access_token").to_owned();
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let id = s(&listed.body["keys"][0], "id").to_owned();
    let edited = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/agents/keys/{id}"),
            json!({"name":"Claude Code","allDsps":true,"dsps":[],"access":"operator",
                "allTools":true,"tools":{},"expiresAt":null}),
        )
        .await;
    // An app is edited like a key, but only ever reads.
    assert_eq!(
        (edited.status, s(&edited.body, "error")),
        (400, "invalid_input")
    );
    let revoked = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/agents/keys/{id}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(revoked.status, 200, "{}", revoked.body);
    let answer = server.bearer("/api/v1/mcp", &access).await;
    assert_eq!(answer.status, 401);
    assert!(
        answer
            .header("www-authenticate")
            .ends_with(", error=\"invalid_token\"")
    );
    let after = server
        .refresh(CLAUDE_CODE, s(&fresh.body, "refresh_token"))
        .await;
    assert_eq!(s(&after.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn a_connected_app_reaches_only_what_the_owner_chose() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let tokens = server
        .connect(
            &owner,
            CHATGPT,
            CHATGPT_REDIRECT,
            json!({"name":"ChatGPT","allDsps":false,"dsps":[north],"allTools":true,"tools":{}}),
        )
        .await;
    let access = s(&tokens, "access_token");
    let whoami = server.bearer("/api/v1/whoami", access).await;
    assert_eq!(whoami.status, 200, "{}", whoami.body);
    assert_eq!(
        whoami.body["key"],
        json!({"name":"ChatGPT","access":"read","expiresAt":null,
            "tools":{"get_profile":"read","whoami":"read"}})
    );
    let dsps: Vec<&str> = whoami.body["dsps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dsp| s(dsp, "id"))
        .collect();
    assert_eq!(dsps, [north.as_str()]);
    // MCP offers it the tools a key has.
    let listed = server.mcp(access, "tools/list", json!({})).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let tools: Vec<&str> = listed.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| s(tool, "name"))
        .collect();
    assert_eq!(tools, ["get_profile", "whoami"]);
    // Its calls count like a key's, under the connected app.
    let used = Kept::of(&server.state).usage.last();
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let app = &listed.body["keys"][0];
    assert!(used.contains_key(s(app, "id")));
    assert!(app["lastUsedAt"].is_string());
    assert_eq!(app["dsps"], json!([north]));
}

#[tokio::test]
async fn revoking_any_token_ends_the_app_and_unknown_tokens_are_fine() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50001/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    for token in ["dsr_dev_unknown", "anything"] {
        let answer = server
            .form(
                "/oauth/revoke",
                &[("token", token), ("client_id", CLAUDE_CODE)],
            )
            .await;
        assert_eq!(answer.status, 200, "{}", answer.body);
    }
    // A token named by another client is left alone.
    let other = server
        .form(
            "/oauth/revoke",
            &[
                ("token", s(&tokens, "refresh_token")),
                ("client_id", CHATGPT),
            ],
        )
        .await;
    assert_eq!(other.status, 200);
    let access = s(&tokens, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
    // Claude Code signs out with its refresh token, then its access token.
    for token in [s(&tokens, "refresh_token"), access] {
        let answer = server
            .form(
                "/oauth/revoke",
                &[
                    ("token", token),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", CLAUDE_CODE),
                ],
            )
            .await;
        assert_eq!(answer.status, 200);
    }
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 401);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(listed.body["keys"][0]["revokedAt"].is_string());
    // Form posts only, as RFC 6749 says, and never 415.
    let json_body = server
        .answer(
            server
                .client
                .post(server.url("/oauth/token"))
                .header("content-type", "application/json")
                .body("{}"),
        )
        .await;
    assert_eq!(
        (json_body.status, s(&json_body.body, "error")),
        (400, "invalid_request")
    );
    let grant = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "password"),
                ("client_id", CLAUDE_CODE),
                ("resource", &server.resource()),
            ],
        )
        .await;
    assert_eq!(s(&grant.body, "error"), "unsupported_grant_type");
}

#[test]
fn what_can_no_longer_be_used_is_pruned() {
    dispatch_backend::install();
    let (_root, db, _) = common::bootstrapped();
    let old = db::at(db::now() - 2 * 24 * 60 * 60 * 1000);
    let soon = db::at(db::now() + 60_000);
    let ancient = db::at(db::now() - 100 * 24 * 60 * 60 * 1000);
    let owner = s(
        &db.platform
            .one("SELECT id FROM users LIMIT 1", [])
            .unwrap()
            .unwrap(),
        "id",
    )
    .to_owned();
    db.platform
        .exec(
            "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,locations,\
             created_at,kind) VALUES ('app','App','app:app','',?,1,'read','full',0,?,'app')",
            [&owner, &old],
        )
        .unwrap();
    for (id, at) in [("expired", &old), ("waiting", &soon)] {
        db.platform
            .exec(
                "INSERT INTO oauth_requests(id,client_id,client_name,verified,redirect_uri,state,\
                 code_challenge,resource,scope,created_at,expires_at) \
                 VALUES (?,'c','C',1,'r',NULL,'x','res','dispatch',?,?)",
                [id, at.as_str(), at.as_str()],
            )
            .unwrap();
        db.platform
            .exec(
                "INSERT INTO oauth_codes(hash,client_id,client_name,client_verified,redirect_uri,\
                 code_challenge,resource,choices,approved_by,created_at,expires_at) \
                 VALUES (?,'c','C',1,'r','x','res','{}',?,?,?)",
                [id, owner.as_str(), at.as_str(), at.as_str()],
            )
            .unwrap();
        db.platform
            .exec(
                "INSERT INTO oauth_tokens(hash,key_id,kind,resource,created_at,expires_at) \
                 VALUES (?,'app','access','res',?,?)",
                [id, at.as_str(), at.as_str()],
            )
            .unwrap();
    }
    for (id, created, used) in [
        ("dcr_never", Some(&old), None),
        ("dcr_new", Some(&soon), None),
        ("dcr_idle", Some(&ancient), Some(&ancient)),
        ("dcr_used", Some(&ancient), Some(&old)),
    ] {
        db.platform
            .exec(
                "INSERT INTO oauth_clients VALUES (?,'App','[]',?,?)",
                rusqlite::params![id, created, used],
            )
            .unwrap();
    }
    // An unused registration is tied to its short pairing attempt.
    db.platform
        .exec(
            "INSERT INTO oauth_clients VALUES ('dcr_today','App','[]',?,NULL)",
            [db::at(db::now() - 12 * 60 * 60 * 1000)],
        )
        .unwrap();
    db.platform
        .exec(
            "UPDATE oauth_clients SET created_at=? WHERE id='dcr_never'",
            [db::at(db::now() - 25 * 60 * 60 * 1000)],
        )
        .unwrap();
    db.prune_oauth().unwrap();
    let left = |table: &str, column: &str| -> Vec<String> {
        db.platform
            .all(&format!("SELECT {column} FROM {table} ORDER BY 1"), [])
            .unwrap()
            .iter()
            .map(|row| s(row, column).to_owned())
            .collect()
    };
    assert_eq!(left("oauth_requests", "id"), ["waiting"]);
    assert_eq!(left("oauth_codes", "hash"), ["waiting"]);
    assert_eq!(left("oauth_tokens", "hash"), ["waiting"]);
    assert_eq!(left("oauth_clients", "id"), ["dcr_new", "dcr_used"]);
}

/// The notices queued for platform owners about connected apps: recipient, subject, text.
async fn notices(server: &Server) -> Vec<(String, String, String)> {
    let key = server.state.key.clone();
    server
        .state
        .read(move |db| {
            Ok(db
                .platform
                .all(
                    "SELECT id,encrypted_message,user_id FROM outbox WHERE kind='connected_app' \
                     ORDER BY created_at,id",
                    [],
                )?
                .iter()
                .map(|row| {
                    let mail =
                        crypto::decrypt(&key, s(row, "id"), s(row, "encrypted_message")).unwrap();
                    (
                        s(&mail, "to").to_owned(),
                        s(&mail, "subject").to_owned(),
                        s(&mail, "text").to_owned(),
                    )
                })
                .collect())
        })
        .await
        .unwrap()
}

/// The texts of the notices with this subject.
fn told<'a>(sent: &'a [(String, String, String)], subject: &str) -> Vec<&'a str> {
    sent.iter()
        .filter(|(_, sent, _)| sent == subject)
        .map(|(.., text)| text.as_str())
        .collect()
}

#[tokio::test]
async fn platform_owners_hear_when_an_app_connects_and_when_dispatch_ends_one() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    // Every active platform owner hears; a member, or an owner no longer active, does not.
    server
        .state
        .run(|db| {
            for (id, email, owner, status) in [
                ("usr_second", "second@dispatch.test", 1, "active"),
                ("usr_gone", "gone@dispatch.test", 1, "disabled"),
            ] {
                db.platform.exec(
                    "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,\
                     status,created_at) VALUES (?,?,'Other','Owner','x',?,?,?)",
                    rusqlite::params![id, email, owner, status, db::iso()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let north = server.dsp("Northline Logistics").await;
    let local = "http://localhost:50012/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let code = server
        .approved(
            &owner,
            &request,
            json!({"name":"Laptop","allDsps":false,"dsps":[north],"allTools":true,"tools":{}}),
        )
        .await;
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let mut sent = notices(&server).await;
    sent.sort();
    let recipients: Vec<&str> = sent.iter().map(|(to, ..)| to.as_str()).collect();
    assert_eq!(recipients, ["owner@dispatch.test", "second@dispatch.test"]);
    for (to, subject, text) in &sent {
        assert_eq!(subject, "[Dispatch Dev] Claude Code connected to Dispatch");
        for line in [
            "Connection: Laptop",
            "App: Claude Code (recognized app)",
            "Sends access to: this computer",
            "DSPs: Northline Logistics",
            &tools_line(),
            "Approved by: Platform Owner",
            &format!("{}/#agents?tab=apps", server.origin),
            &format!("This notice was sent to {to}"),
        ] {
            assert!(text.contains(line), "{line}\n{text}");
        }
    }
    // Renewing tells nobody; presenting a rotated refresh token again ends the app, and the
    // owners hear why.
    let refresh = s(&tokens.body, "refresh_token").to_owned();
    assert_eq!(server.refresh(CLAUDE_CODE, &refresh).await.status, 200);
    assert_eq!(notices(&server).await.len(), 2);
    let late = server.refresh(CLAUDE_CODE, &refresh).await;
    assert_eq!(s(&late.body, "error"), "invalid_grant");
    let sent = notices(&server).await;
    assert_eq!(sent.len(), 4);
    let ended = told(&sent, "[Dispatch Dev] Dispatch disconnected Claude Code");
    assert_eq!(ended.len(), 2);
    for text in ended {
        assert!(text.contains("already used was presented again"), "{text}");
        assert!(text.contains("Connection: Laptop"), "{text}");
    }
    // A replayed code ends what it made, and they hear that too, once.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server
        .approved(&owner, &request, everything("ChatGPT"))
        .await;
    assert_eq!(
        server
            .exchange(CHATGPT, CHATGPT_REDIRECT, &code)
            .await
            .status,
        200
    );
    for _ in 0..2 {
        let replay = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
        assert_eq!(s(&replay.body, "error"), "invalid_grant");
    }
    let sent = notices(&server).await;
    assert_eq!(sent.len(), 8);
    let ended = told(&sent, "[Dispatch Dev] Dispatch disconnected ChatGPT");
    assert_eq!(ended.len(), 2);
    for text in ended {
        assert!(text.contains("one-time code"), "{text}");
        assert!(text.contains("Sent access to: chatgpt.com"), "{text}");
        assert!(text.contains(&format!("{}\n", tools_line())), "{text}");
    }
    // An app signing out, or the owner revoking one, is no news to them.
    let signed_in = server
        .connect(&owner, CLAUDE_CODE, local, everything("Desktop"))
        .await;
    assert_eq!(notices(&server).await.len(), 10);
    server
        .form(
            "/oauth/revoke",
            &[("token", s(&signed_in, "refresh_token"))],
        )
        .await;
    assert_eq!(notices(&server).await.len(), 10);
}

#[test]
fn a_notice_waiting_to_be_sent_goes_only_to_a_platform_owner_still_active() {
    dispatch_backend::install();
    let (_root, db) = common::seeded();
    let user = |email: &str| {
        let (id,): (String,) = db
            .platform
            .one_as("SELECT id FROM users WHERE email=?", [email])
            .unwrap()
            .unwrap();
        id
    };
    let owner = user("owner@dispatch.test");
    // The member's account is Northline's own.
    let (north,): (String,) = db
        .platform
        .one_as("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let (member,): (String,) = db
        .dsp(&north)
        .unwrap()
        .one_as(
            "SELECT id FROM users WHERE email='member@dispatch.test'",
            [],
        )
        .unwrap()
        .unwrap();
    db.platform
        .exec(
            "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,status,\
             created_at) VALUES ('usr_gone','gone@dispatch.test','Gone','Owner','x',1,\
             'disabled',?)",
            [db::iso()],
        )
        .unwrap();
    for (id, to) in [
        ("mail_owner", owner.as_str()),
        ("mail_member", member.as_str()),
        ("mail_gone", "usr_gone"),
    ] {
        db.platform
            .exec(
                "INSERT INTO outbox(id,encrypted_message,available_at,created_at,kind,user_id) \
                 VALUES (?1,'',?3,?3,'connected_app',?2)",
                rusqlite::params![id, to, db::now()],
            )
            .unwrap();
    }
    dispatch_core::server::mail::discard_stale(&db).unwrap();
    let (left,): (String,) = db
        .platform
        .one_as("SELECT group_concat(id) FROM outbox", [])
        .unwrap()
        .unwrap();
    assert_eq!(left, "mail_owner");
}

/// What the Agents page lists of one key or app, by name.
async fn listed_key(server: &Server, owner: &Who, name: &str) -> Value {
    let listed = server
        .as_owner(owner, "GET", "/api/platform/agents", json!({}))
        .await;
    listed.body["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["name"] == name && key["revokedAt"].is_null())
        .unwrap_or_else(|| panic!("no {name} in {}", listed.body))
        .clone()
}

/// The choices an approval's code carries until the app redeems it.
async fn stored_choices(server: &Server, code: &str) -> Value {
    let hash = crypto::sha(code);
    let row = server
        .state
        .read(move |db| {
            db.platform
                .one("SELECT choices FROM oauth_codes WHERE hash=?", [hash])
        })
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(s(&row, "choices")).unwrap()
}

#[tokio::test]
async fn an_approval_keeps_its_choices_and_one_from_before_still_connects() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let local = "http://localhost:50020/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let code = server
        .approved(
            &owner,
            &request,
            json!({"name":"Laptop","allDsps":false,"dsps":[north],"allTools":true,"tools":{}}),
        )
        .await;
    // The code carries the choices until the app redeems it; an older release redeeming one
    // in a rollback reads that it may read nothing, and uses the tools of `agent_tools`.
    assert_eq!(
        stored_choices(&server, &code).await,
        json!({"name":"Laptop","all_dsps":false,"dsps":[north],"all_tools":true,
            "tool_levels":{},"agent_tools":[],"reads":{"areas":[],"bypass":false},
            "tools":"full","locations":false})
    );
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let app = listed_key(&server, &owner, "Laptop").await;
    assert_eq!(
        (&app["allDsps"], &app["dsps"]),
        (&json!(false), &json!([north]))
    );

    // A code an older release approved, with what it read, connects as chosen.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server.approved(&owner, &request, everything("Desk")).await;
    let old = json!({"name":"Desk","all_dsps":true,"dsps":[],"tools":"essential",
        "locations":true,"reads":{"areas":["routes"],"bypass":true}})
    .to_string();
    let hash = crypto::sha(&code);
    server
        .state
        .run(move |db| {
            db.platform
                .exec("UPDATE oauth_codes SET choices=? WHERE hash=?", [old, hash])?;
            Ok(())
        })
        .await
        .unwrap();
    let tokens = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let app = listed_key(&server, &owner, "Desk").await;
    assert_eq!(
        (&app["allDsps"], &app["access"]),
        (&json!(true), &json!("read"))
    );
}

#[tokio::test]
async fn a_connected_app_is_edited_like_a_key_but_only_ever_reads_and_never_expires() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let summit = server.dsp("Summit Delivery").await;
    let local = "http://localhost:50021/callback";
    let tokens = server
        .connect(
            &owner,
            CLAUDE_CODE,
            local,
            json!({"name":"Laptop","allDsps":false,"dsps":[north],"allTools":true,"tools":{}}),
        )
        .await;
    let access = s(&tokens, "access_token").to_owned();
    let id = s(&listed_key(&server, &owner, "Laptop").await, "id").to_owned();
    let path = format!("/api/platform/agents/keys/{id}");
    let edit = |change: Value| {
        let mut body = json!({"name":"Laptop","allDsps":false,"dsps":[north],"access":"read",
            "allTools":true,"tools":{},"expiresAt":null});
        for (field, value) in change.as_object().unwrap() {
            body[field] = value.clone();
        }
        body
    };
    // Never an operator, and never an expiry.
    for refused in [
        json!({"access":"operator"}),
        json!({"expiresAt":"2030-01-01T00:00:00Z"}),
    ] {
        let answer = server
            .as_owner(&owner, "POST", &path, edit(refused.clone()))
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (400, "invalid_input"),
            "{refused}"
        );
    }
    // Renamed and reaching another DSP: the next call reaches it.
    let edited = server
        .as_owner(
            &owner,
            "POST",
            &path,
            edit(json!({"name":"Laptop – Claude Code","dsps":[north, summit]})),
        )
        .await;
    assert_eq!(edited.status, 200, "{}", edited.body);
    assert_eq!(edited.body["kind"], "app");
    let whoami = server.bearer("/api/v1/whoami", &access).await;
    let reached: Vec<&str> = whoami.body["dsps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dsp| s(dsp, "name"))
        .collect();
    assert_eq!(reached, ["Northline Logistics", "Summit Delivery"]);
    assert_eq!(
        logged(&server, "agent.app_updated").await,
        [(
            "agent.app_updated".to_owned(),
            "Laptop – Claude Code".to_owned()
        )]
    );
}

#[tokio::test]
async fn an_app_stops_renewing_and_connecting_once_its_owner_is_no_platform_owner() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://127.0.0.1:61007/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Laptop"))
        .await;
    // An approval waiting to be redeemed, from the same owner.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server.approved(&owner, &request, everything("Desk")).await;
    let standing = |platform_owner: i64| {
        let state = server.state.clone();
        async move {
            state
                .run(move |db| {
                    db.platform.exec(
                        "UPDATE users SET platform_owner=? WHERE email='owner@dispatch.test'",
                        [platform_owner],
                    )?;
                    Ok(())
                })
                .await
                .unwrap()
        }
    };
    standing(0).await;
    let refused = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    assert_eq!(
        (
            s(&refused.body, "error"),
            s(&refused.body, "error_description")
        ),
        ("invalid_grant", "The connection was ended"),
        "{}",
        refused.body
    );
    let redeemed = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
    assert_eq!(
        (
            s(&redeemed.body, "error"),
            s(&redeemed.body, "error_description")
        ),
        ("invalid_grant", "The approval no longer stands"),
        "{}",
        redeemed.body
    );
    // A platform owner again, the app renews as it stood: the refusal spent nothing.
    standing(1).await;
    let renewed = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
}
