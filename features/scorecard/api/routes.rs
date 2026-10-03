//! Weekly scorecards: what is stored, collecting a week, and its jobs. The scorecard is a
//! feature of its own; nothing here asks for another's permission.
use crate::{
    Result, State,
    collectors::cortex::scorecard,
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, async_post, read, write},
    },
    validate as v, weeks,
};
use std::sync::Arc;

const VIEW: Dsp = Dsp("scorecard.view");
const COLLECT: Dsp = Dsp("scorecard.collect");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/scorecard/weeks", VIEW, weeks),
        read("/api/dsp/scorecard/jobs", VIEW, jobs),
        write("/api/dsp/scorecard/collect", COLLECT, collect),
        async_post("/api/dsp/scorecard/jobs/{id}/cancel", COLLECT, cancel),
    ]
}

fn weeks(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.scorecard_weeks(c.dsp_id())?)
}
fn jobs(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.recent_jobs_of(c.dsp_id(), scorecard::JOB_KIND)?)
}
async fn cancel(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    super::jobs::cancel_kind(state, input, access, Some(scorecard::JOB_KIND)).await
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
