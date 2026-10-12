//! Short-lived observation sessions issued only by an operator on the server host.
//! These are separate from human sessions and never confer write or fresh-auth authority.
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
pub const EXCHANGE: &str = "/api/auth/observation";
const LIFETIME: i64 = 15 * 60 * 1000;
const COOKIE: &str = "dispatch_observation_read";
const SECURE_COOKIE: &str = "__Host-dispatch_observation_read";

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

pub fn cookie_name(config: &Config) -> &'static str {
    if config.origin.starts_with("https://") {
        SECURE_COOKIE
    } else {
        COOKIE
    }
}

/// Presence matters even when a value is empty or malformed: never regain human authority.
pub fn present(headers: &HeaderMap) -> bool {
    headers.get_all("cookie").iter().any(|header| {
        header.as_bytes().split(|byte| *byte == b';').any(|part| {
            let name = part.split(|byte| *byte == b'=').next().unwrap_or_default();
            let name = name.trim_ascii();
            name == COOKIE.as_bytes() || name == SECURE_COOKIE.as_bytes()
        })
    })
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
    let mut found = None;
    for header in headers.get_all("cookie") {
        for part in header.to_str().map_err(|_| invalid())?.split(';') {
            let part = part.trim();
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            if name == COOKIE || name == SECURE_COOKIE {
                ensure(
                    name == cookie_name(config) && found.is_none() && value.starts_with(PREFIX),
                    "sign_in_required",
                    401,
                )?;
                found = Some(value);
            }
        }
    }
    found.map(Some).ok_or_else(invalid)
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

fn claims(db: &Store, raw: &str, site: &Site) -> Result<Claims> {
    ensure(
        site == &Site::Admin && raw.len() <= 2048,
        "sign_in_required",
        401,
    )?;
    let (encoded, signature) = raw
        .strip_prefix(PREFIX)
        .and_then(|raw| raw.split_once(':'))
        .ok_or_else(invalid)?;
    let unsigned = format!("{PREFIX}{encoded}");
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
            && value.environment == db.config.environment
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
    Ok(value)
}

fn owner(db: &Store, value: &Claims) -> Result<PublicUser> {
    let row = UserRow::find(&db.platform, "id", &value.user)?
        .filter(|row| row.active() && row.user.platform_owner && row.version == value.version)
        .ok_or_else(invalid)?;
    Ok(row.user)
}

pub(crate) fn identity(db: &Store, raw: &str, site: &Site) -> Result<PublicUser> {
    owner(db, &claims(db, raw, site)?)
}

/// Exchanges the operator credential without writing a login/session row or an audit visit.
pub fn exchange(db: &Store, input: &Input, raw: &str) -> Result<crate::server::http::input::Reply> {
    at_origin(&db.config, input)?;
    let value = claims(db, raw, &input.site)?;
    let user = owner(db, &value)?;
    {
        let mut trace = input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        trace.actor = Some(user.id);
    }
    let secure = if db.config.origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let seconds = (value.expires_at - now()).max(0) / 1000;
    Ok(crate::server::http::input::Reply::json(
        json!({"ok":true,"readOnly":true,"expiresAt":value.expires_at}),
    )
    .cookie(format!(
        "{}={raw}; Path=/; HttpOnly; SameSite=Strict; Max-Age={seconds}{secure}",
        cookie_name(&db.config)
    )))
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
/// written to a new private file; stdout receives only non-secret metadata.
pub fn mint(config: &Config, output: &Path, email: Option<&str>) -> Result<Value> {
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
        environment: config.environment.clone(),
        issued_at,
        expires_at: issued_at + LIFETIME,
        nonce: crypto::token()?,
    };
    let unsigned = format!("{PREFIX}{}", B64.encode(serde_json::to_vec(&value)?));
    let token = format!("{unsigned}:{}", crypto::sign(&key, &unsigned));
    let metadata = json!({"origin":config.origin,"expiresAt":value.expires_at,"readOnly":true});
    let credential =
        json!({"origin":config.origin,"token":token,"expiresAt":value.expires_at,"readOnly":true});
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
