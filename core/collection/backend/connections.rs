//! Provider connection state, encrypted credentials, and cross-process account leases.
use crate::{
    Error, Result,
    accounts::Context,
    collection::{
        api::types::{Connection, ConnectionCheck},
        browser::egress,
        registry::Provider,
    },
    db::{self, FromRow, Row, Store, iso},
    ensure,
    foundation::crypto,
};
use fs2::FileExt;
use rusqlite::params;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// What a worker needs to know about a provider's connection before it collects.
pub(crate) struct ConnectionLease {
    pub enabled: bool,
    pub revision: i64,
}
impl FromRow for ConnectionLease {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            enabled: row.get("enabled")?,
            revision: row.get("revision")?,
        })
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

/// Where a DSP's settings keep what its connection to `provider` last checked.
fn checks_key(provider: Provider) -> String {
    format!("connection.checks.{}", provider.id())
}

impl Store {
    pub(crate) fn connection_lease(
        &self,
        dsp: &str,
        provider: Provider,
    ) -> Result<Option<ConnectionLease>> {
        self.collector(dsp, provider)?.one_as(
            "SELECT enabled,revision FROM connections WHERE provider=?",
            [provider.id()],
        )
    }
    /// The revision of the DSP's connection to `provider` while it's enabled; none while it's
    /// disabled or the DSP has none. Replacing its credentials or disabling it moves it on, so
    /// what was taken from a sign-in under one revision can be told stale under another.
    pub fn connection_revision(&self, id: &str, provider: Provider) -> Result<Option<i64>> {
        Ok(self
            .connection_lease(id, provider)?
            .filter(|lease| lease.enabled)
            .map(|lease| lease.revision))
    }
    pub fn connection_for(&self, id: &str, provider: Provider) -> Result<Connection> {
        let mut connection: Connection = self
            .collector(id, provider)?
            .one_as(
                "SELECT provider,enabled,status,error,updated_at,verified_at,account_label \
                 FROM connections WHERE provider=?",
                [provider.id()],
            )?
            .ok_or_else(|| Error::new("connection_required", 409))?;
        // Kept under the revision they were found at, so new credentials start without them.
        let kept = self.dsp(id)?.setting(&checks_key(provider), Value::Null)?;
        let revision = self.connection_revision(id, provider)?;
        if revision.is_some() && kept["revision"].as_i64() == revision {
            connection.checks = serde_json::from_value(kept["checks"].clone())?;
        }
        if provider.collector().allows_by_address() {
            connection.source_address = egress::source_address(&self.config);
        }
        Ok(connection)
    }
    /// Keeps what the DSP's connection to `provider` checked as it connected at `revision`,
    /// in the DSP's own settings: no more than how each check went, by its code.
    pub(crate) fn connection_checks(
        &self,
        id: &str,
        provider: Provider,
        revision: i64,
        checks: &[ConnectionCheck],
    ) -> Result<()> {
        self.dsp(id)?.set(
            &checks_key(provider),
            &json!({"revision":revision,"checks":checks}),
        )
    }
    pub fn connection_state(
        &self,
        id: &str,
        provider: Provider,
        revision: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.collector(id, provider)?.exec(
            "UPDATE connections SET \
            status=?,error=?,updated_at=?,verified_at=CASE WHEN ?='ready' THEN ? ELSE \
            verified_at END WHERE provider=? AND revision=? AND \
            enabled=1",
            params![status, error, iso(), status, iso(), provider.id(), revision],
        )?;
        Ok(())
    }
    /// Marks a connection ready on fixture credentials, verified now, as a development
    /// seed's DSP shows it.
    pub fn connect_demo(&self, id: &str, provider: Provider, label: &str) -> Result<()> {
        self.collector(id, provider)?.exec(
            "UPDATE connections SET enabled=1,status='ready',account_label=?,verified_at=?",
            [label, &iso()],
        )?;
        Ok(())
    }
    /// Whether the DSP may connect to `provider` with these credentials here: valid as its
    /// collector reads them, and, when they need no browser, ones its collector will connect
    /// with. Asked before anything of the connection the DSP has is stopped or replaced, so
    /// credentials refused leave it as it was.
    pub fn accepts_credentials(&self, id: &str, provider: Provider, value: &Value) -> Result<()> {
        provider.validate_credentials(value)?;
        provider.collector().direct(self, id, value).map(drop)
    }
    pub fn save_credentials(&self, c: &Context, value: &Value, provider: Provider) -> Result<()> {
        self.revalidate(c, "connections.manage")?;
        let id = c.dsp.id.as_str();
        self.accepts_credentials(id, provider, value)?;
        let area = self.area(id, "secrets")?;
        let key = db::key_file(&area.join("vault.key"))?;
        db::write_private(
            &area.join(format!("{}.enc", provider.id())),
            crypto::encrypt(&key, &format!("{id}:{}:2", provider.id()), value)?.as_bytes(),
        )?;
        self.collector(id, provider)?.exec("UPDATE connections SET \
            enabled=1,status='not_connected',error=NULL,account_label=?,verified_at=NULL,revision=revision+1,updated_at=? WHERE provider=?",
                [provider.collector().account_label(value),&iso(),provider.id()])?;

        self.clear_collector_browser_state(id, provider)?;
        c.audit(self, "connection.credentials_saved", provider.id())
    }
    pub fn credentials(&self, id: &str, provider: Provider) -> Result<Value> {
        let area = self.area(id, "secrets")?;
        let path = area.join(format!("{}.enc", provider.id()));
        db::private_file(&path, false)?;
        let key = db::key_file(&area.join("vault.key"))?;
        crypto::decrypt(
            &key,
            &format!("{id}:{}:2", provider.id()),
            &std::fs::read_to_string(path)?,
        )
    }
    pub fn disable(&self, c: &Context, remove: bool, provider: Provider) -> Result<()> {
        self.revalidate(c, "connections.manage")?;
        let id = c.dsp.id.as_str();
        let db = self.collector(id, provider)?;
        db.transaction(|| {
            db.exec(
                "UPDATE connections SET \
                enabled=0,status='not_connected',error=NULL,revision=revision+1,updated_at=? \
                WHERE provider=?",
                [iso(), provider.id().into()],
            )?;
            provider.collector().disabled(&db)
        })?;
        self.pause_provider_schedules(id, provider)?;
        self.clear_collector_browser_state(id, provider)?;
        if remove {
            let file = self
                .area(id, "secrets")?
                .join(format!("{}.enc", provider.id()));
            db::private_file(&file, false)?;
            if file.exists() {
                std::fs::remove_file(file)?;
            }
        }
        // Whether its saved login went with it is a fact of the event.
        let deleted = [("login", None, Some("deleted".to_owned()))];
        self.audit_with(
            Some(c.actor()),
            Some(id),
            "connection.disabled",
            provider.id(),
            None,
            if remove { &deleted } else { &[] },
        )
    }
    /// The lease of the DSP's account with `provider` a provider browser starts with, as
    /// `credential_lease`, while this backend serves the DSP: checked in the turn that takes it,
    /// so a start Disable and Delete finish around makes nothing in the deleted DSP's folder.
    pub fn browser_lease(&self, dsp: &str, provider: Provider) -> Result<File> {
        self.ensure_dsp_active(dsp)?;
        credential_lease(&self.config.root, dsp, provider)
    }
}
