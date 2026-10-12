//! Timecard keeps the meal breaks Cortex reports.
use crate::backend::TimecardStore;
use dispatch_core::{
    Result,
    collection::browser::Collected,
    db::{Db, Store},
    ensure,
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    meals::{DayCapture, DayRequest, JOB_KIND},
};
use serde_json::{Value, json};

pub struct MealBreaks;
impl Keeper for MealBreaks {
    fn queue_capacity(&self) -> i64 {
        dispatch_paycom::timecards::PERIOD_DAYS + 1
    }
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "collections.run"
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        let request: Value = serde_json::from_str(&store.job_row(job, Some(dsp))?.request)?;
        if let Some(request) = DayRequest::parse(&request)? {
            ensure(scope.is_none(), "cortex_scope_mismatch", 502)?;
            let data: DayCapture = serde_json::from_value(data)
                .map_err(|_| dispatch_core::Error::new("invalid_cortex_capture", 502))?;
            data.validate(&request)?;
            let db = store.collector(dsp, cortex::PROVIDER)?;
            return db.transaction(|| {
                for (index, capture) in data.captures.iter().enumerate() {
                    let key = format!("{job}:scope:{index}");
                    store.publish_meals(dsp, &key, capture, &capture.scope)?;
                    db.exec(
                        "INSERT OR IGNORE INTO meal_publication_jobs(publication_key,job_id,scope_index) VALUES (?,?,?)",
                        rusqlite::params![key,job,index as i64],
                    )?;
                }
                Ok(())
            });
        }
        store.publish_meals(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &Scope::collected(scope)?,
        )?;
        Ok(())
    }
    /// The routes the scope's active publication holds, as Cortex collected them, so a
    /// collection reads again only what may still change.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        let scope: Scope = serde_json::from_value(question.clone())?;
        Ok(serde_json::to_value(super::kept_routes(
            store, dsp, &scope,
        )?)?)
    }
    fn verify(&self, db: &Db) -> Result<()> {
        ensure(
            db.all("SELECT version FROM meal_schema", [])? == vec![json!({"version":1})],
            "unsupported_cortex_schema",
            503,
        )?;
        // Missing initialized feature tables fail closed, rather than recreating lost data.
        // meal_delivery_events, meal_breaks and their trigger hold nothing and are not
        // required here. Cortex's baseline still creates them because v0.0.9 refuses to
        // start without them.
        for table in ["meal_publications", "meal_itineraries"] {
            db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
        }
        db.one("SELECT count(*) FROM meal_publication_jobs WHERE 0", [])?;
        ensure(
            db.all("SELECT version FROM meal_record_schema", [])? == vec![json!({"version":1})],
            "unsupported_cortex_schema",
            503,
        )?;
        db.one("SELECT count(*) FROM meal_records WHERE 0", [])?;
        Ok(())
    }
    fn collected_at(&self, store: &Store, dsp: &str, date: &str) -> Result<Option<Value>> {
        store.collector(dsp, cortex::PROVIDER)?.one(
            "SELECT MAX(collected_at) collected_at FROM meal_publications WHERE report_date=? \
            AND active=1",
            [date],
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        ensure(
            !super::sync::meal_sync_scopes(store, dsp, &store.local_date(dsp)?)?.is_empty(),
            "schedule_scope_required",
            409,
        )
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        super::sync::meal_sync_scopes(store, dsp, &store.local_date(dsp)?)?
            .iter()
            .enumerate()
            .map(|(index, scope)| {
                scope.validate()?;
                Ok((format!("flex:{index}"), serde_json::to_value(scope)?))
            })
            .collect()
    }
}
