//! Agent keys: how an outside agent signs in to Dispatch. Only a platform owner makes them.
//! A key works for the platform owner who made it, while they stay an active platform
//! owner; reaches the DSPs it was given, active ones of this environment only; and stops once
//! it expires or is revoked. Nothing an agent does appears in a DSP's activity log.
use crate::{
    LastUse,
    api::types::{
        AgentAccess, AgentDsp, AgentKey, AgentKeyCreated, AgentKeyKind, AgentKeyRequest, AgentKeys,
        AgentProfile, AgentWhoami, AgentWhoamiDsp, AgentWhoamiKey, ToolLevel,
    },
    token,
    toolbox::{Effect, Grants, Toolbox},
};
use dispatch_core::{
    Error, Result,
    accounts::api::types::Dsp,
    db::{Store, at, iso, now},
    ensure,
    foundation::crypto,
    tenancy::audit::AuditChange,
};
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

/// Keys that may be in use at once.
const MOST_KEYS: i64 = 50;
/// The furthest a key's expiry may be set, short of never.
const LONGEST: i64 = 5 * 366 * 86_400_000;
/// A key as listed, with whether a connected app can still renew its access: a refresh
/// token neither expired nor exchanged. `?1` is the time now.
const LISTED: &str = "SELECT k.*,EXISTS(SELECT 1 FROM oauth_tokens t WHERE t.key_id=k.id \
    AND t.kind='refresh' AND t.used_at IS NULL AND t.expires_at>?1) signed_in FROM agent_keys k";

/// An agent signed in with a key: what the key may do and use, and the DSPs it reaches now.
#[derive(Clone, Debug)]
pub struct Caller {
    pub key: String,
    pub name: String,
    pub user: String,
    pub access: AgentAccess,
    pub tools: Grants,
    pub expires_at: Option<String>,
    pub dsps: Vec<Dsp>,
    pub client: String,
}

/// The client a request came from, as the Agents page names it: a known agent or tool and
/// its version, never the whole user agent.
pub fn client_label(user_agent: &str) -> String {
    const KNOWN: &[(&str, &str)] = &[
        ("claude-code", "Claude Code"),
        ("claude-cli", "Claude Code"),
        ("codex", "Codex"),
        ("hermes", "Hermes Agent"),
        ("gemini", "Gemini CLI"),
        ("opencode", "OpenCode"),
        ("goose", "Goose"),
        ("cursor", "Cursor"),
        ("curl", "curl"),
        ("wget", "Wget"),
        // Python HTTP libraries name themselves `python-<library>` with the library's version;
        // the standard library's `Python-urllib/<version>` carries Python's, so it stays Python.
        ("python-httpx2", "HTTPX2"),
        ("python-httpx", "httpx"),
        ("python-requests", "requests"),
        ("python", "Python"),
        ("node", "Node.js"),
        ("undici", "Node.js"),
        ("go-http-client", "Go"),
        ("mozilla", "Browser"),
    ];
    let first = user_agent.split_whitespace().next().unwrap_or("");
    let (product, version) = first.split_once('/').unwrap_or((first, ""));
    let lower = product.to_ascii_lowercase();
    let name = KNOWN
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map(|(_, name)| (*name).to_owned())
        .unwrap_or_else(|| {
            product
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
                .take(32)
                .collect()
        });
    if name.is_empty() {
        return "Unknown".into();
    }
    let version: Vec<&str> = version
        .split('.')
        .take(2)
        .take_while(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    if version.is_empty() || name == "Browser" {
        name
    } else {
        format!("{name} {}", version.join("."))
    }
}

/// An RFC 3339 expiry as the database keeps it, between a minute from now and five years.
fn expiry(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let when = chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| Error::new("invalid_expiry", 400))?
        .timestamp_millis();
    ensure(
        when > now() + 60_000 && when <= now() + LONGEST,
        "invalid_expiry",
        400,
    )?;
    Ok(Some(at(when)))
}

pub trait KeyStore {
    /// Every key, those in use first and newest first, with the DSPs a key can be given and
    /// the tools it can be allowed. `seen` is what this process knows of each key's last use,
    /// newer than the database.
    fn agent_keys(&self, seen: &HashMap<String, LastUse>) -> Result<AgentKeys>;

    /// Makes a key. The key itself is returned this once; only its hash is kept.
    fn create_agent_key(&self, user: &str, input: &AgentKeyRequest) -> Result<AgentKeyCreated>;

    /// Changes where a key or connected app reaches, what it may use, and a key's expiry. A
    /// revoked one stays revoked; an app keeps its access and never expires.
    fn update_agent_key(&self, user: &str, id: &str, input: &AgentKeyRequest) -> Result<AgentKey>;

    /// Revokes a key, or a connected app with its tokens, at once. Revoking a revoked key
    /// changes nothing.
    fn revoke_agent_key(&self, user: &str, id: &str) -> Result<AgentKey>;

    /// Revokes every key and connected app still in use: all of them, or only `owner`'s, with
    /// the tokens of the apps and the approvals not yet redeemed. Answers how many.
    fn revoke_agent_keys(&self, actor: Option<&str>, owner: Option<&str>) -> Result<usize>;

    /// The agent a key belongs to, or why it may not sign in.
    fn authenticate_agent(&self, token: &str, client: &str) -> Result<Caller>;

    /// Refresh an admitted key while the caller holds the same State read lock
    /// used for discovery or protected data. Admission's snapshot is not policy.
    fn revalidate_agent(&self, caller: &Caller) -> Result<Caller>;

    /// Writes down when keys were last used, as the usage counter collected it.
    fn record_agent_use(&self, used: &[(String, LastUse)]) -> Result<()>;

    /// The pseudonymous platform-owner profile represented by an agent credential.
    fn agent_profile(&self, caller: &Caller) -> Result<AgentProfile>;

    /// What an agent is told about itself: its key, the time, and each DSP it reaches.
    fn agent_whoami(&self, caller: &Caller) -> Result<AgentWhoami>;
}
impl KeyStore for Store {
    fn agent_keys(&self, seen: &HashMap<String, LastUse>) -> Result<AgentKeys> {
        let toolbox = Toolbox::installed();
        let mut keys: Vec<AgentKey> = self.platform.query_as(
            &format!("{LISTED} ORDER BY k.revoked_at IS NOT NULL,k.created_at DESC,k.id"),
            [iso()],
        )?;
        for key in &mut keys {
            key.dsps = agent_key_dsps(self, &key.id)?;
            key.tools = toolbox.granted(&agent_key_grants(self, &key.id, key.all_tools)?);
            if let Some((when, client)) = seen.get(&key.id) {
                let when = at(*when);
                if key.last_used_at.as_ref().is_none_or(|last| *last < when) {
                    key.last_used_at = Some(when);
                    key.last_client = Some(client.clone());
                }
            }
        }
        Ok(AgentKeys {
            keys,
            dsps: agent_dsp_choices(self)?,
            tools: toolbox.listed(),
        })
    }

    fn create_agent_key(&self, user: &str, input: &AgentKeyRequest) -> Result<AgentKeyCreated> {
        check_agent_key(self, None, input, &[])?;
        let expires = expiry(input.expires_at.as_deref())?;
        let token = token::new(token::Kind::Key, self.config.env())?;
        let id = crypto::id("agentkey")?;
        self.platform.transaction(|| {
            // What an older release reads it may read, run again in a rollback: nothing.
            self.platform.exec(
                "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                 locations,areas,bypass,created_at,expires_at) \
                 VALUES (?,?,?,?,?,?,?,'full',0,'',0,?,?)",
                params![
                    id,
                    input.name,
                    crypto::sha(&token),
                    &token[token.len() - 4..],
                    user,
                    i64::from(input.all_dsps),
                    input.access,
                    iso(),
                    expires
                ],
            )?;
            set_agent_key_dsps(self, &id, input)?;
            set_agent_key_tools(self, &id, input)?;
            self.audit_with(
                Some(user),
                None,
                "agent.key_created",
                &id,
                Some(&input.name),
                &[],
            )
        })?;
        Ok(AgentKeyCreated {
            key: agent_key(self, &id)?,
            token,
        })
    }

    fn update_agent_key(&self, user: &str, id: &str, input: &AgentKeyRequest) -> Result<AgentKey> {
        let before = agent_key(self, id)?;
        ensure(before.revoked_at.is_none(), "agent_key_revoked", 409)?;
        // An app is edited like a key, but keeps its access and never expires.
        if before.kind == AgentKeyKind::App {
            ensure(
                input.access == AgentAccess::Read && input.expires_at == before.expires_at,
                "invalid_input",
                400,
            )?;
        }
        check_agent_key(self, Some(&before), input, &[])?;
        // An expiry left as it was stays, even one that is close or already past.
        let expires = if input.expires_at == before.expires_at {
            before.expires_at.clone()
        } else {
            expiry(input.expires_at.as_deref())?
        };
        let reach = |all: bool, dsps: &[String]| {
            if all {
                "all DSPs".to_owned()
            } else {
                format!("[{}]", dsps.join(", "))
            }
        };
        let mut changes: Vec<AuditChange> = vec![];
        let mut change = |field: &'static str, from: String, to: String| {
            if from != to {
                changes.push((field, Some(from), Some(to)));
            }
        };
        change("name", before.name.clone(), input.name.clone());
        change(
            "access",
            before.access.as_str().into(),
            input.access.as_str().into(),
        );
        change(
            "dsps",
            reach(before.all_dsps, &before.dsps),
            reach(input.all_dsps, &input.dsps),
        );
        // Each tool by name, one that may also change something marked so.
        let tools = |tools: &BTreeMap<String, ToolLevel>| {
            let named: Vec<String> = tools
                .iter()
                .filter(|(_, level)| **level != ToolLevel::Off)
                .map(|(name, level)| match level {
                    ToolLevel::Change => format!("{name} (changes)"),
                    _ => name.clone(),
                })
                .collect();
            if named.is_empty() {
                "none".to_owned()
            } else {
                format!("[{}]", named.join(", "))
            }
        };
        change("tools", tools(&before.tools), tools(&input.tools));
        change(
            "all_tools",
            before.all_tools.to_string(),
            input.all_tools.to_string(),
        );
        let never = || "never".to_owned();
        change(
            "expires",
            before.expires_at.clone().unwrap_or_else(never),
            expires.clone().unwrap_or_else(never),
        );
        let action = match before.kind {
            AgentKeyKind::Key => "agent.key_updated",
            AgentKeyKind::App => "agent.app_updated",
        };
        self.platform.transaction(|| {
            self.platform.exec(
                "UPDATE agent_keys SET name=?,all_dsps=?,access=?,expires_at=? WHERE id=?",
                params![
                    input.name,
                    i64::from(input.all_dsps),
                    input.access,
                    expires,
                    id
                ],
            )?;
            set_agent_key_dsps(self, id, input)?;
            set_agent_key_tools(self, id, input)?;
            if !changes.is_empty() {
                self.audit_with(Some(user), None, action, id, Some(&input.name), &changes)?;
            }
            Ok(())
        })?;
        agent_key(self, id)
    }

    fn revoke_agent_key(&self, user: &str, id: &str) -> Result<AgentKey> {
        let key = agent_key(self, id)?;
        if key.revoked_at.is_none() {
            let action = match key.kind {
                AgentKeyKind::Key => "agent.key_revoked",
                AgentKeyKind::App => "agent.app_revoked",
            };
            self.platform.transaction(|| {
                self.platform.exec(
                    "UPDATE agent_keys SET revoked_at=? WHERE id=? AND revoked_at IS NULL",
                    [iso(), id.to_owned()],
                )?;
                self.platform
                    .exec("DELETE FROM oauth_tokens WHERE key_id=?", [id])?;
                self.audit_with(Some(user), None, action, id, Some(&key.name), &[])
            })?;
        }
        agent_key(self, id)
    }

    fn revoke_agent_keys(&self, actor: Option<&str>, owner: Option<&str>) -> Result<usize> {
        self.platform
            .transaction(|| revoke_agent_keys_within(self, actor, owner))
    }

    fn authenticate_agent(&self, token: &str, client: &str) -> Result<Caller> {
        ensure(!token.is_empty(), "agent_key_required", 401)?;
        let environment = self.config.env();
        ensure(
            token::well_formed(token, token::Kind::Key, environment),
            "agent_key_invalid",
            401,
        )?;
        let row = self
            .platform
            .one(
                "SELECT id,name,user_id,all_dsps,access,all_tools,expires_at,revoked_at \
                 FROM agent_keys WHERE hash=?",
                [crypto::sha(token)],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        agent_caller(self, &row, client)
    }

    fn revalidate_agent(&self, caller: &Caller) -> Result<Caller> {
        let row = self
            .platform
            .one(
                "SELECT id,name,user_id,all_dsps,access,all_tools,expires_at,revoked_at \
                 FROM agent_keys WHERE id=? AND user_id=?",
                [&caller.key, &caller.user],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        agent_caller(self, &row, &caller.client)
    }

    fn record_agent_use(&self, used: &[(String, LastUse)]) -> Result<()> {
        self.platform.transaction(|| {
            for (key, (when, client)) in used {
                let when = at(*when);
                self.platform.exec(
                    "UPDATE agent_keys SET last_used_at=?1,last_client=?2 WHERE id=?3 \
                     AND (last_used_at IS NULL OR last_used_at<?1)",
                    params![when, client, key],
                )?;
            }
            Ok(())
        })
    }

    fn agent_profile(&self, caller: &Caller) -> Result<AgentProfile> {
        let user = self
            .active_platform_owner(&caller.user)?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        let name = user.name();
        // Hosts need one stable profile id across refresh and reconnection, not Dispatch's
        // internal user primary key. Domain separation keeps this opaque identifier unrelated
        // to the signed request/account labels used elsewhere.
        let id = format!(
            "profile_{}",
            crypto::sign(&self.key, &format!("agent-profile:{}", user.id))
        );
        Ok(AgentProfile {
            id,
            name: name.trim().to_owned(),
        })
    }

    fn agent_whoami(&self, caller: &Caller) -> Result<AgentWhoami> {
        let mut dsps = vec![];
        for dsp in &caller.dsps {
            let zone: chrono_tz::Tz = dsp.timezone.parse().unwrap_or(chrono_tz::UTC);
            dsps.push(AgentWhoamiDsp {
                id: dsp.id.clone(),
                name: dsp.name.clone(),
                timezone: dsp.timezone.clone(),
                today: chrono::Utc::now()
                    .with_timezone(&zone)
                    .date_naive()
                    .to_string(),
            });
        }
        Ok(AgentWhoami {
            key: AgentWhoamiKey {
                name: caller.name.clone(),
                access: caller.access,
                expires_at: caller.expires_at.clone(),
                tools: Toolbox::installed()
                    .all()
                    .iter()
                    .map(|tool| (tool.name().to_owned(), caller.tools.level(*tool)))
                    .filter(|(_, level)| *level != ToolLevel::Off)
                    .collect(),
            },
            environment: self.config.env(),
            now: iso(),
            dsps,
        })
    }
}

/// `KeyStore::revoke_agent_keys` inside a transaction the caller holds, so the revocation
/// stands or falls with what it belongs to, such as a password reset.
pub(crate) fn revoke_agent_keys_within(
    db: &Store,
    actor: Option<&str>,
    owner: Option<&str>,
) -> Result<usize> {
    let revoked = db.platform.exec(
        "UPDATE agent_keys SET revoked_at=?1 WHERE revoked_at IS NULL \
         AND (?2 IS NULL OR user_id=?2)",
        params![iso(), owner],
    )?;
    db.platform.exec(
        "DELETE FROM oauth_tokens WHERE key_id IN \
         (SELECT id FROM agent_keys WHERE revoked_at IS NOT NULL)",
        [],
    )?;
    db.platform.exec(
        "DELETE FROM oauth_codes WHERE used_at IS NULL AND (?1 IS NULL OR approved_by=?1)",
        [owner],
    )?;
    if revoked > 0 {
        db.audit(actor, None, "agent.keys_revoked", &revoked.to_string())?;
    }
    Ok(revoked)
}

fn agent_dsp_choices(db: &Store) -> Result<Vec<AgentDsp>> {
    Ok(db
        .served_dsps()?
        .into_iter()
        .map(|dsp| AgentDsp {
            id: dsp.id,
            name: dsp.name,
        })
        .collect())
}

pub(crate) fn agent_key(db: &Store, id: &str) -> Result<AgentKey> {
    let mut key: AgentKey = db
        .platform
        .one_as(&format!("{LISTED} WHERE k.id=?2"), [iso(), id.to_owned()])?
        .ok_or_else(|| Error::new("agent_key_not_found", 404))?;
    key.dsps = agent_key_dsps(db, id)?;
    key.tools = Toolbox::installed().granted(&agent_key_grants(db, id, key.all_tools)?);
    Ok(key)
}

/// What a key or app may use: its choices, and whether a tool added since that only
/// reads is allowed (`all`).
pub(crate) fn agent_key_grants(db: &Store, id: &str, all: bool) -> Result<Grants> {
    Ok(Grants {
        all,
        chosen: db
            .platform
            .query_as::<(String, i64, i64)>(
                "SELECT tool,allowed,changes FROM agent_key_tools WHERE key_id=?",
                [id],
            )?
            .into_iter()
            .map(|(tool, allowed, changes)| {
                let level = match (allowed, changes) {
                    (0, _) => ToolLevel::Off,
                    (_, 0) => ToolLevel::Read,
                    _ => ToolLevel::Change,
                };
                (tool, level)
            })
            .collect(),
    })
}

pub(crate) fn agent_key_dsps(db: &Store, id: &str) -> Result<Vec<String>> {
    Ok(db
        .platform
        .query_as::<(String,)>(
            "SELECT dsp_id FROM agent_key_dsps WHERE key_id=? ORDER BY dsp_id",
            [id],
        )?
        .into_iter()
        .map(|(dsp,)| dsp)
        .collect())
}

// What every new or changed key must satisfy. `before` is the key being changed, as it
// is. `replaced` are the connected apps a new connection of the same app replaces, which
// so neither take its name nor count.
pub(crate) fn check_agent_key(
    db: &Store,
    before: Option<&AgentKey>,
    input: &AgentKeyRequest,
    replaced: &[String],
) -> Result<()> {
    let replaced = replaced.len() as i64;
    let taken = db.platform.count(
        "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL AND id<>?1 \
         AND lower(name)=lower(?2)",
        params![before.map_or("", |key| key.id.as_str()), input.name],
    )?;
    ensure(taken - replaced == 0, "agent_key_name_taken", 409)?;
    if before.is_none() {
        let active = db.platform.count(
            "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL \
             AND (expires_at IS NULL OR expires_at>?)",
            [iso()],
        )?;
        ensure(active - replaced < MOST_KEYS, "agent_key_limit", 409)?;
    }
    let choices: Vec<String> = agent_dsp_choices(db)?
        .into_iter()
        .map(|dsp| dsp.id)
        .collect();
    // A DSP that isn't active, as one suspended or removed, can't be given to a key. One
    // the key already has stays, for when it is active again: the Agents page doesn't
    // list it, and sends it back unchanged.
    let kept = before.map_or(&[][..], |key| key.dsps.as_slice());
    ensure(
        input
            .dsps
            .iter()
            .all(|dsp| choices.contains(dsp) || kept.contains(dsp)),
        "invalid_input",
        400,
    )?;
    // Only tools a key can be granted, a tool no longer installed being no choice; and
    // changing something only with a tool that can.
    let toolbox = Toolbox::installed();
    ensure(
        input.tools.iter().all(|(name, level)| {
            toolbox.switchable().any(|tool| {
                tool.name() == name
                    && (*level != ToolLevel::Change || tool.effect() == Effect::Changes)
            })
        }),
        "invalid_input",
        400,
    )?;
    Ok(())
}

/// The DSPs a key reaches.
pub(crate) fn set_agent_key_dsps(db: &Store, id: &str, input: &AgentKeyRequest) -> Result<()> {
    db.platform
        .exec("DELETE FROM agent_key_dsps WHERE key_id=?", [id])?;
    for dsp in &input.dsps {
        db.platform.exec(
            "INSERT OR IGNORE INTO agent_key_dsps(key_id,dsp_id) VALUES (?,?)",
            [id, dsp.as_str()],
        )?;
    }
    Ok(())
}

/// What a key or app may do with each tool: a choice for every tool there is now, and
/// whether one added later comes, to read, as it is added.
pub(crate) fn set_agent_key_tools(db: &Store, id: &str, input: &AgentKeyRequest) -> Result<()> {
    db.platform.exec(
        "UPDATE agent_keys SET all_tools=? WHERE id=?",
        params![i64::from(input.all_tools), id],
    )?;
    db.platform
        .exec("DELETE FROM agent_key_tools WHERE key_id=?", [id])?;
    for tool in Toolbox::installed().switchable() {
        let level = input
            .tools
            .get(tool.name())
            .copied()
            .unwrap_or(ToolLevel::Off);
        db.platform.exec(
            "INSERT INTO agent_key_tools(key_id,tool,allowed,changes) VALUES (?,?,?,?)",
            params![
                id,
                tool.name(),
                i64::from(level != ToolLevel::Off),
                i64::from(level == ToolLevel::Change)
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn agent_caller(db: &Store, row: &serde_json::Value, client: &str) -> Result<Caller> {
    let text = |name: &str| row[name].as_str().unwrap_or_default().to_owned();
    ensure(row["revoked_at"].is_null(), "agent_key_revoked", 401)?;
    let expires_at = row["expires_at"].as_str().map(str::to_owned);
    ensure(
        expires_at.as_ref().is_none_or(|when| *when > iso()),
        "agent_key_expired",
        401,
    )?;
    // A key is never stronger than its maker: it stops when they stop being an
    // active platform owner.
    ensure(
        db.active_platform_owner(&text("user_id"))?.is_some(),
        "agent_key_invalid",
        401,
    )?;
    let key = text("id");
    let mut dsps: Vec<Dsp> = db.served_dsps()?;
    if row["all_dsps"] != 1 {
        let given = agent_key_dsps(db, &key)?;
        dsps.retain(|dsp| given.contains(&dsp.id));
    }
    Ok(Caller {
        name: text("name"),
        user: text("user_id"),
        access: AgentAccess::parse(&text("access"))
            .ok_or_else(|| Error::new("invalid_stored_record", 500))?,
        tools: agent_key_grants(db, &key, row["all_tools"] == 1)?,
        expires_at,
        dsps,
        client: client.to_owned(),
        key,
    })
}

#[cfg(test)]
#[path = "../tests/backend/keys.rs"]
mod tests;
