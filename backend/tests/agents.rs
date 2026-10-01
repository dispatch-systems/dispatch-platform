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
