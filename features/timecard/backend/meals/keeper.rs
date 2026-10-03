//! Timecard keeps the meal breaks Cortex reports.
use crate::{
    Result,
    browsers::Collected,
    collectors::cortex::{self, discovery::Scope, meals::JOB_KIND},
    db::{Db, Store},
    ensure,
    manifest::Keeper,
    read_cache::DataDomain,
    workforce::TimecardStore,
};
use serde_json::{Value, json};

pub struct MealBreaks;
impl Keeper for MealBreaks {
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
        store.publish_meals(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &Scope::collected(scope)?,
        )?;
        Ok(())
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
