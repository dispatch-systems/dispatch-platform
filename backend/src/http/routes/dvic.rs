//! DVIC collection controls and read APIs; no dashboard is required.
use crate::{
    Result, State,
    db::Store,
    dvic,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, async_post, read, write},
    },
    validate as v,
};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/dvic/status", Dsp("dvic.view"), status),
        read("/api/dsp/dvic/inspections", Dsp("dvic.view"), inspections),
        write("/api/dsp/dvic/collect", Dsp("dvic.collect"), collect),
        async_post(
            "/api/dsp/dvic/jobs/{id}/cancel",
            Dsp("dvic.collect"),
            cancel,
        ),
    ]
}
fn status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.dvic_status(c.dsp_id())?)
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId", "week", "weeks"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let week = if b.get("week").is_some() {
        Some(v::text(b, "week", 8, 8)?)
    } else {
        None
    };
    let weeks = if b.get("weeks").is_some() {
        v::integer(b, "weeks", 1, dvic::MAX_WEEKS as i64)? as usize
    } else if week.is_some() {
        1
    } else {
        2
    };
    let job = db.enqueue_dvic(c.dsp_id(), Some(c.actor()), key, week, weeks)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "dvic.collection_requested",
        week.unwrap_or("recent"),
    )?;
    Ok(Reply::status(job, 202))
}
fn inspections(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(q, &["from", "to", "driver", "after", "limit"])?;
    let from = v::text(q, "from", 10, 10)?;
    let to = v::text(q, "to", 10, 10)?;
    let driver = if q.get("driver").is_some() {
        Some(v::text(q, "driver", 1, 128)?)
    } else {
        None
    };
    let after = if q.get("after").is_some() {
        v::text(q, "after", 1, 256)?
    } else {
        ""
    };
    let limit = if q.get("limit").is_some() {
        v::text(q, "limit", 1, 3)?
            .parse()
            .map_err(|_| crate::Error::new("invalid_input", 400))?
    } else {
        100
    };
    Reply::of(&db.dvic_inspections(c.dsp_id(), from, to, driver, after, limit)?)
}
async fn cancel(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    super::jobs::cancel_kind(state, input, access, Some(dvic::JOB_KIND)).await
}
