//! What a collector declares: a provider the platform reads, its collections and what its
//! connection supplies. Core reaches a collector through `Provider` and the registry, never by
//! name; what a collection brings is a feature's `Keeper`'s (`keeping`).
use crate::{
    Code, Result,
    collection::{
        browser::{Collected, Driver, Pending, browseros},
        metrics::{Counted, Counts},
    },
    db::{Db, Kind, Migrations, Store},
    manifest::{Tables, registry},
};
use serde_json::Value;
use std::path::Path;

/// One kind of data a collector reads. A job of `job_kind` collects it for one DSP, and
/// the one feature that keeps it stores what the job brings.
pub struct Collection {
    pub job_kind: &'static str,
    /// The schedule `collection` that runs it.
    pub schedule: &'static str,
    /// How Diagnostics and the audit log name it, and the schedules that run it.
    pub label: &'static str,
    /// One item of its workload, as Diagnostics compares its runs per item: "employee".
    pub unit: &'static str,
    /// The count of a run's measurements that is its workload.
    pub counted: Counted,
    /// The error a schedule answers while its collector is not connected.
    pub unconnected: &'static str,
}

/// What a collector's connection supplies to the pages that require it, such as timecards.
#[derive(Clone, Copy, Debug)]
pub struct Capability {
    pub id: &'static str,
    /// How the DSPs page names it where a page needs it: "a timecard source".
    pub label: &'static str,
}

/// Everything the platform needs to know about one provider: how to reach it and read
/// what it offers. Storage, credentials, the browser and the job queue ask here instead
/// of matching on the provider; what a collection brings is a feature's `Keeper`'s.
pub trait Collector: Sync {
    /// Names its connection row, its files, its secrets and its API path.
    fn id(&self) -> &'static str;
    /// Its name as the dashboard shows it.
    fn label(&self) -> &'static str;
    /// What its connection supplies to the pages that require it (`features`).
    fn capabilities(&self) -> &'static [Capability];
    /// What it reads, its main collection first; each is chosen by `job_kind_for`.
    fn collections(&self) -> &'static [Collection];
    /// The kind of job a request queues.
    fn job_kind_for(&self, _request: &Value) -> &'static str {
        self.collections()[0].job_kind
    }
    /// Normalizes a trusted read-only verification request for this isolated DSP. A
    /// collector must opt in and bind its own provider scope; future collectors fail closed.
    fn verification_request(&self, _: &Store, _dsp: &str, _input: &Value) -> Result<Value> {
        Err(crate::Error::new("support_verification_unsupported", 403))
    }
    /// Its database. Its migrations are every owner's, its own included (`migrations`).
    fn database(&self) -> Kind;
    /// What it adds to databases, starting with its own's baseline.
    fn migrations(&self) -> &'static [Migrations];
    /// The tables it keeps in its database, beside core's and its collections' keepers'.
    fn tables(&self) -> Tables {
        &[]
    }
    /// Written into a new database with its schema. Must identify the storage.
    fn seed(&self, dsp: &str) -> String;
    /// The DSP setting recording that this storage was added to an existing DSP.
    /// `None` only for Paycom, whose storage every DSP was created with.
    fn marker(&self) -> Option<&'static str>;
    /// Fails closed when initialized storage lost what it must hold.
    fn verify(&self, _: &Db) -> Result<()> {
        Ok(())
    }
    /// After every collector of a DSP opened at startup.
    fn opened(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// Writes what a new DSP's storage holds beyond `seed`, once all of its storage exists.
    fn provision(&self, _: &Store, _dsp: &str, _timezone: &str) -> Result<()> {
        Ok(())
    }
    /// Connects a development seed's DSP with fixture credentials, as its owner would.
    fn demo(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// What a credential change removes from `state/browsers`.
    fn browser_entries(&self) -> &'static [&'static str];
    /// The hosts its browser may reach, as its own `HostPolicy`.
    fn network(&self) -> browseros::NetworkPolicy;
    fn validate_credentials(&self, value: &Value) -> Result<()>;
    /// Shown beside the connection. Never a secret.
    fn account_label<'a>(&self, _credentials: &'a Value) -> &'a str {
        ""
    }
    /// Its driver, on a browser already running under `network`. `fixture` is the
    /// origin of a local stand-in for the provider's site.
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>>;
    /// What a collection returns in fixture mode, where no browser runs.
    fn fixture(&self, timezone: &str, request: &Value) -> Result<Collected>;
    /// The job's message while it collects `request`.
    fn progress(&self, _request: &Value) -> &'static str;
    /// What a finished collection brought, as the job's metrics count it.
    fn counts(&self, _data: &Value) -> Counts {
        Counts::default()
    }
    /// Whether its collections bring the whole roster, so that a finished one naming
    /// no single employee may have changed who is on it.
    fn roster(&self) -> bool {
        false
    }
    /// Those of its codes a failed attempt is queued again after, while attempts remain.
    fn retryable(&self) -> &'static [Code] {
        &[]
    }
    /// Work a finished collection needs before it is stored, done without the database
    /// and outside the platform lock: parsing, shaping rows, compressing.
    fn prepare(&self, collected: Collected) -> Result<Collected> {
        Ok(collected)
    }
    /// Drops what an unfinished job kept to resume from. `None` means every job.
    fn discard(&self, _: &Store, _dsp: &str, _job: Option<&str>) -> Result<()> {
        Ok(())
    }
    /// Drops what it kept to resume jobs that have ended or expired. The platform's
    /// periodic cleanup runs it for every DSP.
    fn prune(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// Runs with the connection's own disable, in its transaction.
    fn disabled(&self, _: &Db) -> Result<()> {
        Ok(())
    }
}

/// Core's retryable error codes, joined by the ones each registered collector declares.
impl Code {
    /// A failed attempt with one of these is queued again while attempts remain: core's
    /// own, then each collector's.
    pub fn retryable() -> impl Iterator<Item = Code> {
        let collectors = registry().collectors.iter().flat_map(|c| c.retryable());
        Code::RETRYABLE.iter().chain(collectors).copied()
    }
}
