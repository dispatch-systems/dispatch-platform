//! Timecard keeps the meal breaks Cortex reports.
use crate::{
    Result,
    browsers::Collected,
    collectors::cortex::{self, discovery::Scope, meals::JOB_KIND},
    db::Store,
    ensure,
    manifest::Keeper,
    read_cache::DataDomain,
};
use serde_json::Value;

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
    fn collected_at(&self, store: &Store, dsp: &str, date: &str) -> Result<Option<Value>> {
        store.collector(dsp, cortex::PROVIDER)?.one(
            "SELECT MAX(collected_at) collected_at FROM meal_publications WHERE report_date=? \
            AND active=1",
            [date],
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        ensure(
            !store
                .meal_sync_scopes(dsp, &store.local_date(dsp)?)?
                .is_empty(),
            "schedule_scope_required",
            409,
        )
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store
            .meal_sync_scopes(dsp, &store.local_date(dsp)?)?
            .iter()
            .enumerate()
            .map(|(index, scope)| {
                scope.validate()?;
                Ok((format!("flex:{index}"), serde_json::to_value(scope)?))
            })
            .collect()
    }
}
