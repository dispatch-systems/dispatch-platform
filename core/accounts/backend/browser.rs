//! Short-lived sessions issued only by an operator on the server host, separate from human
//! sessions and never conferring fresh-auth authority: observation, which only reads, on Dev
//! or Production; and an agent's, which acts as the platform owner to test changes, on Dev
//! alone.
use super::{Auth, UserRow};
use crate::{
    Error, Result,
    accounts::api::types::PublicUser,
    db::{Store, now},
    ensure,
    foundation::{
        config::{Config, Site},
        crypto,
    },
    server::http::input::Input,
};
use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub const PREFIX: &str = "observation-read:";
/// An agent's session on Dev, which may change what the platform owner may.
pub const WRITE_PREFIX: &str = "agent-write:";
pub const EXCHANGE: &str = "/api/auth/observation";
const LIFETIME: i64 = 15 * 60 * 1000;
const COOKIE: &str = "dispatch_observation_read";
const SECURE_COOKIE: &str = "__Host-dispatch_observation_read";
const WRITE_COOKIE: &str = "dispatch_agent_write";
const SECURE_WRITE_COOKIE: &str = "__Host-dispatch_agent_write";

/// What a host-issued session lets its holder do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Read, on Dev or Production.
    Read,
    /// Act as the platform owner, on Dev alone: an agent testing a change.
    Write,
}
impl Access {
    fn prefix(self) -> &'static str {
        match self {
            Self::Read => PREFIX,
            Self::Write => WRITE_PREFIX,
        }
    }
    /// Its cookie's name at a plain address, and at an HTTPS one.
    fn cookies(self) -> [&'static str; 2] {
        match self {
            Self::Read => [COOKIE, SECURE_COOKIE],
            Self::Write => [WRITE_COOKIE, SECURE_WRITE_COOKIE],
        }
    }
    fn cookie(self, config: &Config) -> &'static str {
        self.cookies()[usize::from(config.origin.starts_with("https://"))]
    }
    /// The access `raw` was issued with, by its prefix.
    fn of(raw: &str) -> Option<Self> {
        [Self::Read, Self::Write]
            .into_iter()
            .find(|access| raw.starts_with(access.prefix()))
    }
}
/// Writes only where agents may: Dev, never Production.
fn allowed(access: Access, config: &Config) -> bool {
    access == Access::Read || config.environment.is_preview()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Claims {
    user: String,
    version: i64,
    origin: String,
    environment: String,
    issued_at: i64,
    expires_at: i64,
    nonce: String,
}

/// All browser service identities with no mutation authority, including support leases.
pub fn read_only(auth: &Auth) -> bool {
    auth.raw.starts_with(PREFIX) || crate::support::browser::read_only(auth)
}
/// Whether `auth` is an agent's session, which the request log marks.
pub fn agent(auth: &Auth) -> bool {
    auth.raw.starts_with(WRITE_PREFIX)
}

pub fn cookie_name(config: &Config) -> &'static str {
    Access::Read.cookie(config)
}
/// The cookie that ends `access`'s session in this browser.
fn expired(access: Access, config: &Config) -> String {
    format!(
        "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        access.cookie(config),
        secure(config)
    )
}
/// The cookie that ends an agent's session in this browser, as signing out does.
pub fn expire_agent(config: &Config) -> String {
    expired(Access::Write, config)
}
fn secure(config: &Config) -> &'static str {
    if config.origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    }
}

/// Whether a cookie of `access`'s names is sent. Presence matters even when a value is empty
/// or malformed: never regain human authority.
fn sent(headers: &HeaderMap, access: Access) -> bool {
    headers.get_all("cookie").iter().any(|header| {
        header.as_bytes().split(|byte| *byte == b';').any(|part| {
            let name = part.split(|byte| *byte == b'=').next().unwrap_or_default();
            let name = name.trim_ascii();
            access
                .cookies()
                .iter()
                .any(|cookie| name == cookie.as_bytes())
        })
    })
}
/// Whether an agent's cookie is sent, valid or not: what the request log marks.
pub fn agent_cookie(headers: &HeaderMap) -> bool {
    sent(headers, Access::Write)
}
/// Whether an observation cookie is sent.
pub fn present(headers: &HeaderMap) -> bool {
    sent(headers, Access::Read)
}

/// A misplaced observation credential still cannot enable a public mutation route.
pub fn restricted_request(headers: &HeaderMap) -> bool {
    present(headers)
        || headers.get_all("cookie").iter().any(|header| {
            header.as_bytes().split(|byte| *byte == b';').any(|part| {
                part.splitn(2, |byte| *byte == b'=')
                    .nth(1)
                    .is_some_and(|value| value.trim_ascii().starts_with(PREFIX.as_bytes()))
            })
        })
}

/// Refuses duplicate or alternate-name cookies instead of falling back to a human session.
pub fn token<'a>(headers: &'a HeaderMap, config: &Config) -> Result<Option<&'a str>> {
    if !present(headers) {
        ensure(!restricted_request(headers), "sign_in_required", 401)?;
        return Ok(None);
    }
    cookie_token(headers, config, Access::Read).map(Some)
}
/// An agent's session cookie, refused as an observation one is when duplicated, under the
/// other name, or not an agent's. None when there is none.
pub fn write_token<'a>(headers: &'a HeaderMap, config: &Config) -> Result<Option<&'a str>> {
    if !sent(headers, Access::Write) {
        return Ok(None);
    }
    cookie_token(headers, config, Access::Write).map(Some)
}
fn cookie_token<'a>(headers: &'a HeaderMap, config: &Config, access: Access) -> Result<&'a str> {
    let mut found = None;
    for header in headers.get_all("cookie") {
        for part in header.to_str().map_err(|_| invalid())?.split(';') {
            let part = part.trim();
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            if access.cookies().contains(&name) {
                ensure(
                    name == access.cookie(config)
                        && found.is_none()
                        && value.starts_with(access.prefix()),
                    "sign_in_required",
                    401,
                )?;
                found = Some(value);
            }
        }
    }
    found.ok_or_else(invalid)
}

/// The loopback health-check alias is not this credential's canonical browser audience.
pub fn at_origin(config: &Config, input: &Input) -> Result<()> {
    let origin = url::Url::parse(&config.origin).map_err(|_| invalid())?;
    let expected = &origin[url::Position::BeforeHost..url::Position::AfterPort];
    ensure(
        input.site == Site::Admin && input.header("host").eq_ignore_ascii_case(expected),
        "sign_in_required",
        401,
    )
}

fn invalid() -> Error {
    Error::new("sign_in_required", 401)
}

fn claims(db: &Store, raw: &str, site: &Site) -> Result<(Claims, Access)> {
    let access = Access::of(raw).ok_or_else(invalid)?;
    ensure(
        site == &Site::Admin && raw.len() <= 2048 && allowed(access, &db.config),
        "sign_in_required",
        401,
    )?;
    let (encoded, signature) = raw
        .strip_prefix(access.prefix())
        .and_then(|raw| raw.split_once(':'))
        .ok_or_else(invalid)?;
    let unsigned = format!("{}{encoded}", access.prefix());
    ensure(
        crypto::equal(&crypto::sign(&db.key, &unsigned), signature),
        "sign_in_required",
        401,
    )?;
    let value: Claims = serde_json::from_slice(&B64.decode(encoded).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let at = now();
    ensure(
        value.origin == db.config.origin
            && value.environment == db.config.environment.as_str()
            && value.issued_at <= at
            && value.expires_at > at
            && value.expires_at.checked_sub(value.issued_at) == Some(LIFETIME)
            && value.nonce.len() == 43
            && value
                .nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'),
        "sign_in_required",
        401,
    )?;
    Ok((value, access))
}

fn owner(db: &Store, value: &Claims) -> Result<PublicUser> {
    let row = UserRow::find(&db.platform, "id", &value.user)?
        .filter(|row| row.active() && row.user.platform_owner && row.version == value.version)
        .ok_or_else(invalid)?;
    Ok(row.user)
}

pub(crate) fn identity(db: &Store, raw: &str, site: &Site) -> Result<PublicUser> {
    owner(db, &claims(db, raw, site)?.0)
}

/// Exchanges the operator credential without writing a login/session row or an audit visit.
pub fn exchange(db: &Store, input: &Input, raw: &str) -> Result<crate::server::http::input::Reply> {
    at_origin(&db.config, input)?;
    let (value, access) = claims(db, raw, &input.site)?;
    let user = owner(db, &value)?;
    {
        let mut trace = input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        trace.actor = Some(user.id);
    }
    let seconds = (value.expires_at - now()).max(0) / 1000;
    // The other access's session ends here, so the one just exchanged is the one used: an
    // observation cookie left beside an agent's would keep the tab read only.
    let other = match access {
        Access::Read => Access::Write,
        Access::Write => Access::Read,
    };
    Ok(crate::server::http::input::Reply::json(
        json!({"ok":true,"readOnly":access == Access::Read,"expiresAt":value.expires_at}),
    )
    .cookie(format!(
        "{}={raw}; Path=/; HttpOnly; SameSite=Strict; Max-Age={seconds}{}",
        access.cookie(&db.config),
        secure(&db.config)
    ))
    .cookie(expired(other, &db.config)))
}

fn private_existing(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure(
        path.is_absolute()
            && fs::canonicalize(path)? == path
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file() && metadata.nlink() == 1
            },
        "private_observation_path_required",
        400,
    )
}

/// Uses only existing host storage, through a read-only SQLite connection. The bearer is
/// written to a new private file; stdout receives only non-secret metadata. An agent's
/// session is minted on Dev alone.
pub fn mint(config: &Config, output: &Path, email: Option<&str>, access: Access) -> Result<Value> {
    ensure(allowed(access, config), "agent_session_dev_only", 403)?;
    ensure(
        output.is_absolute() && output.file_name().is_some(),
        "absolute_observation_output_required",
        400,
    )?;
    private_existing(output.parent().ok_or_else(invalid)?, true)?;
    let account_path = config.platform().join("accounts.sqlite");
    let key_path = config.platform().join("platform.key");
    private_existing(&account_path, false)?;
    private_existing(&key_path, false)?;
    let key = fs::read(key_path)?;
    ensure(key.len() == 32, "invalid_key", 500)?;
    let connection = Connection::open_with_flags(
        account_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut statement = connection.prepare(
        "SELECT id,version FROM users WHERE status='active' AND platform_owner=1 \
         AND (?1 IS NULL OR email=?1) LIMIT 2",
    )?;
    let rows = statement
        .query_map(params![email], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure(rows.len() == 1, "observation_owner_required", 409)?;
    let (user, version) = rows.into_iter().next().unwrap();
    let issued_at = now();
    let value = Claims {
        user,
        version,
        origin: config.origin.clone(),
        environment: config.environment.as_str().into(),
        issued_at,
        expires_at: issued_at + LIFETIME,
        nonce: crypto::token()?,
    };
    let unsigned = format!(
        "{}{}",
        access.prefix(),
        B64.encode(serde_json::to_vec(&value)?)
    );
    let token = format!("{unsigned}:{}", crypto::sign(&key, &unsigned));
    let read_only = access == Access::Read;
    let metadata =
        json!({"origin":config.origin,"expiresAt":value.expires_at,"readOnly":read_only});
    let credential = json!({"origin":config.origin,"token":token,
        "expiresAt":value.expires_at,"readOnly":read_only});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)?;
    let result: Result<Value> = (|| {
        file.write_all(&serde_json::to_vec(&credential)?)?;
        file.sync_all()?;
        Ok(metadata)
    })();
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

#[cfg(test)]
#[path = "../tests/backend/browser.rs"]
mod tests;
