//! Cortex: meal evidence, the scorecard, daily routes, and short DVIC inspections from Amazon Logistics.
//! Its storage was added to DSPs that already existed, which is the path every later
//! provider takes.
#[path = "discovery/scope.rs"]
pub mod discovery;
#[path = "collections/dvic/capture.rs"]
pub mod dvic;
#[path = "collections/meals/capture.rs"]
pub mod meals;
#[path = "collections/routes/capture.rs"]
pub mod routes;
#[path = "collections/scorecard/capture.rs"]
pub mod scorecard;

use super::AddedStorage;
use super::Collector;
use crate::{
    Error, Result,
    browsers::{
        Collected, Driver, Pending,
        browseros::{self, NetworkPolicy},
        cortex,
    },
    db::{self, Db, Kind, Store},
    ensure, validate as v,
};
use discovery::CollectionRequest;
use serde_json::{Value, json};
use std::path::Path;

pub(super) struct Cortex;
impl Collector for Cortex {
    fn id(&self) -> &'static str {
        "cortex"
    }
    fn label(&self) -> &'static str {
        "Cortex"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["meal_breaks", "routes", "dvic", "scorecard"]
    }
    fn job_kind(&self) -> &'static str {
        "cortex.meal_breaks.collect"
    }
    fn other_job_kinds(&self) -> &'static [&'static str] {
        &[scorecard::JOB_KIND, routes::JOB_KIND, dvic::JOB_KIND]
    }
    fn job_kind_for(&self, request: &Value) -> &'static str {
        if dvic::Request::is(request) {
            dvic::JOB_KIND
        } else if scorecard::Request::is(request) {
            scorecard::JOB_KIND
        } else if routes::Request::is(request) {
            routes::JOB_KIND
        } else {
            self.job_kind()
        }
    }
    fn bind_request(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        if dvic::Request::is(request) {
            let station = request["station"]
                .as_str()
                .ok_or_else(|| Error::new("invalid_dvic_request", 400))?;
            let weeks: Vec<String> = serde_json::from_value(request["weeks"].clone())
                .map_err(|_| Error::new("invalid_dvic_request", 400))?;
            let bound = store.bind_dvic_request(dsp, weeks)?;
            ensure(bound["station"] == station, "dvic_scope_mismatch", 409)?;
            return Ok(bound);
        }
        if scorecard::Request::is(request) {
            let week = request["week"]
                .as_str()
                .ok_or_else(|| Error::new("invalid_input", 400))?;
            let station = request["station"]
                .as_str()
                .ok_or_else(|| Error::new("invalid_input", 400))?;
            let bound = store.bind_scorecard_request(dsp, week)?;
            ensure(bound["station"] == station, "scorecard_scope_mismatch", 409)?;
            return Ok(bound);
        }
        Ok(request.clone())
    }
    fn added_storages(&self) -> &'static [&'static AddedStorage] {
        static ADDED: [&AddedStorage; 3] = [
            &crate::scorecard::STORAGE,
            &crate::routedata::STORAGE,
            &crate::dvic::STORAGE,
        ];
        &ADDED
    }
    fn database(&self) -> Kind {
        Kind::Cortex
    }
    fn seed(&self, dsp: &str) -> String {
        format!(
            "INSERT INTO storage_identity VALUES ('{dsp}','cortex','cortex-v1');\nINSERT \
                INTO connections(provider,updated_at) VALUES ('cortex','{}');",
            db::iso()
        )
    }
    fn marker(&self) -> Option<&'static str> {
        Some("storage.cortex")
    }
    fn verify(&self, db: &Db) -> Result<()> {
        ensure(
            db.all("SELECT version FROM meal_schema", [])? == vec![json!({"version":1})],
            "unsupported_cortex_schema",
            503,
        )?;
        // Missing initialized feature tables fail closed, rather than recreating lost data.
        // meal_delivery_events, meal_breaks and their trigger hold nothing and are not
        // required here. The baseline still creates them because v0.0.9 refuses to start
        // without them.
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
    fn browser_entries(&self) -> &'static [&'static str] {
        &[
            "cortex-browseros",
            "cortex-attempt.json",
            ".cortex-browseros.browseros.lock",
        ]
    }
    fn network(&self) -> NetworkPolicy {
        NetworkPolicy::Cortex
    }
    fn validate_credentials(&self, value: &Value) -> Result<()> {
        v::fields(value, &["username", "password"])?;
        v::name(value, "username", 200)?;
        v::text(value, "password", 1, 256)?;
        Ok(())
    }
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>> {
        Box::pin(async move {
            Ok(Box::new(cortex::Driver::new(browser, profile, fixture).await?) as Box<dyn Driver>)
        })
    }
    fn fixture(&self, _: &str, request: &Value) -> Result<Collected> {
        if let Some(request) = dvic::Request::parse(request)? {
            let scope = match request.scope_request() {
                CollectionRequest::Discover(discovery) => {
                    discovery.scope("area-fixture", "company-fixture")?
                }
                CollectionRequest::Scoped(scope) => scope,
            };
            return Ok(Collected {
                data: serde_json::to_value(dvic::fixture(&request)?)?,
                scope: Some(scope),
            });
        }
        if let Some(request) = scorecard::Request::parse(request)? {
            let scope = match request.scope_request() {
                CollectionRequest::Discover(discovery) => {
                    discovery.scope("area-fixture", "company-fixture")?
                }
                CollectionRequest::Scoped(scope) => scope,
            };
            return Ok(Collected {
                data: serde_json::to_value(scorecard::fixture(&request)?)?,
                scope: Some(scope),
            });
        }
        if let Some(request) = routes::Request::parse(request)? {
            let capture = routes::fixture(&request)?;
            let scope = capture.scope.clone();
            return Ok(Collected {
                data: serde_json::to_value(capture)?,
                scope: Some(scope),
            });
        }
        let scope = match serde_json::from_value(request.clone())? {
            CollectionRequest::Scoped(scope) => scope,
            CollectionRequest::Discover(discovery) => {
                discovery.scope("area-demo", "provider-demo")?
            }
        };
        Ok(Collected {
            data: serde_json::to_value(meals::fixture(&scope))?,
            scope: Some(scope),
        })
    }
    fn progress(&self, request: &Value) -> &'static str {
        if dvic::Request::is(request) {
            "Collecting DVIC"
        } else if scorecard::Request::is(request) {
            "Collecting scorecard"
        } else if routes::Request::is(request) {
            "Collecting routes"
        } else {
            "Collecting meal breaks"
        }
    }
    fn stage<'a>(
        &'a self,
        state: &'a std::sync::Arc<crate::State>,
        dsp: &'a str,
        job: &'a str,
        owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move {
            if !routes::Request::is(&collected.data) {
                return Ok(collected);
            }
            let capture: routes::Capture = serde_json::from_value(collected.data)?;
            let staged = crate::routedata::stage(state, dsp, job, owner, capture).await?;
            Ok(Collected {
                data: serde_json::to_value(staged)?,
                scope: collected.scope,
            })
        })
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        if dvic::Request::is(&data) {
            return store.publish_dvic(
                dsp,
                job,
                &serde_json::from_value(data)?,
                &scope.ok_or_else(|| Error::new("invalid_cortex_scope", 502))?,
            );
        }
        if scorecard::Request::is(&data) {
            return store.publish_scorecard(
                dsp,
                job,
                &serde_json::from_value(data)?,
                &scope.ok_or_else(|| Error::new("invalid_cortex_scope", 502))?,
            );
        }
        if routes::Request::is(&data) {
            return store.publish_routes(dsp, job, &serde_json::from_value(data)?);
        }
        store.publish_meals(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &scope.ok_or_else(|| Error::new("invalid_cortex_scope", 502))?,
        )?;
        Ok(())
    }
    fn collected_at(&self, db: &Db, date: &str) -> Result<Option<Value>> {
        db.one("SELECT MAX(collected_at) collected_at FROM meal_publications WHERE report_date=? AND active=1",[date])
    }
    fn schedules(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("meal_break", "schedule_meals_required"),
            ("scorecard", "schedule_scorecard_required"),
            ("routes", "schedule_routes_required"),
            ("dvic", "schedule_dvic_required"),
        ]
    }
    fn schedule_ready(&self, store: &Store, dsp: &str, collection: &str) -> Result<()> {
        if collection == "dvic" {
            return store.dvic_schedule_ready(dsp);
        }
        if collection == "scorecard" {
            return store.scorecard_schedule_ready(dsp);
        }
        if collection == "routes" {
            return store.routes_schedule_ready(dsp);
        }
        ensure(
            !store
                .meal_sync_scopes(dsp, &store.local_date(dsp)?)?
                .is_empty(),
            "schedule_scope_required",
            409,
        )
    }
    fn scheduled(
        &self,
        store: &Store,
        dsp: &str,
        collection: &str,
    ) -> Result<Vec<(String, Value)>> {
        if collection == "dvic" {
            return store.dvic_jobs(dsp);
        }
        if collection == "scorecard" {
            return store.scorecard_jobs(dsp);
        }
        if collection == "routes" {
            return store.routes_jobs(dsp);
        }
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
