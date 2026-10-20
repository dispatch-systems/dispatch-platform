//! Vans' storage: the fleet in a database of its own beside Cortex's. Only this crate writes
//! SQL against it.
use crate::api::types::FleetStatus;
use dispatch_core::{
    Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, FromRow, Kind, Row, Store, at},
    ensure,
    foundation::validate,
};
use dispatch_cortex::{
    self as cortex,
    vans::{Capture, Collection, JOB_KIND, Request},
};
use dispatch_vans_contract::Van;
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Its database beside Cortex's: `vans/vans.sqlite` in each DSP's data.
pub(crate) const DATABASE: Kind = Kind::new("vans", 1);
pub(crate) static STORAGE: AddedStorage = AddedStorage {
    id: "vans",
    kind: DATABASE,
    marker: "storage.vans",
    source: "vans-v1",
    verify,
};
/// Fails closed when the initialized storage lost a table.
fn verify(db: &Db) -> Result<()> {
    for table in ["vans", "fleet_runs", "settings"] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}
/// The most VINs one question names.
const MAX_ASKED: usize = 100;

/// What a fleet job carries while queued; the DSP's code is bound just before it runs.
pub(crate) fn fleet_request() -> Value {
    json!({"collection": "vans"})
}
/// The request a fleet job runs with: the DSP's code, as its profile has it now.
pub(crate) fn bind_fleet_request(store: &Store, dsp: &str) -> Result<Value> {
    let abbreviation = store.profile(dsp)?.abbreviation;
    ensure(!abbreviation.is_empty(), "vans_abbreviation_required", 409)?;
    let request = Request {
        collection: Collection::Vans,
        dsp_abbreviation: abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}

/// What Vans reads and writes for one DSP.
pub trait VansStore {
    fn vans_db(&self, dsp: &str) -> Result<DspLease<'_>>;
    /// Queues a collection of the fleet, answering with the job.
    fn enqueue_fleet(&self, dsp: &str, actor: Option<&str>, key: &str) -> Result<Value>;
    /// Stores a fleet as the current one.
    fn publish_fleet(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()>;
    /// When the fleet was collected, what it holds, and when it is collected next.
    fn fleet_status(&self, dsp: &str) -> Result<FleetStatus>;
    /// What the fleet says of each of `vins` it has listed, in their order. Refuses an empty
    /// VIN, one over 128 characters, or more than a hundred.
    fn vans(&self, dsp: &str, vins: &[String]) -> Result<Vec<Van>>;
}
impl VansStore for Store {
    fn vans_db(&self, dsp: &str) -> Result<DspLease<'_>> {
        self.added_storage(dsp, cortex::PROVIDER, &STORAGE)
    }
    fn enqueue_fleet(&self, dsp: &str, actor: Option<&str>, key: &str) -> Result<Value> {
        bind_fleet_request(self, dsp)?;
        let job = self
            .enqueue_batch(dsp, actor, &[(key.into(), cortex::PROVIDER, fleet_request())])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    fn publish_fleet(&self, dsp: &str, job: &str, capture: &Capture) -> Result<()> {
        let request = Request::parse(&bind_fleet_request(self, dsp)?)?
            .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?;
        capture.validate(&request)?;
        let collected_at = at(capture.finished_at);
        let db = self.vans_db(dsp)?;
        db.transaction(|| {
            for van in &capture.vans {
                db.exec(
                    "INSERT INTO vans(vin,company_id,name,plate,status,service_tier,station,listed,\
                     first_listed,last_listed,row) VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?8,?9) \
                     ON CONFLICT(vin) DO UPDATE SET company_id=excluded.company_id,\
                     name=excluded.name,plate=excluded.plate,status=excluded.status,\
                     service_tier=excluded.service_tier,station=excluded.station,listed=1,\
                     last_listed=excluded.last_listed,row=excluded.row",
                    params![
                        van.vin,
                        capture.company_id,
                        van.name,
                        van.plate,
                        van.status,
                        van.service_tier,
                        van.station,
                        collected_at,
                        van.row.to_string(),
                    ],
                )?;
            }
            let listed: Vec<&str> = capture.vans.iter().map(|van| van.vin.as_str()).collect();
            db.exec(
                "UPDATE vans SET listed=0 WHERE listed=1 AND vin NOT IN \
                 (SELECT value FROM json_each(?))",
                [serde_json::to_string(&listed)?],
            )?;
            db.exec(
                "INSERT INTO fleet_runs(job_id,company_id,collected_at,listed) VALUES (?,?,?,?)",
                params![
                    job,
                    capture.company_id,
                    collected_at,
                    capture.vans.len() as i64
                ],
            )?;
            Ok(())
        })
    }
    fn fleet_status(&self, dsp: &str) -> Result<FleetStatus> {
        let db = self.vans_db(dsp)?;
        let collected_at = db
            .query_as::<(String,)>(
                "SELECT collected_at FROM fleet_runs ORDER BY collected_at DESC LIMIT 1",
                [],
            )?
            .into_iter()
            .next()
            .map(|(at,)| at);
        let listed = db.count("SELECT count(*) FROM vans WHERE listed=1", [])? as usize;
        drop(db);
        let next_run = self
            .collection_schedules(dsp)?
            .schedules
            .into_iter()
            .filter(|s| s.enabled && s.collection.as_str() == "vans")
            .filter_map(|s| s.next_run)
            .min();
        Ok(FleetStatus {
            collected_at,
            listed,
            next_run,
            jobs: self.recent_jobs_of(dsp, JOB_KIND)?,
        })
    }
    fn vans(&self, dsp: &str, vins: &[String]) -> Result<Vec<Van>> {
        validate::ids(vins, MAX_ASKED)?;
        let asked: Vec<String> = vins.iter().map(|vin| vin.trim().to_ascii_uppercase()).collect();
        let mut kept = self
            .vans_db(dsp)?
            .query_as::<Kept>(
                "SELECT vin,name,status,listed,last_listed FROM vans \
                 WHERE vin IN (SELECT value FROM json_each(?))",
                [serde_json::to_string(&asked)?],
            )?
            .into_iter()
            .map(|Kept(van)| (van.vin.clone(), van))
            .collect::<BTreeMap<_, _>>();
        Ok(asked.iter().filter_map(|vin| kept.remove(vin)).collect())
    }
}
/// A van as its row has it.
struct Kept(Van);
impl FromRow for Kept {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self(Van {
            vin: row.get("vin")?,
            name: row.get("name")?,
            status: row.get("status")?,
            listed: row.get("listed")?,
            last_listed: row.get("last_listed")?,
        }))
    }
}

#[cfg(test)]
#[path = "../tests/backend/storage.rs"]
mod tests;
