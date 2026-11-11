//! Token redemption, rotation, replacement, and termination of connected apps.
use super::{
    Answer, Query, Refusal, SCOPE, clients,
    notices::{self, Told},
    protocol::{check_scope, grant, verifies},
    resource,
};
use crate::api::types::{AgentAccess, AgentKeyRequest, ToolLevel};
use crate::{
    Caller, ClientStore, GuardStore, keys,
    token::{self, Kind},
    toolbox::Toolbox,
};
use dispatch_core::{
    Error, Result,
    db::{Store, at, iso, now, s},
    ensure,
    foundation::crypto,
};
use serde_json::{Value, json};

/// Seconds, as the token endpoint says it.
const ACCESS_SECONDS: i64 = 60 * 60;
const REFRESH_LIFETIME: i64 = 30 * 24 * 60 * 60 * 1000;
/// Used codes are kept this long past their expiry, so a replay still ends what they made.
const CODE_KEPT: i64 = 24 * 60 * 60 * 1000;

pub(super) fn exchange(db: &Store, form: &Query) -> Answer<Value> {
    let grant_type = form.required("grant_type")?;
    let client_id = form.required("client_id")?;
    if form.one("resource")? != Some(resource(&db.config).as_str()) {
        return Err(Refusal::new(
            "invalid_target",
            "resource must be Dispatch's MCP endpoint",
        ));
    }
    check_scope(form)?;
    let known = clients::known_app(client_id).is_some()
        || (client_id.starts_with("dcr_") && db.registered_client(client_id)?.is_some())
        || (client_id.starts_with("https://") && website_approved(db, client_id)?);
    if !known {
        return Err(Refusal::new(
            "invalid_client",
            "The client is not registered",
        ));
    }
    match grant_type {
        "authorization_code" => redeem_code(db, form, client_id),
        "refresh_token" => refresh(db, form, client_id),
        _ => Err(Refusal::new(
            "unsupported_grant_type",
            "Only authorization_code and refresh_token are supported",
        )),
    }
}

pub(super) fn revoke(db: &Store, form: &Query) -> Answer<()> {
    let presented = form.required("token")?;
    let client_id = form.one("client_id")?;
    let row = db.platform.one(
        "SELECT t.key_id,k.client_id FROM oauth_tokens t JOIN agent_keys k ON k.id=t.key_id \
         WHERE t.hash=?",
        [crypto::sha(presented)],
    )?;
    if let Some(row) = row
        && client_id.is_none_or(|client| client == s(&row, "client_id"))
    {
        end_app(db, s(&row, "key_id"), "signed_out")?;
    }
    Ok(())
}

pub(super) fn authenticate(db: &Store, token: &str, client: &str) -> Result<Caller> {
    ensure(
        token::well_formed(token, Kind::Access, db.config.environment),
        "access_token_invalid",
        401,
    )?;
    let row = db
        .platform
        .one(
            "SELECT t.expires_at token_expires_at,t.resource,k.id,k.name,k.user_id,k.all_dsps,\
             k.access,k.all_tools,k.expires_at,k.revoked_at FROM oauth_tokens t \
             JOIN agent_keys k ON k.id=t.key_id WHERE t.hash=? AND t.kind='access'",
            [crypto::sha(token)],
        )?
        .ok_or_else(|| Error::new("access_token_invalid", 401))?;
    ensure(
        s(&row, "resource") == resource(&db.config),
        "access_token_invalid",
        401,
    )?;
    ensure(
        s(&row, "token_expires_at") > iso().as_str(),
        "access_token_expired",
        401,
    )?;
    keys::agent_caller(db, &row, client)
}

pub(super) fn prune(db: &Store) -> Result<()> {
    let day = 24 * 60 * 60 * 1000;
    db.platform.transaction(|| {
        db.platform
            .exec("DELETE FROM oauth_requests WHERE expires_at<?", [iso()])?;
        db.platform.exec(
            "DELETE FROM oauth_codes WHERE expires_at<?",
            [at(now() - CODE_KEPT)],
        )?;
        db.platform
            .exec("DELETE FROM oauth_tokens WHERE expires_at<?", [iso()])?;
        db.platform.exec(
            "DELETE FROM oauth_clients WHERE (last_used_at IS NULL AND created_at<?1) \
             OR last_used_at<?2",
            [at(now() - clients::UNUSED_LIFETIME), at(now() - 90 * day)],
        )?;
        Ok(())
    })
}

/// A code for the connected app it was approved as, and its first tokens.
fn redeem_code(db: &Store, form: &Query, client_id: &str) -> Answer<Value> {
    let code = form.required("code")?;
    let verifier = form.required("code_verifier")?;
    let redirect_uri = form.required("redirect_uri")?;
    let row = db
        .platform
        .one(
            "SELECT * FROM oauth_codes WHERE hash=?",
            [crypto::sha(code)],
        )?
        .ok_or_else(|| grant("The code is not one Dispatch issued"))?;
    // Only the client that asked, at the redirect it asked for, with the verifier it
    // began with. A mismatch ends nothing: it proves no one held the code rightly.
    if s(&row, "client_id") != client_id
        || s(&row, "redirect_uri") != redirect_uri
        || !verifies(verifier, s(&row, "code_challenge"))
    {
        return Err(grant(
            "The code was issued for another client, redirect or verifier",
        ));
    }
    if !row["used_at"].is_null() {
        // Redeemed once already, and presented again in full: whoever holds it, the
        // connected app it made can no longer be trusted.
        if let Some(key) = row["key_id"].as_str() {
            end_app(db, key, "code_reused")?;
        }
        return Err(grant("The code was already used"));
    }
    if s(&row, "expires_at") <= iso().as_str() {
        return Err(grant("The code expired"));
    }
    if !db.oauth_client_allowed(client_id, redirect_uri)? {
        return Err(grant("The owner no longer lets this kind of app connect"));
    }
    let choices: Value = serde_json::from_str(s(&row, "choices")).map_err(Error::from)?;
    // Approved before tools had levels, each tool it may use reads; approved before tools
    // were chosen, every tool reads, and those added later.
    let (all_tools, tools) = match (choices.get("tool_levels"), choices.get("agent_tools")) {
        (Some(levels), _) => (
            choices["all_tools"] == true,
            serde_json::from_value(levels.clone()).map_err(Error::from)?,
        ),
        (None, Some(tools)) => (
            choices["all_tools"] == true,
            serde_json::from_value::<Vec<String>>(tools.clone())
                .map_err(Error::from)?
                .into_iter()
                .map(|tool| (tool, ToolLevel::Read))
                .collect(),
        ),
        (None, None) => (true, Toolbox::installed().defaults()),
    };
    let key = AgentKeyRequest {
        name: s(&choices, "name").to_owned(),
        all_dsps: choices["all_dsps"] == true,
        dsps: serde_json::from_value(choices["dsps"].clone()).map_err(Error::from)?,
        access: AgentAccess::Read,
        all_tools,
        tools,
        expires_at: None,
    };
    let owner = s(&row, "approved_by");
    if db.active_platform_owner(owner)?.is_none() {
        return Err(grant("The approval no longer stands"));
    }
    // What the owner chose is checked again, as a key's would be when it is made.
    let replaced = replaced_apps(
        db,
        &key.name,
        client_id,
        s(&row, "client_name"),
        row["client_verified"] == 1,
    )?;
    keys::check_agent_key(db, None, &key, &replaced).map_err(|error| match error.status {
        500.. => error.into(),
        _ => grant(&format!(
            "The approval can no longer be used: {}",
            error.code
        )),
    })?;
    let id = crypto::id("agentkey")?;
    let tokens = db.platform.transaction(|| {
        let redeemed = db.platform.exec(
            "UPDATE oauth_codes SET used_at=?1,key_id=?2 WHERE hash=?3 AND used_at IS NULL",
            [iso(), id.clone(), crypto::sha(code)],
        )?;
        ensure(redeemed == 1, "code_already_used", 409)?;
        // The app connected again: its earlier connection of the same name ends with it.
        for earlier in &replaced {
            end_app_within(db, Some(owner), earlier, "replaced")?;
        }
        // What an older release reads it may read, run again in a rollback: nothing, as a
        // key's.
        db.platform.exec(
            "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,locations,\
             areas,bypass,created_at,kind,client_id,client_name,client_verified) \
             VALUES (?,?,?,'',?,?,?,'full',0,'',0,?,'app',?,?,?)",
            rusqlite::params![
                id,
                key.name,
                format!("app:{id}"),
                owner,
                i64::from(key.all_dsps),
                key.access,
                iso(),
                client_id,
                s(&row, "client_name"),
                row["client_verified"].as_i64()
            ],
        )?;
        keys::set_agent_key_dsps(db, &id, &key)?;
        keys::set_agent_key_tools(db, &id, &key)?;
        db.audit_with(
            Some(owner),
            None,
            "agent.app_connected",
            &id,
            Some(&key.name),
            &[],
        )?;
        issue_tokens(db, &id, client_id)
    })?;
    notices::tell_owners(db, &id, Told::Connected { redirect_uri });
    Ok(tokens)
}

/// Whether a website's client id was ever approved, and so may use the token endpoint.
fn website_approved(db: &Store, client_id: &str) -> Result<bool> {
    Ok(db.platform.count(
        "SELECT EXISTS(SELECT 1 FROM oauth_codes WHERE client_id=?1) \
         OR EXISTS(SELECT 1 FROM agent_keys WHERE kind='app' AND client_id=?1)",
        [client_id],
    )? == 1)
}

/// A refresh token for a new pair. The one presented is spent exactly once; presenting it
/// again proves that the app's token family may be compromised and ends the connection.
fn refresh(db: &Store, form: &Query, client_id: &str) -> Answer<Value> {
    let presented = form.required("refresh_token")?;
    if !token::well_formed(presented, Kind::Refresh, db.config.environment) {
        return Err(grant("The refresh token is not one Dispatch issued"));
    }
    let row = db
        .platform
        .one(
            "SELECT t.key_id,t.resource,t.expires_at,t.used_at,k.client_id,k.revoked_at,\
             k.user_id FROM oauth_tokens t JOIN agent_keys k ON k.id=t.key_id \
             WHERE t.hash=? AND t.kind='refresh'",
            [crypto::sha(presented)],
        )?
        .ok_or_else(|| grant("The refresh token is not one Dispatch issued"))?;
    if s(&row, "client_id") != client_id || s(&row, "resource") != resource(&db.config) {
        return Err(grant("The refresh token was issued to another client"));
    }
    if s(&row, "expires_at") <= iso().as_str() {
        return Err(grant("The refresh token expired"));
    }
    let live =
        row["revoked_at"].is_null() && db.active_platform_owner(s(&row, "user_id"))?.is_some();
    if !live {
        return Err(grant("The connection was ended"));
    }
    let key = s(&row, "key_id").to_owned();
    if !row["used_at"].is_null() {
        end_app(db, &key, "refresh_reused")?;
        return Err(grant("The refresh token was already used"));
    }
    let hash = crypto::sha(presented);
    match db.platform.transaction(|| {
        // Only the transaction that wins this unused-row compare-and-swap writes either
        // exchange marker and issues the replacement pair.
        let exchanged = db.platform.exec(
            "UPDATE oauth_tokens SET used_at=?1,replaced_at=?1 \
             WHERE hash=?2 AND kind='refresh' AND used_at IS NULL",
            [iso(), hash],
        )?;
        ensure(exchanged == 1, "refresh_reused", 409)?;
        issue_tokens(db, &key, client_id)
    }) {
        Ok(tokens) => Ok(tokens),
        Err(error) if error.code == "refresh_reused" => {
            end_app(db, &key, "refresh_reused")?;
            Err(grant("The refresh token was already used"))
        }
        Err(error) => Err(error.into()),
    }
}

/// The live connections a new connection named `name` replaces: the same app's, under the
/// same name. The same app is the same client; or, for apps that registered themselves and
/// so get a new client each time, one that gave the same name. A known app and a
/// self-registered one are never the same, whatever they call themselves.
pub(super) fn replaced_apps(
    db: &Store,
    name: &str,
    client_id: &str,
    client_name: &str,
    verified: bool,
) -> Result<Vec<String>> {
    Ok(db
        .platform
        .query_as::<(String,)>(
            "SELECT id FROM agent_keys WHERE kind='app' AND revoked_at IS NULL \
             AND lower(name)=lower(?1) AND (client_id=?2 OR (?3=0 AND client_verified=0 \
             AND lower(trim(client_name))=lower(trim(?4))))",
            rusqlite::params![name, client_id, i64::from(verified), client_name],
        )?
        .into_iter()
        .map(|(id,)| id)
        .collect())
}

/// A new access and refresh token for a connected app, as the token endpoint answers.
fn issue_tokens(db: &Store, key: &str, client_id: &str) -> Result<Value> {
    let environment = db.config.environment;
    let access = token::new(Kind::Access, environment)?;
    let refresh = token::new(Kind::Refresh, environment)?;
    for (value, kind, lifetime) in [
        (&access, "access", ACCESS_SECONDS * 1000),
        (&refresh, "refresh", REFRESH_LIFETIME),
    ] {
        db.platform.exec(
            "INSERT INTO oauth_tokens(hash,key_id,kind,resource,created_at,expires_at) \
             VALUES (?,?,?,?,?,?)",
            [
                crypto::sha(value),
                key.to_owned(),
                kind.to_owned(),
                resource(&db.config),
                iso(),
                at(now() + lifetime),
            ],
        )?;
    }
    db.used_oauth_client(client_id)?;
    Ok(json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_SECONDS,
        "refresh_token": refresh,
        "scope": SCOPE,
    }))
}

/// Ends a connected app from the protocol's side: a code or refresh token replayed, or the
/// app signing out. Its tokens stop at once. A replay is Dispatch's own doing, so the
/// platform owners are told why.
fn end_app(db: &Store, key: &str, reason: &str) -> Result<()> {
    let ended = db
        .platform
        .transaction(|| end_app_within(db, None, key, reason))?;
    if ended && matches!(reason, "code_reused" | "refresh_reused") {
        notices::tell_owners(db, key, Told::Disconnected { reason });
    }
    Ok(())
}

/// The same inside a transaction the caller holds, such as the one connecting the app's
/// replacement. `actor` is the owner when it is their doing. Whether it was live until now.
fn end_app_within(db: &Store, actor: Option<&str>, key: &str, reason: &str) -> Result<bool> {
    let ended = db.platform.exec(
        "UPDATE agent_keys SET revoked_at=? WHERE id=? AND kind='app' AND revoked_at IS NULL",
        [iso(), key.to_owned()],
    )?;
    db.platform
        .exec("DELETE FROM oauth_tokens WHERE key_id=?", [key])?;
    if ended > 0 {
        let (name,): (String,) = db
            .platform
            .one_as("SELECT name FROM agent_keys WHERE id=?", [key])?
            .ok_or_else(|| Error::new("agent_key_not_found", 404))?;
        db.audit_with(
            actor,
            None,
            "agent.app_revoked",
            key,
            Some(&name),
            &[("reason", None, Some(reason.to_owned()))],
        )?;
    }
    Ok(ended > 0)
}
