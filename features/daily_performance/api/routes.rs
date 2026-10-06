//! Independent daily collection, jobs, schedules and freshness policy.
use crate::{DailyPerformancePolicy, DailyPerformanceStore};
use dispatch_core::{
    Result,
    collection::api::routes::{
        jobs::{job_cancel, job_list},
        schedules::schedule_routes,
    },
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
};
use dispatch_cortex::daily_performance::JOB_KIND;
const VIEW: Dsp = Dsp("daily_performance.view");
const COLLECT: Dsp = Dsp("daily_performance.collect");
const MANAGE: Dsp = Dsp("daily_performance.manage");
pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/daily-performance", VIEW, summary),
        write("/api/dsp/daily-performance/collect", COLLECT, collect),
        job_list("/api/dsp/daily-performance/jobs", VIEW, &[JOB_KIND]),
        job_cancel(
            "/api/dsp/daily-performance/jobs/{id}/cancel",
            COLLECT,
            &[JOB_KIND],
        ),
        read("/api/dsp/daily-performance/policy", MANAGE, policy),
        write("/api/dsp/daily-performance/policy", MANAGE, update_policy),
    ];
    routes.extend(schedule_routes(
        "/api/dsp/daily-performance/schedules",
        MANAGE,
        "daily_performance",
    ));
    routes
}
fn summary(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.daily_performance_days(c.dsp_id())?)
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let body = &input.body;
    v::fields(body, &["requestId", "date", "from", "to"])?;
    let key = v::text(body, "requestId", 1, 100)?;
    let default_date = (super::super::backend::storage::today(db, c.dsp_id())?
        - chrono::Duration::days(1))
    .to_string();
    let (from, to) = if body.get("from").is_some() || body.get("to").is_some() {
        dispatch_core::ensure(body.get("date").is_none(), "invalid_input", 400)?;
        (v::text(body, "from", 10, 10)?, v::text(body, "to", 10, 10)?)
    } else {
        let date = if body.get("date").is_some() {
            v::text(body, "date", 10, 10)?
        } else {
            &default_date
        };
        (date, date)
    };
    let jobs = db.enqueue_daily_performance(c.dsp_id(), Some(c.actor()), key, from, to)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "daily_performance.collection_requested",
        from,
    )?;
    Ok(Reply::status(jobs, 202))
}
fn policy(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.daily_performance_policy(c.dsp_id())?)
}
fn update_policy(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let policy: DailyPerformancePolicy = serde_json::from_value(input.body.clone())
        .map_err(|_| dispatch_core::Error::new("invalid_input", 400))?;
    db.set_daily_performance_policy(c.dsp_id(), &policy)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "daily_performance.policy_updated",
        "",
    )?;
    Reply::of(&policy)
}
