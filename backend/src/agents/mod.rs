//! Agent keys: how an outside agent signs in to Dispatch. Only a platform owner makes them.
//! A key works for the platform owner who made it, while they stay an active platform
//! owner; reaches the DSPs it was given, active ones of this environment only; and stops
//! once it expires or is revoked. Nothing an agent does appears in a DSP's activity log.
mod token;
mod usage;

pub use usage::{LastUse, PER_MINUTE, Usage};

use crate::{
    Error, Result,
    audit::AuditChange,
    contracts::{
        AgentAccess, AgentDsp, AgentKey, AgentKeyCreated, AgentKeyRequest, AgentKeys, AgentTools,
        AgentWhoami, AgentWhoamiDsp, AgentWhoamiKey, Dsp,
    },
    crypto,
    db::{Store, at, iso, now},
    ensure,
};
use rusqlite::params;
use std::collections::HashMap;

/// Keys that may be in use at once.
const MOST_KEYS: i64 = 50;
/// The furthest a key's expiry may be set, short of never.
const LONGEST: i64 = 5 * 366 * 86_400_000;

/// An agent signed in with a key: what the key may do and the DSPs it reaches now.
#[derive(Clone, Debug)]
pub struct Caller {
    pub key: String,
    pub name: String,
    pub user: String,
    pub access: AgentAccess,
    pub tools: AgentTools,
    pub locations: bool,
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

impl Store {
    fn agent_dsp_choices(&self) -> Result<Vec<AgentDsp>> {
        Ok(self
            .platform
            .query_as::<(String, String)>(
                "SELECT id,name FROM dsps WHERE environment=? AND status='active' \
                 ORDER BY name COLLATE NOCASE",
                [self.config.env().as_str()],
            )?
            .into_iter()
            .map(|(id, name)| AgentDsp { id, name })
            .collect())
    }
    fn agent_key(&self, id: &str) -> Result<AgentKey> {
        let mut key: AgentKey = self
            .platform
            .one_as("SELECT * FROM agent_keys WHERE id=?", [id])?
            .ok_or_else(|| Error::new("agent_key_not_found", 404))?;
        key.dsps = self.agent_key_dsps(id)?;
        Ok(key)
    }
    fn agent_key_dsps(&self, id: &str) -> Result<Vec<String>> {
        Ok(self
            .platform
            .query_as::<(String,)>(
                "SELECT dsp_id FROM agent_key_dsps WHERE key_id=? ORDER BY dsp_id",
                [id],
            )?
            .into_iter()
            .map(|(dsp,)| dsp)
            .collect())
    }

    /// Every key, those in use first and newest first, with the DSPs a key can be given.
    /// `seen` is what this process knows of each key's last use, newer than the database.
    pub fn agent_keys(&self, seen: &HashMap<String, LastUse>) -> Result<AgentKeys> {
        let mut keys: Vec<AgentKey> = self.platform.query_as(
            "SELECT * FROM agent_keys ORDER BY revoked_at IS NOT NULL,created_at DESC,id",
            [],
        )?;
        for key in &mut keys {
            key.dsps = self.agent_key_dsps(&key.id)?;
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
            dsps: self.agent_dsp_choices()?,
        })
    }

    // What every new or changed key must satisfy. `id` is the key being changed.
    fn check_agent_key(&self, id: Option<&str>, input: &AgentKeyRequest) -> Result<()> {
        let taken = self.platform.count(
            "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL AND id<>?1 \
             AND lower(name)=lower(?2)",
            params![id.unwrap_or(""), input.name],
        )?;
        ensure(taken == 0, "agent_key_name_taken", 409)?;
        if id.is_none() {
            let active = self.platform.count(
                "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL \
                 AND (expires_at IS NULL OR expires_at>?)",
                [iso()],
            )?;
            ensure(active < MOST_KEYS, "agent_key_limit", 409)?;
        }
        let choices: Vec<String> = self
            .agent_dsp_choices()?
            .into_iter()
            .map(|dsp| dsp.id)
            .collect();
        ensure(
            input.dsps.iter().all(|dsp| choices.contains(dsp)),
            "invalid_input",
            400,
        )?;
        Ok(())
    }
    fn set_agent_key_dsps(&self, id: &str, input: &AgentKeyRequest) -> Result<()> {
        self.platform
            .exec("DELETE FROM agent_key_dsps WHERE key_id=?", [id])?;
        for dsp in &input.dsps {
            self.platform.exec(
                "INSERT OR IGNORE INTO agent_key_dsps(key_id,dsp_id) VALUES (?,?)",
                [id, dsp.as_str()],
            )?;
        }
        Ok(())
    }

    /// Makes a key. The key itself is returned this once; only its hash is kept.
    pub fn create_agent_key(&self, user: &str, input: &AgentKeyRequest) -> Result<AgentKeyCreated> {
        self.check_agent_key(None, input)?;
        let expires = expiry(input.expires_at.as_deref())?;
        let token = token::new(self.config.env())?;
        let id = crypto::id("agentkey")?;
        self.platform.transaction(|| {
            self.platform.exec(
                "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                 locations,created_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    id,
                    input.name,
                    crypto::sha(&token),
                    &token[token.len() - 4..],
                    user,
                    i64::from(input.all_dsps),
                    input.access,
                    input.tools,
                    i64::from(input.locations),
                    iso(),
                    expires
                ],
            )?;
            self.set_agent_key_dsps(&id, input)?;
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
            key: self.agent_key(&id)?,
            token,
        })
    }

    /// Changes what a key may do. A revoked key stays revoked.
    pub fn update_agent_key(
        &self,
        user: &str,
        id: &str,
        input: &AgentKeyRequest,
    ) -> Result<AgentKey> {
        let before = self.agent_key(id)?;
        ensure(before.revoked_at.is_none(), "agent_key_revoked", 409)?;
        self.check_agent_key(Some(id), input)?;
        let expires = expiry(input.expires_at.as_deref())?;
        let reach = |all: bool, dsps: &[String]| {
            if all {
                "all DSPs".to_owned()
            } else {
                format!("{} DSPs", dsps.len())
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
            "tools",
            before.tools.as_str().into(),
            input.tools.as_str().into(),
        );
        change(
            "locations",
            before.locations.to_string(),
            input.locations.to_string(),
        );
        change(
            "dsps",
            reach(before.all_dsps, &before.dsps),
            reach(input.all_dsps, &input.dsps),
        );
        let never = || "never".to_owned();
        change(
            "expires",
            before.expires_at.clone().unwrap_or_else(never),
            expires.clone().unwrap_or_else(never),
        );
        self.platform.transaction(|| {
            self.platform.exec(
                "UPDATE agent_keys SET name=?,all_dsps=?,access=?,tools=?,locations=?,expires_at=? \
                 WHERE id=?",
                params![
                    input.name,
                    i64::from(input.all_dsps),
                    input.access,
                    input.tools,
                    i64::from(input.locations),
                    expires,
                    id
                ],
            )?;
            self.set_agent_key_dsps(id, input)?;
            if !changes.is_empty() {
                self.audit_with(
                    Some(user),
                    None,
                    "agent.key_updated",
                    id,
                    Some(&input.name),
                    &changes,
                )?;
            }
            Ok(())
        })?;
        self.agent_key(id)
    }

    /// Revokes a key at once. Revoking a revoked key changes nothing.
    pub fn revoke_agent_key(&self, user: &str, id: &str) -> Result<AgentKey> {
        let key = self.agent_key(id)?;
        if key.revoked_at.is_none() {
            self.platform.transaction(|| {
                self.platform.exec(
                    "UPDATE agent_keys SET revoked_at=? WHERE id=? AND revoked_at IS NULL",
                    [iso(), id.to_owned()],
                )?;
                self.audit_with(
                    Some(user),
                    None,
                    "agent.key_revoked",
                    id,
                    Some(&key.name),
                    &[],
                )
            })?;
        }
        self.agent_key(id)
    }

    /// Revokes every key still in use: all of them, or only `owner`'s. Answers how many.
    pub fn revoke_agent_keys(&self, actor: Option<&str>, owner: Option<&str>) -> Result<usize> {
        self.platform.transaction(|| {
            let revoked = self.platform.exec(
                "UPDATE agent_keys SET revoked_at=?1 WHERE revoked_at IS NULL \
                 AND (?2 IS NULL OR user_id=?2)",
                params![iso(), owner],
            )?;
            if revoked > 0 {
                self.audit(actor, None, "agent.keys_revoked", &revoked.to_string())?;
            }
            Ok(revoked)
        })
    }

    /// The agent a key belongs to, or why it may not sign in.
    pub fn authenticate_agent(&self, token: &str, client: &str) -> Result<Caller> {
        ensure(!token.is_empty(), "agent_key_required", 401)?;
        let environment = self.config.env();
        ensure(
            token::well_formed(token, environment),
            "agent_key_invalid",
            401,
        )?;
        let row = self
            .platform
            .one(
                "SELECT k.id,k.name,k.user_id,k.all_dsps,k.access,k.tools,k.locations,\
                 k.expires_at,k.revoked_at,u.platform_owner,u.status FROM agent_keys k \
                 JOIN users u ON u.id=k.user_id WHERE k.hash=?",
                [crypto::sha(token)],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
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
            row["platform_owner"] == 1 && row["status"] == "active",
            "agent_key_invalid",
            401,
        )?;
        let key = text("id");
        let dsps: Vec<Dsp> = if row["all_dsps"] == 1 {
            self.platform.query_as(
                "SELECT * FROM dsps WHERE environment=? AND status='active' \
                 ORDER BY name COLLATE NOCASE",
                [environment.as_str()],
            )?
        } else {
            self.platform.query_as(
                "SELECT d.* FROM dsps d JOIN agent_key_dsps k ON k.dsp_id=d.id \
                 WHERE k.key_id=? AND d.environment=? AND d.status='active' \
                 ORDER BY d.name COLLATE NOCASE",
                [key.as_str(), environment.as_str()],
            )?
        };
        Ok(Caller {
            name: text("name"),
            user: text("user_id"),
            access: AgentAccess::parse(&text("access"))
                .ok_or_else(|| Error::new("invalid_stored_record", 500))?,
            tools: AgentTools::parse(&text("tools"))
                .ok_or_else(|| Error::new("invalid_stored_record", 500))?,
            locations: row["locations"] == 1,
            expires_at,
            dsps,
            client: client.to_owned(),
            key,
        })
    }

    /// Writes down when keys were last used, as the usage counter collected it.
    pub fn record_agent_use(&self, used: &[(String, LastUse)]) -> Result<()> {
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

    /// What an agent is told about itself: its key, the time, and each DSP it reaches.
    pub fn agent_whoami(&self, caller: &Caller) -> Result<AgentWhoami> {
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
                features: self.features(&dsp.id)?,
            });
        }
        Ok(AgentWhoami {
            key: AgentWhoamiKey {
                name: caller.name.clone(),
                access: caller.access,
                tools: caller.tools,
                locations: caller.locations,
                expires_at: caller.expires_at.clone(),
            },
            environment: self.config.env(),
            now: iso(),
            dsps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_are_named_without_their_whole_user_agent() {
        assert_eq!(
            client_label("claude-code/2.1.283 (external, cli)"),
            "Claude Code 2.1"
        );
        assert_eq!(
            client_label("codex_cli_rs/0.157.1 (Ubuntu 24.04; x86_64) xterm"),
            "Codex 0.157"
        );
        assert_eq!(client_label("curl/8.5.0"), "curl 8.5");
        assert_eq!(client_label("Mozilla/5.0 (X11; Linux x86_64)"), "Browser");
        assert_eq!(client_label("my-agent/1.0-beta"), "my-agent 1");
        assert_eq!(client_label("<script>/1"), "script 1");
        assert_eq!(client_label(""), "Unknown");
    }
    #[test]
    fn expiries_are_a_minute_to_five_years_away() {
        assert_eq!(expiry(None).unwrap(), None);
        let soon = chrono::Utc::now() + chrono::Duration::days(30);
        let kept = expiry(Some(&soon.to_rfc3339())).unwrap().unwrap();
        assert!(kept.ends_with('Z') && kept > iso());
        let past = chrono::Utc::now() - chrono::Duration::days(1);
        assert!(expiry(Some(&past.to_rfc3339())).is_err());
        let far = chrono::Utc::now() + chrono::Duration::days(6 * 366);
        assert!(expiry(Some(&far.to_rfc3339())).is_err());
        assert!(expiry(Some("next tuesday")).is_err());
    }
}
