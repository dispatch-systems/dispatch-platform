//! Agent keys: how an outside agent signs in to Dispatch. Only a platform owner makes them.
//! A key works for the platform owner who made it, while they stay an active platform
//! owner; reaches the DSPs it was given, active ones of this environment only; reads there
//! the kinds of data it was allowed, its own settings or a DSP's own; and stops once it
//! expires or is revoked. Nothing an agent does appears in a DSP's activity log.
//! An app the owner connects with Sign in with Dispatch (`oauth`) is a key of kind `app`.
//! What keys and apps call is kept for the Agents page's Activity log (`activity`).
#[path = "activity.rs"]
pub mod activity;
#[path = "data/mod.rs"]
pub mod data;
#[path = "server.rs"]
pub mod mcp;
#[path = "oauth/mod.rs"]
pub mod oauth;
#[path = "pieces.rs"]
pub mod pieces;
#[path = "skill.rs"]
pub mod skill;
#[path = "synthetic.rs"]
pub mod synthetic;
#[path = "token.rs"]
mod token;
#[path = "usage.rs"]
mod usage;

pub use activity::Activity;
pub use pieces::Mcp;
pub use usage::{LastUse, PER_MINUTE, Usage};

use crate::{
    Error, Result,
    audit::AuditChange,
    contracts::{
        AgentAccess, AgentArea, AgentDsp, AgentDspReads, AgentKey, AgentKeyCreated, AgentKeyDsp,
        AgentKeyKind, AgentKeyRequest, AgentKeys, AgentReads, AgentSource, AgentWhoami,
        AgentWhoamiDsp, AgentWhoamiKey, Dsp,
    },
    crypto,
    db::{Store, at, iso, now, s},
    ensure,
};
use rusqlite::params;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

/// Keys that may be in use at once.
const MOST_KEYS: i64 = 50;
/// The furthest a key's expiry may be set, short of never.
const LONGEST: i64 = 5 * 366 * 86_400_000;
/// A key as listed, with whether a connected app can still renew its access: a refresh
/// token neither expired nor exchanged. `?1` is the time now.
const LISTED: &str = "SELECT k.*,EXISTS(SELECT 1 FROM oauth_tokens t WHERE t.key_id=k.id \
    AND t.kind='refresh' AND t.used_at IS NULL AND t.expires_at>?1) signed_in FROM agent_keys k";

/// An agent signed in with a key: what the key may do and read, and the DSPs it reaches now.
#[derive(Clone, Debug)]
pub struct Caller {
    pub key: String,
    pub name: String,
    pub user: String,
    pub access: AgentAccess,
    /// What it reads at a DSP without settings of its own.
    pub reads: AgentReads,
    /// The DSPs with settings of their own, by id.
    pub dsp_reads: HashMap<String, AgentReads>,
    pub expires_at: Option<String>,
    pub dsps: Vec<Dsp>,
    pub client: String,
}
impl Caller {
    /// What it reads at a DSP: the DSP's own settings, or else its own.
    pub fn reads_at(&self, dsp: &str) -> &AgentReads {
        self.dsp_reads.get(dsp).unwrap_or(&self.reads)
    }
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

/// The audit log's field for an app-wide kind of data, as `reads.timecards`.
fn read_field(area: AgentArea) -> &'static str {
    static FIELDS: LazyLock<Vec<(AgentArea, String)>> = LazyLock::new(|| {
        AgentArea::all()
            .map(|area| (area, format!("reads.{}", area.as_str())))
            .collect()
    });
    FIELDS
        .iter()
        .find(|(kind, _)| *kind == area)
        .map(|(_, field)| field.as_str())
        .expect("a declared kind of data")
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
            .one_as(&format!("{LISTED} WHERE k.id=?2"), [iso(), id.to_owned()])?
            .ok_or_else(|| Error::new("agent_key_not_found", 404))?;
        key.dsps = self.agent_key_dsps(id)?;
        key.dsp_reads = self.agent_key_dsp_reads(id)?;
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
    fn agent_key_dsp_reads(&self, id: &str) -> Result<Vec<AgentDspReads>> {
        Ok(self
            .platform
            .query_as::<(String, String, i64)>(
                "SELECT dsp_id,areas,bypass FROM agent_key_dsp_reads WHERE key_id=? \
                 ORDER BY dsp_id",
                [id],
            )?
            .into_iter()
            .map(|(dsp, areas, bypass)| {
                let reads = AgentReads::stored(&areas, bypass);
                AgentDspReads {
                    dsp,
                    areas: reads.areas,
                    bypass: reads.bypass,
                }
            })
            .collect())
    }

    /// Every key, those in use first and newest first, with the DSPs a key can be given and
    /// the features each has switched off. `seen` is what this process knows of each key's
    /// last use, newer than the database.
    pub fn agent_keys(&self, seen: &HashMap<String, LastUse>) -> Result<AgentKeys> {
        let mut keys: Vec<AgentKey> = self.platform.query_as(
            &format!("{LISTED} ORDER BY k.revoked_at IS NOT NULL,k.created_at DESC,k.id"),
            [iso()],
        )?;
        for key in &mut keys {
            key.dsps = self.agent_key_dsps(&key.id)?;
            key.dsp_reads = self.agent_key_dsp_reads(&key.id)?;
            if let Some((when, client)) = seen.get(&key.id) {
                let when = at(*when);
                if key.last_used_at.as_ref().is_none_or(|last| *last < when) {
                    key.last_used_at = Some(when);
                    key.last_client = Some(client.clone());
                }
            }
        }
        let mut dsps = vec![];
        for dsp in self.agent_dsp_choices()? {
            let on = data::switched_on(self, &dsp.id)?;
            dsps.push(AgentKeyDsp {
                switched_off: AgentSource::all()
                    .filter(|source| !on.contains(source))
                    .collect(),
                id: dsp.id,
                name: dsp.name,
            });
        }
        Ok(AgentKeys { keys, dsps })
    }

    // What every new or changed key must satisfy. `before` is the key being changed, as it
    // is. `replaced` are the connected apps a new connection of the same app replaces, which
    // so neither take its name nor count.
    fn check_agent_key(
        &self,
        before: Option<&AgentKey>,
        input: &AgentKeyRequest,
        replaced: &[String],
    ) -> Result<()> {
        let replaced = replaced.len() as i64;
        let taken = self.platform.count(
            "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL AND id<>?1 \
             AND lower(name)=lower(?2)",
            params![before.map_or("", |key| key.id.as_str()), input.name],
        )?;
        ensure(taken - replaced == 0, "agent_key_name_taken", 409)?;
        if before.is_none() {
            let active = self.platform.count(
                "SELECT count(*) FROM agent_keys WHERE revoked_at IS NULL \
                 AND (expires_at IS NULL OR expires_at>?)",
                [iso()],
            )?;
            ensure(active - replaced < MOST_KEYS, "agent_key_limit", 409)?;
        }
        let choices: Vec<String> = self
            .agent_dsp_choices()?
            .into_iter()
            .map(|dsp| dsp.id)
            .collect();
        // A DSP that isn't active, as one suspended or removed, can't be given to a key or
        // given settings of its own. One the key already has stays as it is, with its own
        // settings, for when it is active again: the Agents page doesn't list it, and sends it
        // back unchanged.
        let kept = before.map_or(&[][..], |key| key.dsps.as_slice());
        let kept_reads = before.map_or(&[][..], |key| key.dsp_reads.as_slice());
        ensure(
            input
                .dsps
                .iter()
                .all(|dsp| choices.contains(dsp) || kept.contains(dsp)),
            "invalid_input",
            400,
        )?;
        // A DSP has settings of its own only while the key reaches it.
        ensure(
            input.dsp_reads.iter().all(|own| {
                (choices.contains(&own.dsp) || kept_reads.contains(own))
                    && (input.all_dsps || input.dsps.contains(&own.dsp))
            }),
            "invalid_input",
            400,
        )?;
        Ok(())
    }
    /// The DSPs a key reaches, and the settings of their own any of them has.
    fn set_agent_key_dsps(&self, id: &str, input: &AgentKeyRequest) -> Result<()> {
        self.platform
            .exec("DELETE FROM agent_key_dsps WHERE key_id=?", [id])?;
        for dsp in &input.dsps {
            self.platform.exec(
                "INSERT OR IGNORE INTO agent_key_dsps(key_id,dsp_id) VALUES (?,?)",
                [id, dsp.as_str()],
            )?;
        }
        self.platform
            .exec("DELETE FROM agent_key_dsp_reads WHERE key_id=?", [id])?;
        for own in &input.dsp_reads {
            self.platform.exec(
                "INSERT INTO agent_key_dsp_reads(key_id,dsp_id,areas,bypass) VALUES (?,?,?,?)",
                params![id, own.dsp, own.reads().areas_text(), i64::from(own.bypass)],
            )?;
        }
        Ok(())
    }
    /// DSPs' own settings as the audit log shows them: "Summit Delivery: 7 of 9, bypass on;
    /// Harbor Route Co: 9 of 9", or "none".
    fn dsp_reads_line(&self, own: &[AgentDspReads]) -> Result<String> {
        if own.is_empty() {
            return Ok("none".into());
        }
        let names: HashMap<String, String> = self
            .platform
            .query_as::<(String, String)>("SELECT id,name FROM dsps", [])?
            .into_iter()
            .collect();
        let mut lines: Vec<String> = own
            .iter()
            .map(|own| {
                let name = names.get(&own.dsp).unwrap_or(&own.dsp);
                let bypass = if own.bypass { ", bypass on" } else { "" };
                let count = own.areas.len();
                format!("{name}: {count} of {}{bypass}", AgentArea::all().count())
            })
            .collect();
        lines.sort_by_key(|line| line.to_lowercase());
        Ok(lines.join("; "))
    }

    /// Makes a key. The key itself is returned this once; only its hash is kept.
    pub fn create_agent_key(&self, user: &str, input: &AgentKeyRequest) -> Result<AgentKeyCreated> {
        self.check_agent_key(None, input, &[])?;
        let expires = expiry(input.expires_at.as_deref())?;
        let token = token::new(token::Kind::Key, self.config.env())?;
        let id = crypto::id("agentkey")?;
        self.platform.transaction(|| {
            // tools and locations as an older release reads them: every tool, and the addresses.
            self.platform.exec(
                "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                 locations,areas,bypass,created_at,expires_at) \
                 VALUES (?,?,?,?,?,?,?,'full',?,?,?,?,?)",
                params![
                    id,
                    input.name,
                    crypto::sha(&token),
                    &token[token.len() - 4..],
                    user,
                    i64::from(input.all_dsps),
                    input.access,
                    i64::from(input.reads.locations()),
                    input.reads.areas_text(),
                    i64::from(input.reads.bypass),
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

    /// Changes what a key or connected app may do and read. A revoked one stays revoked; an
    /// app only ever reads, and never expires.
    pub fn update_agent_key(
        &self,
        user: &str,
        id: &str,
        input: &AgentKeyRequest,
    ) -> Result<AgentKey> {
        let before = self.agent_key(id)?;
        ensure(before.revoked_at.is_none(), "agent_key_revoked", 409)?;
        // An app is edited like a key, but stays read-only and never expires.
        if before.kind == AgentKeyKind::App {
            ensure(
                input.access == AgentAccess::Read && input.expires_at == before.expires_at,
                "invalid_input",
                400,
            )?;
        }
        self.check_agent_key(Some(&before), input, &[])?;
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
        for area in AgentArea::all() {
            change(
                read_field(area),
                before.reads.has(area).to_string(),
                input.reads.has(area).to_string(),
            );
        }
        change(
            "bypass",
            before.reads.bypass.to_string(),
            input.reads.bypass.to_string(),
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
        // DSPs' own settings changed are noted even where their lines read alike.
        if before.dsp_reads != input.dsp_reads {
            changes.push((
                "dsp_reads",
                Some(self.dsp_reads_line(&before.dsp_reads)?),
                Some(self.dsp_reads_line(&input.dsp_reads)?),
            ));
        }
        let action = match before.kind {
            AgentKeyKind::Key => "agent.key_updated",
            AgentKeyKind::App => "agent.app_updated",
        };
        self.platform.transaction(|| {
            self.platform.exec(
                "UPDATE agent_keys SET name=?,all_dsps=?,access=?,tools='full',locations=?,\
                 areas=?,bypass=?,expires_at=? WHERE id=?",
                params![
                    input.name,
                    i64::from(input.all_dsps),
                    input.access,
                    i64::from(input.reads.locations()),
                    input.reads.areas_text(),
                    i64::from(input.reads.bypass),
                    expires,
                    id
                ],
            )?;
            self.set_agent_key_dsps(id, input)?;
            if !changes.is_empty() {
                self.audit_with(Some(user), None, action, id, Some(&input.name), &changes)?;
            }
            Ok(())
        })?;
        self.agent_key(id)
    }

    /// Revokes a key, or a connected app with its tokens, at once. Revoking a revoked key
    /// changes nothing.
    pub fn revoke_agent_key(&self, user: &str, id: &str) -> Result<AgentKey> {
        let key = self.agent_key(id)?;
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
        self.agent_key(id)
    }

    /// Revokes every key and connected app still in use: all of them, or only `owner`'s, with
    /// the tokens of the apps and the approvals not yet redeemed. Answers how many.
    pub fn revoke_agent_keys(&self, actor: Option<&str>, owner: Option<&str>) -> Result<usize> {
        self.platform
            .transaction(|| self.revoke_agent_keys_within(actor, owner))
    }
    /// The same inside a transaction the caller holds, so the revocation stands or falls
    /// with what it belongs to, such as a password reset.
    pub(crate) fn revoke_agent_keys_within(
        &self,
        actor: Option<&str>,
        owner: Option<&str>,
    ) -> Result<usize> {
        let revoked = self.platform.exec(
            "UPDATE agent_keys SET revoked_at=?1 WHERE revoked_at IS NULL \
             AND (?2 IS NULL OR user_id=?2)",
            params![iso(), owner],
        )?;
        self.platform.exec(
            "DELETE FROM oauth_tokens WHERE key_id IN \
             (SELECT id FROM agent_keys WHERE revoked_at IS NOT NULL)",
            [],
        )?;
        self.platform.exec(
            "DELETE FROM oauth_codes WHERE used_at IS NULL AND (?1 IS NULL OR approved_by=?1)",
            [owner],
        )?;
        if revoked > 0 {
            self.audit(actor, None, "agent.keys_revoked", &revoked.to_string())?;
        }
        Ok(revoked)
    }

    /// The agent a key belongs to, or why it may not sign in.
    pub fn authenticate_agent(&self, token: &str, client: &str) -> Result<Caller> {
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
                "SELECT k.id,k.name,k.user_id,k.all_dsps,k.access,k.areas,k.bypass,k.locations,\
                 k.expires_at,k.revoked_at,u.platform_owner,u.status FROM agent_keys k \
                 JOIN users u ON u.id=k.user_id WHERE k.hash=?",
                [crypto::sha(token)],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        self.agent_caller(&row, client)
    }

    /// Refresh an admitted key while the caller holds the same State read lock
    /// used for discovery or protected data. Admission's snapshot is not policy.
    pub(crate) fn revalidate_agent(&self, caller: &Caller) -> Result<Caller> {
        let row = self
            .platform
            .one(
                "SELECT k.id,k.name,k.user_id,k.all_dsps,k.access,k.areas,k.bypass,k.locations,\
             k.expires_at,k.revoked_at,u.platform_owner,u.status FROM agent_keys k \
             JOIN users u ON u.id=k.user_id WHERE k.id=? AND k.user_id=?",
                [&caller.key, &caller.user],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        self.agent_caller(&row, &caller.client)
    }

    fn agent_caller(&self, row: &serde_json::Value, client: &str) -> Result<Caller> {
        let environment = self.config.env();
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
        let dsp_reads = self
            .agent_key_dsp_reads(&key)?
            .into_iter()
            .map(|own| (own.dsp.clone(), own.reads()))
            .collect();
        Ok(Caller {
            name: text("name"),
            user: text("user_id"),
            access: AgentAccess::parse(&text("access"))
                .ok_or_else(|| Error::new("invalid_stored_record", 500))?,
            reads: AgentReads::stored_key(
                &text("areas"),
                row["bypass"].as_i64().unwrap_or(0),
                row["locations"].as_i64().unwrap_or(0),
            ),
            dsp_reads,
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

    /// The pseudonymous platform-owner profile represented by an agent credential.
    pub fn agent_profile(&self, caller: &Caller) -> Result<Value> {
        let user = self
            .platform
            .one(
                "SELECT id,first_name,last_name FROM users WHERE id=? AND status='active' \
                 AND platform_owner=1",
                [&caller.user],
            )?
            .ok_or_else(|| Error::new("agent_key_invalid", 401))?;
        let name = format!("{} {}", s(&user, "first_name"), s(&user, "last_name"));
        // Hosts need one stable profile id across refresh and reconnection, not Dispatch's
        // internal user primary key. Domain separation keeps this opaque identifier unrelated
        // to the signed request/account labels used elsewhere.
        let id = format!(
            "profile_{}",
            crypto::sign(&self.key, &format!("agent-profile:{}", s(&user, "id")))
        );
        Ok(json!({"id":id,"name":name.trim()}))
    }

    /// What an agent is told about itself: its key, the time, and each DSP it reaches with
    /// what it reads there.
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
                reads: caller.reads_at(&dsp.id).clone(),
            });
        }
        Ok(AgentWhoami {
            key: AgentWhoamiKey {
                name: caller.name.clone(),
                access: caller.access,
                expires_at: caller.expires_at.clone(),
            },
            environment: self.config.env(),
            now: iso(),
            dsps,
        })
    }
}

#[cfg(test)]
#[path = "../tests/backend/mod.rs"]
mod tests;
