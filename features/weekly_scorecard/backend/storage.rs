//! Weekly Scorecard's database registration, startup compatibility and Store boundary.
use super::{keeper, publication, reads, retirement, settings};
use crate::api::types::WeeklyScorecardWeeks;
use dispatch_core::{
    Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, migrations::add_column, s},
    ensure,
    foundation::weeks::week_or_latest,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    weekly_scorecard::{Capture, DATASETS},
};
use serde_json::{Value, json};

/// The scorecard weeks it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("weekly_scorecard");
/// The weekly scorecard database beside `cortex.sqlite`. Its durable identity stays
/// `weekly_scorecard`; the startup import preserves prior publications.
pub const DATABASE: Kind = Kind::new("weekly_scorecard", 1);
pub static STORAGE: AddedStorage = AddedStorage {
    id: "weekly_scorecard",
    kind: DATABASE,
    marker: "storage.weekly_scorecard",
    source: "weekly-scorecard-v1",
    verify,
};
/// What starting the server does to storage kept before: imports the retired scorecard
/// database, then moves the policy its own database kept into Settings.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    retirement::upgrade(store, id)?;
    settings::upgrade(store, id)
}
fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM weekly_scorecard_schema", [])? == vec![json!({"version":1})],
        "unsupported_weekly_scorecard_schema",
        503,
    )?;
    // Missing initialized tables fail closed, rather than recreating lost data.
    for table in [
        "weekly_scorecard_publications",
        "weekly_scorecard_weeks",
        "weekly_scorecard_sources",
    ]
    .into_iter()
    .chain(DATASETS.iter().map(|d| d.table))
    {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// Whether the current publication and week have verified station scope.
pub(crate) fn add_weekly_verified_scope(db: &Db) -> Result<()> {
    for table in ["weekly_scorecard_publications", "weekly_scorecard_weeks"] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}

/// Preserve the shipped migration's identity and behavior for retired databases.
pub(crate) fn add_verified_scope(db: &Db) -> Result<()> {
    for table in ["scorecard_publications", "scorecard_weeks"] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}

/// What Weekly Scorecard reads and writes for a DSP: its weekly scorecards, in the database beside
/// Cortex's, and the collections that bring them.
pub trait WeeklyScorecardStore {
    fn weekly_scorecard_db(&self, id: &str) -> Result<DspLease<'_>>;
    fn enqueue_weekly_scorecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
    ) -> Result<Value>;
    fn weekly_scorecard_jobs(&self, id: &str) -> Result<Vec<(String, Value)>>;
    fn weekly_scorecard_address(&self, id: &str, station: &str)
    -> Result<Option<(String, String)>>;
    fn publish_weekly_scorecard(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()>;
    fn weekly_scorecard_weeks(&self, id: &str) -> Result<WeeklyScorecardWeeks>;
}
impl WeeklyScorecardStore for Store {
    fn weekly_scorecard_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }
    /// Queues `week`, or the most recent completed one, answering with the job.
    fn enqueue_weekly_scorecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
    ) -> Result<Value> {
        let week = week_or_latest(week, keeper::weekly_scorecard_today(self, id)?)?;
        let request = keeper::weekly_scorecard_request(self, id, &week)?;
        let job = self
            .enqueue_batch(id, actor, &[(key.into(), cortex::PROVIDER, request)])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    fn weekly_scorecard_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        keeper::jobs(self, id)
    }
    /// The API address the station's last publication read, with the company it
    /// named. A job reads there again without opening Cortex's page.
    fn weekly_scorecard_address(
        &self,
        id: &str,
        station: &str,
    ) -> Result<Option<(String, String)>> {
        Ok(self
            .weekly_scorecard_db(id)?
            .one(
                "SELECT s.url,p.company_id FROM weekly_scorecard_publications p JOIN weekly_scorecard_sources s \
                 ON s.publication_id=p.id WHERE p.station=? AND s.source='api' \
                 AND p.scope_verified=1 \
                 ORDER BY p.collected_at DESC,s.dataset LIMIT 1",
                [station],
            )?
            .map(|row| (s(&row, "url").to_owned(), s(&row, "company_id").to_owned())))
    }
    fn publish_weekly_scorecard(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()> {
        publication::publish(self, id, job, capture, scope)
    }
    fn weekly_scorecard_weeks(&self, id: &str) -> Result<WeeklyScorecardWeeks> {
        reads::weeks(self, id)
    }
}
