//! DVIC collection controls and read APIs; no dashboard is required.
use crate::backend::DvicStore;
use dispatch_core::{
    Result,
    collection::api::routes::jobs::job_cancel,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text, query_number},
        route::{Dsp, Member, Route, coalesced_read, read, write},
    },
};
use dispatch_cortex::dvic;

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/dvic/status", Dsp("dvic.view"), status),
        read("/api/dsp/dvic/inspections", Dsp("dvic.view"), inspections),
        coalesced_read("/api/dsp/dvic/summary", Dsp("dvic.view"), summary),
        coalesced_read("/api/dsp/dvic/day", Dsp("dvic.view"), day),
        coalesced_read("/api/dsp/dvic/inspection", Dsp("dvic.view"), detail),
        // Queuing a collection writes only its job and its log entry, which no cached read
        // uses: the caches stay as they are.
        write("/api/dsp/dvic/collect", Dsp("dvic.collect"), collect).bookkeeping(),
        job_cancel(
            "/api/dsp/dvic/jobs/{id}/cancel",
            Dsp("dvic.collect"),
            &[dvic::JOB_KIND],
        ),
    ]
}
fn dashboard<T: serde::Serialize>(
    db: &Store,
    c: &Member,
    input: &Input,
    load: impl FnOnce() -> Result<T>,
) -> Result<Reply> {
    // Operator hides use another process, so they cannot notify this State's cache.
    // Read their identity set afresh; notes have no effect on a dashboard answer.
    let hidden: Vec<(String,)> = db.dvic_db(c.dsp_id())?.query_as(
        "SELECT transporter_id FROM dvic_hidden_drivers ORDER BY transporter_id",
        [],
    )?;
    let hidden = dispatch_core::foundation::crypto::sha(serde_json::to_vec(&hidden)?);
    let bytes = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::backend::CACHED, c.dsp_id()),
        serde_json::to_string(&(&input.path, &input.query, hidden))?,
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        load,
    )?;
    Ok(Reply::encoded(bytes))
}
fn summary(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(q, &["from", "to", "q", "vehicles", "offset"])?;
    let from = v::text(q, "from", 10, 10)?;
    let to = v::text(q, "to", 10, 10)?;
    let query = optional_text(q, "q", 200)?;
    let vehicles = optional_text(q, "vehicles", 7)?;
    let offset = query_number(q, "offset", 0, 0, 1_000_000)?;
    dashboard(db, c, input, || {
        crate::backend::dashboard::summary(db, c.dsp_id(), from, to, query, vehicles, offset)
    })
}
fn day(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(q, &["date", "q", "vehicles", "offset"])?;
    let date = v::text(q, "date", 10, 10)?;
    let query = optional_text(q, "q", 200)?;
    let vehicles = optional_text(q, "vehicles", 7)?;
    let offset = query_number(q, "offset", 0, 0, 1_000_000)?;
    dashboard(db, c, input, || {
        crate::backend::dashboard::day(db, c.dsp_id(), date, query, vehicles, offset)
    })
}
fn detail(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(q, &["from", "to", "id"])?;
    let from = v::text(q, "from", 10, 10)?;
    let to = v::text(q, "to", 10, 10)?;
    let id = v::text(q, "id", 1, 256)?;
    dashboard(db, c, input, || {
        crate::backend::dashboard::detail(db, c.dsp_id(), from, to, id)
    })
}
fn status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.dvic_status(c.dsp_id())?)
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId", "week", "weeks", "viewing"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let week = if b.get("week").is_some() {
        Some(v::text(b, "week", 8, 8)?)
    } else {
        None
    };
    // A day of the week the page shows, whose publication weeks the sync checks too.
    let viewing = if b.get("viewing").is_some() {
        Some(v::text(b, "viewing", 10, 10)?)
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
    let job = db.enqueue_dvic(c.dsp_id(), Some(c.actor()), key, week, weeks, viewing)?;
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
            .map_err(|_| dispatch_core::Error::new("invalid_input", 400))?
    } else {
        100
    };
    Reply::of(&db.dvic_inspections(c.dsp_id(), from, to, driver, after, limit)?)
}
