//! Weekly Scorecards: what is stored, collecting a week, and its jobs. The scorecard is a
//! feature of its own; nothing here asks for another's permission.
use crate::{WeeklyScorecardPolicy, backend::WeeklyScorecardStore};
use dispatch_core::{
    Result,
    collection::api::routes::{
        jobs::{job_cancel, job_list},
        schedules::schedule_routes,
    },
    db::Store,
    foundation::{validate as v, weeks},
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
};
use dispatch_cortex::weekly_scorecard;

const VIEW: Dsp = Dsp("weekly_scorecard.view");
const COLLECT: Dsp = Dsp("weekly_scorecard.collect");
const MANAGE: Dsp = Dsp("weekly_scorecard.manage");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/weekly-scorecard/policy", MANAGE, policy),
        write("/api/dsp/weekly-scorecard/policy", MANAGE, update_policy),
        read("/api/dsp/weekly-scorecard/weeks", VIEW, weeks),
        job_list(
            "/api/dsp/weekly-scorecard/jobs",
            VIEW,
            &[weekly_scorecard::JOB_KIND],
        ),
        write("/api/dsp/weekly-scorecard/collect", COLLECT, collect),
        job_cancel(
            "/api/dsp/weekly-scorecard/jobs/{id}/cancel",
            COLLECT,
            &[weekly_scorecard::JOB_KIND],
        ),
    ];
    routes.extend(schedule_routes(
        "/api/dsp/weekly-scorecard/schedules",
        MANAGE,
        "weekly_scorecard",
    ));
    routes
}

fn weeks(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.weekly_scorecard_weeks(c.dsp_id())?)
}

/// Queues one week, the most recent completed one unless `week` names another.
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId", "week"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let week = if b.get("week").is_some() {
        let week = v::text(b, "week", 8, 8)?;
        weeks::parse_week(week)?;
        Some(week)
    } else {
        None
    };
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let job = db.enqueue_weekly_scorecard(id, actor, key, week)?;
    db.audit(
        actor,
        Some(id),
        "weekly_scorecard.collection_requested",
        week.unwrap_or(""),
    )?;
    Ok(Reply::status(job, 202))
}

fn policy(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.weekly_scorecard_policy(c.dsp_id())?)
}
fn update_policy(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let policy: WeeklyScorecardPolicy = serde_json::from_value(input.body.clone())
        .map_err(|_| dispatch_core::Error::new("invalid_input", 400))?;
    db.set_weekly_scorecard_policy(c.dsp_id(), &policy)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "weekly_scorecard.policy_updated",
        "",
    )?;
    Reply::of(&policy)
}
