//! Host-owned, unpublished Paycom progress. Workers never receive storage paths.
use crate::{PROVIDER, timecards::validate_workforce};
use dispatch_core::{
    Result, State,
    collection::live,
    db::{self, Db, Store, n, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const TTL_MS: i64 = 15 * 60 * 1000;
/// What Paycom keeps of a collection before a feature publishes it.
pub trait PaycomStore {
    fn clear_paycom_checkpoints(&self, dsp: &str, job: Option<&str>) -> Result<()>;
    fn prune_paycom_checkpoints(&self, dsp: &str) -> Result<()>;
    fn stage_paycom(
        &self,
        job: &str,
        owner: &str,
        employee: &Value,
        records: &[Value],
    ) -> Result<()>;
}
impl PaycomStore for Store {
    fn clear_paycom_checkpoints(&self, dsp: &str, job: Option<&str>) -> Result<()> {
        let db = self.collector(dsp, PROVIDER)?;
        db.exec(
            "DELETE FROM collection_checkpoints WHERE (?1 IS NULL OR job_id=?1)",
            [job],
        )?;
        Ok(())
    }
    fn prune_paycom_checkpoints(&self, dsp: &str) -> Result<()> {
        let db = self.collector(dsp, PROVIDER)?;
        db.transaction(|| {
            for row in db.all("SELECT job_id,created_at FROM collection_checkpoints", [])? {
                let live = self.job_row(s(&row, "job_id"), None).ok();
                let keep = n(&row, "created_at") <= db::now()
                    && n(&row, "created_at") >= db::now() - TTL_MS
                    && live.is_some_and(|job| {
                        job.dsp_id == dsp && job.provider() == PROVIDER && job.status.is_active()
                    });
                if !keep {
                    db.exec(
                        "DELETE FROM collection_checkpoints WHERE job_id=?",
                        [s(&row, "job_id")],
                    )?;
                }
            }
            Ok(())
        })
    }
    fn stage_paycom(
        &self,
        job: &str,
        owner: &str,
        employee: &Value,
        records: &[Value],
    ) -> Result<()> {
        let dsp = self.guard(job, owner)?;
        let db = self.collector(&dsp.id, PROVIDER)?;
        db.transaction(|| stage_paycom_page(&db, job, owner, employee, records))
    }
}

/// Caller has validated the whole employee page and holds its write transaction.
pub fn stage_paycom_page(
    db: &Db,
    job: &str,
    owner: &str,
    employee: &Value,
    records: &[Value],
) -> Result<()> {
    for record in records {
        let mut data = record.clone();
        for key in ["name", "department", "station"] {
            data[key] = employee[key].clone();
        }
        live::stage_item(
            db,
            job,
            owner,
            s(employee, "code"),
            s(record, "date"),
            &data.to_string(),
        )?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct Checkpoint {
    state: Arc<State>,
    job: String,
    owner: String,
}
pub struct Resume {
    pub token: String,
    pub pages: BTreeMap<String, Vec<Value>>,
}
impl Checkpoint {
    pub fn new(state: Arc<State>, job: &str, owner: &str) -> Self {
        Self {
            state,
            job: job.into(),
            owner: owner.into(),
        }
    }
    pub async fn prepare(
        &self,
        period: &Value,
        employees: &[Value],
        timezone: &str,
    ) -> Result<Resume> {
        let mut roster = employees.to_vec();
        roster.sort_by(|a, b| s(a, "code").cmp(s(b, "code")));
        let fingerprint = crypto::hex(&Sha256::digest(serde_json::to_vec(
            &json!({"version":1,"timezone":timezone,"period":period,"employees":roster}),
        )?));
        let period = period.clone();
        let mut live_metadata = json!({"from":period["start"],"to":period["end"],"roster":roster});
        let job = self.job.clone();
        let owner = self.owner.clone();
        let state = self.state.clone();
        let (dsp, resume) = self.state.run_bookkeeping(move |db| {
            let dsp=db.guard(&job,&owner)?;
            state.read_cache.invalidate_tenant(&dsp.id, dispatch_core::server::cache::DataDomain::LIVE);
            let row=db.job_row(&job,None)?;
            ensure(row.kind.as_str()==PROVIDER.job_kind(),"unsupported_collector",409)?;
            let tenant=dsp.id.as_str();
            db.prune_paycom_checkpoints(tenant)?;
            let storage=db.collector(tenant,PROVIDER)?;
            let resume = storage.transaction(|| {
                let existing=storage.one("SELECT * FROM collection_checkpoints WHERE job_id=?",[&job])?;
                if let Some(existing)=existing.filter(|r| s(r,
                    "fingerprint")==fingerprint && n(r,"connection_revision")==row.connection_revision) {
                    let mut pages=BTreeMap::new();
                    let mut valid=true;
                    for page in storage.all("SELECT employee_code,data FROM collection_checkpoint_pages WHERE job_id=?",[&job])? {
                        let code=s(&page,"employee_code");
                        let parsed=serde_json::from_str::<Vec<Value>>(s(&page,"data"));
                        match (roster.iter().find(|e|s(e,"code")==code),parsed) {
                            (Some(employee),Ok(records)) if validate_page(employee,&period,
                                &records).is_ok()=>{pages.insert(code.into(),records);},
                            _=>{valid=false;break;}
                        }
                    }
                    if valid { return Ok(Resume { token:s(&existing,"token").into(),pages }); }
                }
                storage.exec("DELETE FROM collection_checkpoints WHERE job_id=?",[&job])?;
                let token=crypto::id("checkpoint")?;
                storage.exec("INSERT INTO \
                    collection_checkpoints(job_id,connection_revision,fingerprint,created_at,token) VALUES (?,?,?,?,?)",
                params![job,row.connection_revision,fingerprint,db::now(),
                token])?;
                Ok(Resume { token,pages:BTreeMap::new() })
            })?;
            live_metadata["checkpointToken"]=json!(resume.token);
            drop(storage);
            db.start_live(&job,&owner,&live_metadata)?;
            for employee in &roster {
                if let Some(records)=resume.pages.get(s(employee,"code")) {
                    db.stage_paycom(&job,&owner,employee,records)?;
                }
            }
            Ok((tenant.to_owned(), resume))
        }).await?;
        self.state.updates.changed(
            &dsp,
            dispatch_core::collection::api::types::CollectionChange::provider(PROVIDER.id()),
        );
        Ok(resume)
    }
    pub async fn save(
        &self,
        token: &str,
        employee: &Value,
        period: &Value,
        records: &[Value],
    ) -> Result<()> {
        validate_page(employee, period, records)?;
        let change = dispatch_core::collection::api::types::CollectionChange {
            provider: PROVIDER.id().into(),
            dates: records.iter().map(|r| s(r, "date").to_owned()).collect(),
            employee_code: Some(s(employee, "code").to_owned()),
            roster: false,
        };
        let token = token.to_owned();
        let code = s(employee, "code").to_owned();
        let data = serde_json::to_string(records)?;
        ensure(data.len() <= 256 * 1024, "checkpoint_too_large", 502)?;
        let employee = employee.clone();
        let records = records.to_vec();
        let job = self.job.clone();
        let owner = self.owner.clone();
        let state = self.state.clone();
        let dsp = self
            .state
            .run_bookkeeping(move |db| {
                let dsp = db.guard(&job, &owner)?;
                state
                    .read_cache
                    .invalidate_tenant(&dsp.id, dispatch_core::server::cache::DataDomain::LIVE);
                let row = db.job_row(&job, None)?;
                let storage = db.collector(&dsp.id, PROVIDER)?;
                // Save resume data and visible results with one transaction per driver.
                storage.transaction(|| {
                    // An expired checkpoint simply stops accepting new progress. The
                    // current in-memory collection may still finish and publish normally.
                    storage.exec(
                        "INSERT OR REPLACE INTO \
                collection_checkpoint_pages(job_id,employee_code,data) SELECT job_id,?1,?2 \
                FROM collection_checkpoints WHERE job_id=?3 AND token=?4 AND \
                connection_revision=?5 AND created_at>=?6 AND \
                created_at<=?7",
                        params![
                            code,
                            data,
                            job,
                            token,
                            row.connection_revision,
                            db::now() - TTL_MS,
                            db::now()
                        ],
                    )?;
                    // Expiry limits resume reuse, not validated live visibility. An old
                    // checkpoint token must never write into a replacement run.
                    if live::run_marked(&storage, &job, "checkpointToken", &token)? {
                        stage_paycom_page(&storage, &job, &owner, &employee, &records)?;
                    }
                    Ok(())
                })?;
                Ok(dsp.id)
            })
            .await?;
        self.state.updates.changed(&dsp, change);
        Ok(())
    }
}
fn validate_page(employee: &Value, period: &Value, records: &[Value]) -> Result<()> {
    ensure(records.len() == 14, "invalid_checkpoint", 502)?;
    let start = chrono::NaiveDate::parse_from_str(s(period, "start"), "%Y-%m-%d")
        .map_err(|_| dispatch_core::Error::new("invalid_checkpoint", 502))?;
    for (index, record) in records.iter().enumerate() {
        ensure(
            record["employeeCode"] == employee["code"]
                && s(record, "date")
                    == start
                        .checked_add_signed(chrono::Duration::days(index as i64))
                        .ok_or_else(|| dispatch_core::Error::new("invalid_checkpoint", 502))?
                        .format("%Y-%m-%d")
                        .to_string(),
            "invalid_checkpoint",
            502,
        )?;
    }
    validate_workforce(
        &json!({"employees":[employee],"timecards":records,"from":period["start"],"to":period["end"],"collectedAt":db::iso()}),
    )
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/checkpoint.rs"]
mod tests;
