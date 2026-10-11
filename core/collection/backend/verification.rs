//! Trusted, dev-only collection verification. Credentials stay between the vault and the
//! collector; agents receive only aggregate outcomes. The CLI runs the assigned branch's
//! normal job/driver/keeper path against an isolated DSP.
use crate::{
    Error, Result, State,
    collection::{api::jobs::JobStatus, registry::Provider},
    db::{self, Store},
    ensure,
    foundation::{config::Config, crypto},
};
use fs2::FileExt;
use rusqlite::OpenFlags;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

/// Ticket investigations need provider/job health, never payroll records or credential
/// labels. The caller supplies its already-authorized DSP and ticket-derived time window.
pub fn diagnostics(db: &Store, dsp: &str, since: i64) -> Result<Value> {
    db.ensure_dsp_active(dsp)?;
    let since = since
        .max(db::now() - 7 * 24 * 60 * 60 * 1000)
        .min(db::now());
    let connections = Provider::all()
        .map(|provider| {
            let connection = db.connection_for(dsp, provider)?;
            Ok(json!({
                "provider":provider.id(),
                "enabled":connection.enabled,
                "status":connection.status,
                "error":connection.error.as_deref().map(error_code)
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let jobs = db
        .recent_jobs(Some(dsp))?
        .into_iter()
        .filter(|job| {
            chrono::DateTime::parse_from_rfc3339(&job.created_at)
                .is_ok_and(|date| date.timestamp_millis() >= since)
        })
        .take(20)
        .map(|job| {
            json!({
                "kind":job.kind,
                "status":job.status,
                "error":job.error.as_deref().map(error_code),
                "createdAt":job.created_at,
                "completedAt":job.completed_at
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"since":since,"connections":connections,"jobs":jobs}))
}

fn error_code(code: &str) -> &str {
    if code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        code
    } else {
        "provider_failed"
    }
}

/// One account may be used by one provider browser or trusted verifier at a time, across
/// processes as well as ordinary job scheduling. Dropping the file releases its lease.
pub fn credential_lease(root: &Path, dsp: &str, provider: Provider) -> Result<File> {
    ensure(db::identifier(dsp, "dsp_"), "invalid_dsp_id", 400)?;
    let directory = db::private_dir(&root.join("dsps").join(dsp).join("state/credential-leases"))?;
    let path = directory.join(format!("{}.lock", provider.id()));
    db::private_file(&path, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.try_lock_exclusive()
        .map_err(|_| Error::new("browser_capacity_busy", 429))?;
    Ok(file)
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub source_root: PathBuf,
    pub source_dsp: String,
    pub target_dsp: String,
    pub provider: String,
    pub request: Value,
    pub ticket_id: String,
}

struct Credentials {
    path: PathBuf,
    _account_lease: File,
    source_connection: PathBuf,
    revision: i64,
    provider: Provider,
}
impl Credentials {
    fn current(&self) -> Result<bool> {
        let connection = rusqlite::Connection::open_with_flags(
            &self.source_connection,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let current: (bool, i64) = connection.query_row(
            "SELECT enabled,revision FROM connections WHERE provider=?",
            [self.provider.id()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(current.0 && current.1 == self.revision)
    }
}
impl Drop for Credentials {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn install_credentials(db: &Store, request: &Request, provider: Provider) -> Result<Credentials> {
    ensure(
        db.config.development && db.config.environment == "preview" && !db.config.fixture,
        "support_verification_dev_only",
        403,
    )?;
    ensure(
        request.source_root.is_absolute()
            && request.source_root.canonicalize()? == request.source_root,
        "unsafe_storage_path",
        400,
    )?;
    ensure(
        request
            .source_root
            .file_name()
            .is_some_and(|name| name == "dev")
            && !db.config.root.starts_with(&request.source_root)
            && !request.source_root.starts_with(&db.config.root),
        "support_verification_isolation_required",
        403,
    )?;
    ensure(
        db::identifier(&request.source_dsp, "dsp_")
            && db::identifier(&request.target_dsp, "dsp_")
            && db::identifier(&request.ticket_id, "support_"),
        "invalid_verification_reference",
        400,
    )?;
    let platform = request.source_root.join("data/platform/accounts.sqlite");
    db::private_file(&platform, false)?;
    let accounts =
        rusqlite::Connection::open_with_flags(platform, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let active: bool = accounts.query_row(
        "SELECT EXISTS(SELECT 1 FROM dsps WHERE id=? AND status='active')",
        [&request.source_dsp],
        |row| row.get(0),
    )?;
    ensure(active, "support_verification_reference_required", 403)?;
    let (name, timezone): (String, String) = accounts.query_row(
        "SELECT name,timezone FROM dsps WHERE id=?",
        [&request.source_dsp],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let source_profile = request
        .source_root
        .join("dsps")
        .join(&request.source_dsp)
        .join("data/dispatch.sqlite");
    db::private_file(&source_profile, false)?;
    let profile =
        rusqlite::Connection::open_with_flags(source_profile, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let profile: String = profile.query_row(
        "SELECT value FROM settings WHERE key='dsp.profile'",
        [],
        |row| row.get(0),
    )?;
    let profile: Value = serde_json::from_str(&profile)?;
    db.set_verification_profile(
        &request.target_dsp,
        &name,
        &timezone,
        profile["abbreviation"].as_str().unwrap_or_default(),
        profile["stationCode"].as_str().unwrap_or_default(),
    )?;
    let _account_lease = credential_lease(&request.source_root, &request.source_dsp, provider)?;
    let source_connection = request
        .source_root
        .join("dsps")
        .join(&request.source_dsp)
        .join("data")
        .join(provider.id())
        .join(format!("{}.sqlite", provider.id()));
    db::private_file(&source_connection, false)?;
    let connection = rusqlite::Connection::open_with_flags(
        &source_connection,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let (enabled, revision): (bool, i64) = connection.query_row(
        "SELECT enabled,revision FROM connections WHERE provider=?",
        [provider.id()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure(enabled, "connection_required", 409)?;
    let area = request
        .source_root
        .join("dsps")
        .join(&request.source_dsp)
        .join("secrets");
    let source = area.join(format!("{}.enc", provider.id()));
    db::private_file(&source, false)?;
    ensure(
        source.is_file() && area.join("vault.key").is_file(),
        "connection_required",
        409,
    )?;
    let key = db::key_file(&area.join("vault.key"))?;
    let credentials = crypto::decrypt(
        &key,
        &format!("{}:{}:2", request.source_dsp, provider.id()),
        &std::fs::read_to_string(source)?,
    )?;
    provider.validate_credentials(&credentials)?;
    db.ensure_dsp_active(&request.target_dsp)?;
    let target = db.area(&request.target_dsp, "secrets")?;
    let path = target.join(format!("{}.enc", provider.id()));
    ensure(
        !path.exists(),
        "support_verification_isolation_required",
        403,
    )?;
    let key = db::key_file(&target.join("vault.key"))?;
    db::write_private(
        &path,
        crypto::encrypt(
            &key,
            &format!("{}:{}:2", request.target_dsp, provider.id()),
            &credentials,
        )?
        .as_bytes(),
    )?;
    let retained = Credentials {
        path,
        _account_lease,
        source_connection,
        revision,
        provider,
    };
    db.collector(&request.target_dsp, provider)?.exec(
        "UPDATE connections SET enabled=1,status='not_connected',error=NULL,verified_at=NULL,\
         revision=revision+1,updated_at=?,account_label=? WHERE provider=?",
        [
            db::iso(),
            provider.collector().account_label(&credentials).to_owned(),
            provider.id().into(),
        ],
    )?;
    Ok(retained)
}

/// Runs without a web server, through the same claim/driver/publication path as normal
/// collections. Interactive provider challenges become an explicit waiting outcome.
pub async fn run(
    config: Config,
    input: &[u8],
    authority: crate::collection::browser::browseros::VerificationAuthority,
) -> Result<Value> {
    ensure(input.len() <= 16384, "invalid_input", 400)?;
    let request: Request = serde_json::from_slice(input)?;
    ensure(
        config.development && config.environment == "preview" && !config.fixture,
        "support_verification_dev_only",
        403,
    )?;
    ensure(
        request.source_root.is_absolute()
            && request.source_root.canonicalize()? == request.source_root
            && request
                .source_root
                .file_name()
                .is_some_and(|name| name == "dev")
            && !config.root.starts_with(&request.source_root)
            && !request.source_root.starts_with(&config.root),
        "support_verification_isolation_required",
        403,
    )?;
    ensure(
        db::identifier(&request.source_dsp, "dsp_")
            && db::identifier(&request.target_dsp, "dsp_")
            && db::identifier(&request.ticket_id, "support_"),
        "invalid_verification_reference",
        400,
    )?;
    let provider = Provider::parse(&request.provider)?;
    ensure(
        matches!(provider.id(), "paycom" | "cortex"),
        "invalid_provider",
        400,
    )?;
    ensure(request.request.is_object(), "invalid_input", 400)?;
    let state = State::for_collection_verification(config, authority)?;
    let retained = state
        .run({
            let request = request.clone();
            move |db| install_credentials(db, &request, provider)
        })
        .await?;
    let (dsp, key, job_request) = (
        request.target_dsp.clone(),
        format!(
            "support:{}:{}:{}",
            request.ticket_id,
            provider.id(),
            crypto::id("run")?
        ),
        request.request.clone(),
    );
    let id = state
        .run(move |db| {
            let job_request = provider
                .collector()
                .verification_request(db, &dsp, &job_request)?;
            let public = db.enqueue_for(&dsp, None, &key, provider, &job_request)?;
            Ok(public["id"]
                .as_str()
                .ok_or_else(|| Error::new("operation_failed", 500))?
                .to_owned())
        })
        .await?;
    let owner = crypto::id("worker")?;
    let target = request.target_dsp.clone();
    let claimed = state
        .run_bookkeeping({
            let owner = owner.clone();
            move |db| {
                db.claim_job(&owner, |dsp, candidate| {
                    dsp == target && candidate == provider
                })
            }
        })
        .await?
        .ok_or_else(|| Error::new("connection_busy", 409))?;
    ensure(
        claimed.id == id,
        "support_verification_isolation_required",
        403,
    )?;
    let executing = tokio::spawn(crate::collection::jobs::verify(
        state.clone(),
        claimed,
        owner,
    ));
    let mut waiting = false;
    let mut revoked = false;
    loop {
        if !retained.current()? {
            revoked = true;
            let (job_id, dsp) = (id.clone(), request.target_dsp.clone());
            state
                .run(move |db| db.cancel(&job_id, &dsp).map(|_| ()))
                .await?;
            executing.abort();
            break;
        }
        if executing.is_finished() {
            break;
        }
        let job_id = id.clone();
        let job = state.read(move |db| db.job_row(&job_id, None)).await?;
        if job.status == JobStatus::WaitingVerification {
            waiting = true;
            let (job_id, dsp) = (id.clone(), request.target_dsp.clone());
            state
                .run(move |db| db.cancel(&job_id, &dsp).map(|_| ()))
                .await?;
            executing.abort();
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let _ = executing.await;
    state.browsers.close().await;
    let job_id = id.clone();
    let result = state
        .read(move |db| {
            let row = db.job_row(&job_id, None)?;
            let metrics = db.metrics(&job_id)?;
            let last = metrics.last();
            Ok(json!({
                "jobId":job_id,
                "provider":provider.id(),
                "status":if waiting {"waiting_verification"}
                    else if revoked {"reference_changed"} else {row.status.as_str()},
                "passed":!waiting && !revoked && row.status == JobStatus::Succeeded,
                "error":if waiting {Some("verification_required")}
                    else if revoked {Some("connection_changed")} else {row.error.as_deref()},
                "counts":{
                    "employees":last.and_then(|m|m.employees),
                    "timecards":last.and_then(|m|m.timecards),
                    "itineraries":last.and_then(|m|m.itineraries),
                    "rows":last.and_then(|m|m.rows)
                }
            }))
        })
        .await;
    drop(retained);
    result
}

/// Host-side broker for an isolated changed collector. Its policy comes from the installed
/// collector registry, so branch code cannot add arbitrary network destinations.
pub async fn provider_egress(config: Config, provider: &str, run: &Path) -> Result<()> {
    ensure(
        config.development && config.environment == "preview",
        "support_verification_dev_only",
        403,
    )?;
    let provider = Provider::parse(provider)?;
    ensure(
        matches!(provider.id(), "paycom" | "cortex"),
        "invalid_provider",
        400,
    )?;
    let _egress = crate::collection::browser::egress::Egress::start_with_policy(
        run,
        provider.collector().network(),
    )?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod tests;
