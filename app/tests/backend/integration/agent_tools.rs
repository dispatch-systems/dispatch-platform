//! Agent keys: a key is shown once and kept as a hash, reaches only what it was given, and
//! stops the moment it is revoked, expires, or its maker stops being an active platform
//! owner. Nothing about keys appears in a DSP's activity log.
use common::{audits, bootstrapped};
use dispatch_core::testing as common;
use dispatch_core::{
    db::{self, Store, s},
    foundation::crypto,
};
use dispatch_mcp::{
    KeyStore,
    api::types::{AgentAccess, AgentKeyRequest},
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn request(value: Value) -> AgentKeyRequest {
    AgentKeyRequest::parse(&value).unwrap()
}

fn reach(dsps: &[&str]) -> Value {
    json!({"name":"Laptop – Claude Code","allDsps":dsps.is_empty(),"dsps":dsps,
        "access":"read","allTools":true,"tools":[],"expiresAt":null})
}
fn owner(db: &Store) -> String {
    s(
        &db.platform
            .one("SELECT id FROM users WHERE email='owner@example.test'", [])
            .unwrap()
            .unwrap(),
        "id",
    )
    .to_owned()
}
fn refused(db: &Store, token: &str) -> String {
    db.authenticate_agent(token, "curl 8.5").unwrap_err().code
}

/// Replay the policy snapshot left by admission, after a committed policy change.
async fn admitted_mcp(
    state: &std::sync::Arc<dispatch_core::State>,
    caller: &dispatch_mcp::Caller,
    method: &str,
    params: Value,
) -> Value {
    admitted_mcp_version(state, caller, method, params, "2025-03-26").await
}

async fn admitted_mcp_version(
    state: &std::sync::Arc<dispatch_core::State>,
    caller: &dispatch_mcp::Caller,
    method: &str,
    params: Value,
    protocol: &str,
) -> Value {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/mcp")
        .header("host", "dispatch.test")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", protocol)
        .body(Body::from(
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(state.clone());
    request.extensions_mut().insert(caller.clone());
    let response = dispatch_mcp::server::serve(request).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), 100_000).await.unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn modern_mcp_exposes_a_stable_strict_profile_and_structured_results() {
    dispatch_backend::install();
    use dispatch_core::State;
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let caller = db.authenticate_agent(&made.token, "test").unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();

    let listed = admitted_mcp_version(&state, &caller, "tools/list", json!({}), "2025-06-18").await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let profile = tools
        .iter()
        .find(|tool| tool["name"] == "get_profile")
        .unwrap();
    assert_eq!(profile["inputSchema"]["additionalProperties"], false);
    assert_eq!(profile["outputSchema"]["required"], json!(["id", "name"]));
    assert_eq!(profile["outputSchema"]["additionalProperties"], false);
    assert_eq!(profile["_meta"]["openai/profile"], true);
    assert_eq!(
        profile["_meta"]["securitySchemes"],
        json!([{"type":"oauth2","scopes":["dispatch"]}])
    );

    let first = admitted_mcp_version(
        &state,
        &caller,
        "tools/call",
        json!({"name":"get_profile","arguments":{}}),
        "2025-06-18",
    )
    .await;
    let second = admitted_mcp_version(
        &state,
        &caller,
        "tools/call",
        json!({"name":"get_profile","arguments":{}}),
        "2025-06-18",
    )
    .await;
    let profile_value = &first["result"]["structuredContent"];
    assert_ne!(profile_value["id"], user);
    assert!(
        profile_value["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("profile_") && id.len() > "profile_".len())
    );
    assert_eq!(profile_value["name"], "Test Owner");
    assert_eq!(profile_value, &second["result"]["structuredContent"]);
    assert_eq!(
        serde_json::from_str::<Value>(first["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        *profile_value
    );

    let who = admitted_mcp_version(
        &state,
        &caller,
        "tools/call",
        json!({"name":"whoami","arguments":{}}),
        "2025-06-18",
    )
    .await;
    assert_eq!(
        who["result"]["structuredContent"]["key"]["name"],
        made.key.name
    );
    assert_eq!(
        serde_json::from_str::<Value>(who["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        who["result"]["structuredContent"]
    );

    let refused = admitted_mcp_version(
        &state,
        &caller,
        "tools/call",
        json!({"name":"get_profile","arguments":{"ignored":true}}),
        "2025-06-18",
    )
    .await;
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("invalid_input:")
    );
}

#[tokio::test]
async fn mcp_revalidates_admitted_keys_before_protected_reads_and_discovery() {
    dispatch_backend::install();
    use dispatch_core::State;
    for change in ["revoked", "expired", "inactive_owner", "demoted_owner"] {
        let (_root, db, dsp) = bootstrapped();
        let user = owner(&db);
        let made = db
            .create_agent_key(&user, &request(reach(&[&dsp])))
            .unwrap();
        let caller = db.authenticate_agent(&made.token, "test").unwrap();
        let config = db.config.clone();
        drop(db);
        let state = State::new(config).unwrap();
        let normal = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
        assert_ne!(normal["result"]["isError"], true, "{normal}");
        assert!(
            normal["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(&dsp)
        );
        state
            .run(move |db| {
                match change {
                    "revoked" => {
                        db.revoke_agent_key(&user, &made.key.id)?;
                    }
                    "expired" => {
                        db.platform.exec(
                            "UPDATE agent_keys SET expires_at=? WHERE id=?",
                            [db::at(db::now() - 1000), made.key.id],
                        )?;
                    }
                    "inactive_owner" => {
                        db.platform
                            .exec("UPDATE users SET status='disabled' WHERE id=?", [&user])?;
                    }
                    _ => {
                        db.platform
                            .exec("UPDATE users SET platform_owner=0 WHERE id=?", [&user])?;
                    }
                }
                Ok(())
            })
            .await
            .unwrap();
        let code = match change {
            "revoked" => "agent_key_revoked",
            "expired" => "agent_key_expired",
            _ => "agent_key_invalid",
        };
        let denied = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
        assert_eq!(denied["result"]["isError"], true, "{change}: {denied}");
        assert!(
            denied["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with(code),
            "{denied}"
        );
        let listed = admitted_mcp(&state, &caller, "tools/list", json!({})).await;
        assert_eq!(listed["error"]["message"], code, "{listed}");
        assert!(listed["result"].is_null());
    }
}

#[tokio::test]
async fn mcp_uses_current_dsp_reach_for_an_admitted_caller() {
    dispatch_backend::install();
    use dispatch_core::State;
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let second = db.new_dsp("Other DSP", "UTC", &user, false).unwrap().id;
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let caller = db.authenticate_agent(&made.token, "test").unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let reached = |who: &Value| -> Vec<String> {
        let value: Value =
            serde_json::from_str(who["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        value["dsps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|dsp| s(dsp, "id").to_owned())
            .collect()
    };
    let who = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
    assert_eq!(reached(&who), std::slice::from_ref(&dsp));
    // The owner gives the key another DSP instead: the admitted caller reaches only that.
    let (actor, key, granted) = (user.clone(), made.key.id.clone(), second.clone());
    state
        .run(move |db| {
            db.update_agent_key(&actor, &key, &request(reach(&[&granted])))?;
            Ok(())
        })
        .await
        .unwrap();
    let who = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
    assert_eq!(reached(&who), std::slice::from_ref(&second));
    // Suspended, it is reached no longer.
    state
        .run(move |db| {
            db.platform
                .exec("UPDATE dsps SET status='suspended' WHERE id=?", [&second])?;
            Ok(())
        })
        .await
        .unwrap();
    let who = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
    assert!(reached(&who).is_empty());
}

#[test]
fn a_key_is_shown_once_kept_as_a_hash_and_reaches_only_its_dsps() {
    dispatch_backend::install();
    let (_root, db, first) = bootstrapped();
    let user = owner(&db);
    let second = db
        .new_dsp("Cedar Ridge Delivery", "America/Chicago", &user, false)
        .unwrap()
        .id;
    let made = db
        .create_agent_key(&user, &request(reach(&[&first])))
        .unwrap();
    assert!(made.token.starts_with("dsk_dev_"));
    assert_eq!(made.key.hint, made.token[made.token.len() - 4..]);
    assert_eq!(made.key.dsps, std::slice::from_ref(&first));
    // Only a hash is kept: the key itself is nowhere in the database.
    let row = db
        .platform
        .one("SELECT * FROM agent_keys WHERE id=?", [&made.key.id])
        .unwrap()
        .unwrap();
    assert_eq!(s(&row, "hash"), crypto::sha(&made.token));
    assert!(!row.to_string().contains(&made.token));

    let caller = db.authenticate_agent(&made.token, "curl 8.5").unwrap();
    assert_eq!(caller.access, AgentAccess::Read);
    let ids: Vec<&str> = caller.dsps.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, [first.as_str()]);
    let whoami = db.agent_whoami(&caller).unwrap();
    assert_eq!(whoami.dsps.len(), 1);
    assert_eq!(whoami.key.name, "Laptop – Claude Code");
    assert_eq!(whoami.dsps[0].today.len(), 10);

    // Every DSP, those made later included, once the key reaches them all.
    db.update_agent_key(&user, &made.key.id, &request(reach(&[])))
        .unwrap();
    let all = db.authenticate_agent(&made.token, "curl 8.5").unwrap();
    assert_eq!(all.dsps.len(), 2);
    // A suspended DSP is out of reach until it is active again.
    db.platform
        .exec("UPDATE dsps SET status='suspended' WHERE id=?", [&second])
        .unwrap();
    let ids: Vec<String> = db
        .authenticate_agent(&made.token, "curl 8.5")
        .unwrap()
        .dsps
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, [first]);
}

#[test]
fn revoked_expired_or_orphaned_keys_stop_at_once() {
    dispatch_backend::install();
    let (_root, mut db, dsp) = bootstrapped();
    let user = owner(&db);
    // A key made where the platform runs as production, stored like any other. It reaches every
    // DSP, since none of this platform's is production's to name.
    let preview = std::mem::replace(&mut db.config.environment, "production".into());
    let live = db.create_agent_key(&user, &request(reach(&[]))).unwrap();
    db.config.environment = preview;
    assert!(live.token.starts_with("dsk_live_"));
    let key = |name: &str| {
        let mut body = reach(&[&dsp]);
        body["name"] = json!(name);
        db.create_agent_key(&user, &request(body)).unwrap()
    };
    assert_eq!(refused(&db, ""), "agent_key_required");
    assert_eq!(refused(&db, "dsk_dev_not-a-key"), "agent_key_invalid");
    let revoked = key("Revoked");
    let mut typo = revoked.token.clone().into_bytes();
    typo[10] = if typo[10] == b'a' { b'b' } else { b'a' };
    assert_eq!(
        refused(&db, &String::from_utf8(typo).unwrap()),
        "agent_key_invalid"
    );
    // A production key is never one here, though this database holds it.
    assert_eq!(refused(&db, &live.token), "agent_key_invalid");
    db.revoke_agent_key(&user, &live.key.id).unwrap();
    db.revoke_agent_key(&user, &revoked.key.id).unwrap();
    assert_eq!(refused(&db, &revoked.token), "agent_key_revoked");

    let expired = key("Expired");
    db.platform
        .exec(
            "UPDATE agent_keys SET expires_at=? WHERE id=?",
            [db::at(db::now() - 1000), expired.key.id.clone()],
        )
        .unwrap();
    assert_eq!(refused(&db, &expired.token), "agent_key_expired");

    // A key is never stronger than its maker.
    let orphaned = key("Orphaned");
    db.platform
        .exec("UPDATE users SET platform_owner=0 WHERE id=?", [&user])
        .unwrap();
    assert_eq!(refused(&db, &orphaned.token), "agent_key_invalid");
    db.platform
        .exec("UPDATE users SET platform_owner=1 WHERE id=?", [&user])
        .unwrap();
    db.authenticate_agent(&orphaned.token, "curl 8.5").unwrap();

    // Revoking everything ends every key still in use, and only those.
    assert_eq!(db.revoke_agent_keys(Some(&user), None).unwrap(), 2);
    assert_eq!(refused(&db, &orphaned.token), "agent_key_revoked");
    assert_eq!(db.revoke_agent_keys(Some(&user), None).unwrap(), 0);
}

#[test]
fn names_dsps_and_expiries_are_checked() {
    dispatch_backend::install();
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let mut same = reach(&[&dsp]);
    same["name"] = json!("laptop – claude code");
    assert_eq!(
        db.create_agent_key(&user, &request(same.clone()))
            .unwrap_err()
            .code,
        "agent_key_name_taken"
    );
    // A revoked key's name is free again.
    db.revoke_agent_key(&user, &made.key.id).unwrap();
    db.create_agent_key(&user, &request(same)).unwrap();
    let mut elsewhere = reach(&["dsp_unknown"]);
    elsewhere["name"] = json!("Elsewhere");
    assert_eq!(
        db.create_agent_key(&user, &request(elsewhere))
            .unwrap_err()
            .code,
        "invalid_input"
    );
    // Every DSP and a list of them, or an unknown field, is no request at all.
    let mut both = reach(&[&dsp]);
    both["allDsps"] = json!(true);
    assert!(AgentKeyRequest::parse(&both).is_err());
    let mut extra = reach(&[&dsp]);
    extra["admin"] = json!(true);
    assert!(AgentKeyRequest::parse(&extra).is_err());
    let mut past = reach(&[&dsp]);
    past["name"] = json!("Past");
    past["expiresAt"] = json!("2020-01-01T00:00:00Z");
    assert_eq!(
        db.create_agent_key(&user, &request(past)).unwrap_err().code,
        "invalid_expiry"
    );
    // An expiry left as it was stays, however close it is.
    let mut later = reach(&[&dsp]);
    later["name"] = json!("Later");
    let soon = db.create_agent_key(&user, &request(later.clone())).unwrap();
    let close = db::at(db::now() + 30_000);
    db.platform
        .exec(
            "UPDATE agent_keys SET expires_at=? WHERE id=?",
            [close.clone(), soon.key.id.clone()],
        )
        .unwrap();
    later["name"] = json!("Later still");
    later["expiresAt"] = json!(close);
    let renamed = db
        .update_agent_key(&user, &soon.key.id, &request(later))
        .unwrap();
    assert_eq!(renamed.name, "Later still");
    assert_eq!(renamed.expires_at.as_deref(), Some(close.as_str()));
    // A revoked key stays revoked.
    assert_eq!(
        db.update_agent_key(&user, &made.key.id, &request(reach(&[&dsp])))
            .unwrap_err()
            .code,
        "agent_key_revoked"
    );
}

#[test]
fn keys_are_recorded_on_the_platform_and_never_in_a_dsp() {
    dispatch_backend::install();
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let mut wider = reach(&[]);
    wider["access"] = json!("operator");
    db.update_agent_key(&user, &made.key.id, &request(wider))
        .unwrap();
    db.revoke_agent_key(&user, &made.key.id).unwrap();
    let platform = audits(&db, None).unwrap();
    let actions: Vec<&str> = platform
        .as_array()
        .unwrap()
        .iter()
        .map(|e| s(e, "action"))
        .filter(|a| a.starts_with("agent."))
        .collect();
    assert_eq!(
        actions,
        [
            "agent.key_revoked",
            "agent.key_updated",
            "agent.key_created"
        ]
    );
    let updated = platform
        .as_array()
        .unwrap()
        .iter()
        .find(|e| s(e, "action") == "agent.key_updated")
        .unwrap();
    assert_eq!(s(updated, "target"), "Laptop – Claude Code");
    assert_eq!(s(updated, "area"), "access");
    let dsp_log = audits(&db, Some(&dsp)).unwrap();
    assert!(
        dsp_log
            .as_array()
            .unwrap()
            .iter()
            .all(|e| !s(e, "action").starts_with("agent."))
    );
}

#[test]
fn key_scope_audits_compare_exact_canonical_dsp_sets() {
    dispatch_backend::install();
    let (_root, db, first) = bootstrapped();
    let user = owner(&db);
    let second = db
        .new_dsp("Second Delivery", "UTC", &user, false)
        .unwrap()
        .id;
    let third = db
        .new_dsp("Third Delivery", "UTC", &user, false)
        .unwrap()
        .id;
    let made = db
        .create_agent_key(&user, &request(reach(&[&second, &first])))
        .unwrap();

    // Order and duplicates are normalized before comparison and persistence.
    let no_op = reach(&[&first, &second, &first]);
    let unchanged = db
        .update_agent_key(&user, &made.key.id, &request(no_op))
        .unwrap();
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort();
    assert_eq!(unchanged.dsps, expected);
    assert!(
        audits(&db, None)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .all(|event| s(event, "action") != "agent.key_updated")
    );

    // A same-cardinality substitution is a material change and names both exact sets.
    let changed = db
        .update_agent_key(&user, &made.key.id, &request(reach(&[&third, &second])))
        .unwrap();
    let mut replacement = vec![second.clone(), third.clone()];
    replacement.sort();
    assert_eq!(changed.dsps, replacement);
    let events = audits(&db, None).unwrap();
    let updates: Vec<&Value> = events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| s(event, "action") == "agent.key_updated")
        .collect();
    assert_eq!(updates.len(), 1, "{events}");
    assert_eq!(
        updates[0]["changes"],
        json!([{
            "field":"dsps",
            "from":format!("[{}]", expected.join(", ")),
            "to":format!("[{}]", replacement.join(", ")),
        }])
    );

    // A duplicate-only request becomes a one-DSP scope and logs that effective result.
    let narrowed = db
        .update_agent_key(&user, &made.key.id, &request(reach(&[&third, &third])))
        .unwrap();
    assert_eq!(narrowed.dsps.as_slice(), std::slice::from_ref(&third));
    let events = audits(&db, None).unwrap();
    let newest = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| s(event, "action") == "agent.key_updated")
        .unwrap();
    assert_eq!(
        newest["changes"],
        json!([{
            "field":"dsps",
            "from":format!("[{}]", replacement.join(", ")),
            "to":format!("[{third}]"),
        }])
    );

    // The raw request bound cannot be bypassed by sending one ID hundreds of times.
    let too_many = vec![third; 501];
    let mut oversized = reach(&[]);
    oversized["allDsps"] = json!(false);
    oversized["dsps"] = json!(too_many);
    let error = match AgentKeyRequest::parse(&oversized) {
        Ok(_) => panic!("oversized duplicate scope was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.code, "invalid_input");
}

#[test]
fn last_use_is_written_down_and_never_goes_back() {
    dispatch_backend::install();
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let now = db::now();
    db.record_agent_use(&[(made.key.id.clone(), (now, "Codex 0.157".into()))])
        .unwrap();
    db.record_agent_use(&[(made.key.id.clone(), (now - 60_000, "curl 8.5".into()))])
        .unwrap();
    let listed = db.agent_keys(&HashMap::new()).unwrap();
    let key = &listed.keys[0];
    assert_eq!(key.last_used_at.as_deref(), Some(db::at(now).as_str()));
    assert_eq!(key.last_client.as_deref(), Some("Codex 0.157"));
    // What this process saw since is shown before the scheduler writes it down.
    let seen = HashMap::from([(made.key.id.clone(), (now + 5_000, "Claude Code 2.1".into()))]);
    let fresher = db.agent_keys(&seen).unwrap();
    assert_eq!(
        fresher.keys[0].last_client.as_deref(),
        Some("Claude Code 2.1")
    );
    assert_eq!(fresher.dsps.len(), 1);
}

#[tokio::test]
async fn a_password_reset_ends_its_owners_keys() {
    dispatch_backend::install();
    use dispatch_core::State;
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let version = db::n(
        &db.platform
            .one("SELECT version FROM users WHERE id=?", [&user])
            .unwrap()
            .unwrap(),
        "version",
    );
    let token = crypto::token().unwrap();
    db.platform
        .exec(
            "INSERT INTO resets(hash,user_id,user_version,expires_at) VALUES (?,?,?,?)",
            rusqlite::params![crypto::sha(&token), user, version, db::now() + 60000],
        )
        .unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    state
        .reset_password(
            token,
            "replacement-password-long".into(),
            dispatch_core::foundation::config::Site::Admin,
        )
        .await
        .unwrap();
    let code = state
        .read(move |db| {
            Ok(db
                .authenticate_agent(&made.token, "curl 8.5")
                .unwrap_err()
                .code)
        })
        .await
        .unwrap();
    assert_eq!(code, "agent_key_revoked");
}

#[test]
fn a_suspended_dsp_keeps_its_reach_through_an_edit() {
    dispatch_backend::install();
    let (_root, db, first) = bootstrapped();
    let user = owner(&db);
    let second = db
        .new_dsp("Harbor Route Co", "UTC", &user, false)
        .unwrap()
        .id;
    let chosen = reach(&[&first, &second]);
    let made = db
        .create_agent_key(&user, &request(chosen.clone()))
        .unwrap();
    // Suspended, as a removed DSP is too, the Agents page no longer lists it.
    let status = |status: &str| {
        db.platform
            .exec("UPDATE dsps SET status=? WHERE id=?", [status, &second])
            .unwrap();
    };
    status("suspended");
    let listed = db.agent_keys(&HashMap::new()).unwrap();
    assert!(listed.dsps.iter().all(|dsp| dsp.id != second));

    // Edited with the suspended DSP sent back as it was, the key saves and keeps it.
    let mut renamed = chosen.clone();
    renamed["name"] = json!("Renamed");
    let edited = db
        .update_agent_key(&user, &made.key.id, &request(renamed))
        .unwrap();
    assert_eq!(edited.dsps, made.key.dsps);
    assert_eq!(made.key.dsps.len(), 2);
    // Active again, it is reached, as before it was suspended.
    status("active");
    let caller = db.authenticate_agent(&made.token, "test").unwrap();
    assert!(caller.dsps.iter().any(|dsp| dsp.id == second));

    // While it is suspended, it isn't given to a key.
    status("suspended");
    let mut new = reach(&[&first, &second]);
    new["name"] = json!("New");
    let refused = db.create_agent_key(&user, &request(new)).unwrap_err();
    assert_eq!(refused.code, "invalid_input");
    // Left out of a request, it goes, as any DSP does.
    let narrowed = db
        .update_agent_key(&user, &made.key.id, &request(reach(&[&first])))
        .unwrap();
    assert_eq!(narrowed.dsps, std::slice::from_ref(&first));
}
