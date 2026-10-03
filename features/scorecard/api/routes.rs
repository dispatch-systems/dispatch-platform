//! Weekly scorecards: what is stored, collecting a week, and its jobs. The scorecard is a
//! feature of its own; nothing here asks for another's permission.
use super::{
    jobs::{job_cancel, job_list},
    schedules::schedule_routes,
};
use crate::{
    Result,
    collectors::cortex::scorecard,
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
    scorecard::ScorecardStore,
    validate as v, weeks,
};

const VIEW: Dsp = Dsp("scorecard.view");
const COLLECT: Dsp = Dsp("scorecard.collect");
const MANAGE: Dsp = Dsp("scorecard.manage");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/scorecard/weeks", VIEW, weeks),
        job_list("/api/dsp/scorecard/jobs", VIEW, &[scorecard::JOB_KIND]),
        write("/api/dsp/scorecard/collect", COLLECT, collect),
        job_cancel(
            "/api/dsp/scorecard/jobs/{id}/cancel",
            COLLECT,
            &[scorecard::JOB_KIND],
        ),
    ];
    routes.extend(schedule_routes(
        "/api/dsp/scorecard/schedules",
        MANAGE,
        "scorecard",
    ));
    routes
}

fn weeks(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.scorecard_weeks(c.dsp_id())?)
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
    let job = db.enqueue_scorecard(id, actor, key, week)?;
    db.audit(
        actor,
        Some(id),
        "scorecard.collection_requested",
        week.unwrap_or(""),
    )?;
    Ok(Reply::status(job, 202))
}
