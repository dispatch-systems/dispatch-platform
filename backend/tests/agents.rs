//! Agent keys: a key is shown once and kept as a hash, reaches only what it was given, and
//! stops the moment it is revoked, expires, or its maker stops being an active platform
//! owner. Nothing about keys appears in a DSP's activity log.
mod common;
use common::{audits, bootstrapped};
use dispatch_backend::{
    contracts::{AgentAccess, AgentKeyRequest},
    crypto,
    db::{self, Store, s},
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn request(value: Value) -> AgentKeyRequest {
    AgentKeyRequest::parse(&value).unwrap()
}
fn reach(dsps: &[&str]) -> Value {
    json!({"name":"Laptop – Claude Code","allDsps":dsps.is_empty(),"dsps":dsps,
        "access":"read","tools":"full","locations":false,"expiresAt":null})
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
    state: &std::sync::Arc<dispatch_backend::State>,
    caller: &dispatch_backend::agents::Caller,
    method: &str,
    params: Value,
) -> Value {
    admitted_mcp_version(state, caller, method, params, "2025-03-26").await
}

async fn admitted_mcp_version(
    state: &std::sync::Arc<dispatch_backend::State>,
    caller: &dispatch_backend::agents::Caller,
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
    let response = dispatch_backend::agents::mcp::serve(request).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), 100_000).await.unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn modern_mcp_exposes_a_stable_strict_profile_and_structured_results() {
    use dispatch_backend::State;
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
    assert_eq!(profile["outputSchema"]["required"], json!(["id"]));
    assert_eq!(profile["outputSchema"]["additionalProperties"], false);
    assert_eq!(profile["_meta"]["openai/profile"], true);
    assert_eq!(
        profile["_meta"]["securitySchemes"],
        json!([{"type":"oauth2","scopes":["dispatch"]}])
    );
    let find = tools
        .iter()
        .find(|tool| tool["name"] == "find_drivers")
        .unwrap();
    assert_eq!(find["inputSchema"]["additionalProperties"], false);
    assert_eq!(find["inputSchema"]["properties"]["q"]["maxLength"], 200);
    assert_eq!(find["outputSchema"]["type"], "object");

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
            .starts_with("unknown_parameter:")
    );
}

#[tokio::test]
async fn mcp_revalidates_admitted_keys_before_protected_reads_and_discovery() {
    use dispatch_backend::State;
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
async fn mcp_uses_current_toolset_and_dsp_reach_for_an_admitted_caller() {
    use dispatch_backend::State;
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
    let made = db
        .create_agent_key(&user, &request(reach(&[&dsp])))
        .unwrap();
    let caller = db.authenticate_agent(&made.token, "test").unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let full = admitted_mcp(&state, &caller, "tools/list", json!({})).await;
    let tool = dispatch_backend::agents::data::catalog::ENDPOINTS
        .iter()
        .find(|tool| !tool.essential)
        .unwrap()
        .tool;
    assert!(
        full["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == tool)
    );
    state
        .run(move |db| {
            let mut body = reach(&[&dsp]);
            body["tools"] = json!("essential");
            db.update_agent_key(&user, &made.key.id, &request(body))?;
            db.platform
                .exec("UPDATE dsps SET status='suspended' WHERE id=?", [&dsp])?;
            Ok(())
        })
        .await
        .unwrap();
    let listed = admitted_mcp(&state, &caller, "tools/list", json!({})).await;
    assert!(
        !listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == tool)
    );
    let hidden = admitted_mcp(&state, &caller, "tools/call", json!({"name":tool})).await;
    assert_eq!(hidden["result"]["isError"], true);
    assert!(
        hidden["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("unknown_tool:")
    );
    let who = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
    let current: Value =
        serde_json::from_str(who["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(current["dsps"], json!([]));
}

#[tokio::test]
async fn mcp_rechecks_location_policy_and_changed_dsp_grants() {
    use dispatch_backend::State;
    let (_root, db, dsp) = bootstrapped();
    db.enable_all_features(&dsp).unwrap();
    let user = owner(&db);
    let second = db.new_dsp("Other DSP", "UTC", &user, false).unwrap().id;
    let mut body = reach(&[&dsp]);
    body["locations"] = json!(true);
    let made = db.create_agent_key(&user, &request(body.clone())).unwrap();
    let caller = db.authenticate_agent(&made.token, "test").unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let normal = admitted_mcp(
        &state,
        &caller,
        "tools/call",
        json!({"name":"packages","arguments":{"date":"2026-09-12","list":true}}),
    )
    .await;
    let value: Value =
        serde_json::from_str(normal["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        value["list"]["columns"]
            .as_array()
            .unwrap()
            .contains(&json!("address")),
        "{normal}"
    );
    body["locations"] = json!(false);
    let key = made.key.id.clone();
    let actor = user.clone();
    state
        .run(move |db| {
            db.update_agent_key(&actor, &key, &request(body))?;
            Ok(())
        })
        .await
        .unwrap();
    let current = admitted_mcp(
        &state,
        &caller,
        "tools/call",
        json!({"name":"packages","arguments":{"date":"2026-09-12","list":true}}),
    )
    .await;
    let value: Value =
        serde_json::from_str(current["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        !value["list"]["columns"]
            .as_array()
            .unwrap()
            .contains(&json!("address")),
        "{current}"
    );
    let grouping = admitted_mcp(
        &state,
        &caller,
        "tools/call",
        json!({"name":"packages","arguments":{"date":"2026-09-12","group_by":"address"}}),
    )
    .await;
    assert_eq!(grouping["result"]["isError"], true, "{grouping}");
    assert!(
        grouping["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("locations_off:"),
        "{grouping}"
    );
    let granted = second.clone();
    state
        .run(move |db| {
            db.update_agent_key(&user, &made.key.id, &request(reach(&[&granted])))?;
            Ok(())
        })
        .await
        .unwrap();
    let who = admitted_mcp(&state, &caller, "tools/call", json!({"name":"whoami"})).await;
    let value: Value =
        serde_json::from_str(who["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["dsps"][0]["id"], second);
    assert_eq!(value["dsps"].as_array().unwrap().len(), 1);
    let foreign = admitted_mcp(
        &state,
        &caller,
        "tools/call",
        json!({"name":"find_drivers","arguments":{"dsp":dsp}}),
    )
    .await;
    assert_eq!(foreign["result"]["isError"], true, "{foreign}");
    assert!(
        foreign["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("dsp_not_found:"),
        "{foreign}"
    );
}

#[test]
fn a_key_is_shown_once_kept_as_a_hash_and_reaches_only_its_dsps() {
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
    let (_root, db, dsp) = bootstrapped();
    let user = owner(&db);
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
    // A production key is never one here.
    let live = revoked.token.replacen("dsk_dev_", "dsk_live_", 1);
    assert_eq!(refused(&db, &live), "agent_key_invalid");
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
    use dispatch_backend::State;
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
        .reset_password(token, "replacement-password-long".into())
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
