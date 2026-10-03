//! Daily routes: what is stored, collecting a day, and its jobs. The collection is `routes`
//! here and in its paths; its module and database are `routedata`.
use crate::routedata::{self, MAX_DAYS_PER_REQUEST, RoutesStore};
use dispatch_core::{
    Error, Result,
    collection::api::routes::{
        jobs::{job_cancel, job_list},
        schedules::schedule_routes,
    },
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, PlatformOwner, Route, User, read, write},
    },
};
use dispatch_cortex::routes::{JOB_KIND, Mode, token};

const VIEW: Dsp = Dsp("routes.view");
const COLLECT: Dsp = Dsp("routes.collect");
const MANAGE: Dsp = Dsp("routes.manage");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/routes/days", VIEW, days),
        read("/api/dsp/routes/days/{day}", VIEW, day),
        read(
            "/api/dsp/routes/days/{day}/itineraries/{id}",
            VIEW,
            itinerary,
        ),
        read("/api/dsp/routes/packages/{tracking}", VIEW, package),
        write("/api/dsp/routes/collect", COLLECT, collect),
        job_list("/api/dsp/routes/jobs", VIEW, &[JOB_KIND]),
        job_cancel("/api/dsp/routes/jobs/{id}/cancel", COLLECT, &[JOB_KIND]),
        read("/api/dsp/routes/retention", MANAGE, retention),
        write("/api/dsp/routes/retention", MANAGE, set_retention),
        write(
            "/api/platform/dsps/{id}/routes/reprocess",
            PlatformOwner,
            reprocess,
        ),
    ];
    routes.extend(schedule_routes(
        "/api/dsp/routes/schedules",
        MANAGE,
        "routes",
    ));
    routes
}

/// Rebuilds a DSP's route rows from the responses it stored, for every day or one,
/// after a release that reads fields the earlier one did not.
fn reprocess(db: &Store, _: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["day"])?;
    let day = if input.body.get("day").is_some() {
        let day = v::text(&input.body, "day", 10, 10)?;
        v::date(day)?;
        Some(day)
    } else {
        None
    };
    Reply::of(&db.reprocess_routes(input.param("id"), day)?)
}

fn itinerary(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let day = input.param("day");
    v::date(day)?;
    let id = input.param("id");
    ensure(token(id, 128), "invalid_input", 400)?;
    let view = db
        .route_itinerary(c.dsp_id(), day, id)?
        .ok_or_else(|| Error::new("not_found", 404))?;
    Reply::of(&view)
}

fn package(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let tracking = input.param("tracking");
    ensure(token(tracking, 64), "invalid_input", 400)?;
    Reply::of(&db.route_package(c.dsp_id(), tracking)?)
}

fn days(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.route_days(c.dsp_id())?)
}

fn day(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let day = input.param("day");
    v::date(day)?;
    let view = db
        .route_day(c.dsp_id(), day)?
        .ok_or_else(|| Error::new("not_found", 404))?;
    Reply::of(&view)
}

fn retention(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.route_retention(c.dsp_id())?)
}

/// Sets the window: `days` from 30 to 3650, or null to keep every day.
fn set_retention(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["days"])?;
    let days = match b.get("days") {
        Some(serde_json::Value::Null) => None,
        Some(_) => Some(v::integer(
            b,
            "days",
            routedata::MIN_RETENTION_DAYS,
            routedata::MAX_RETENTION_DAYS,
        )?),
        None => return Err(dispatch_core::Error::new("invalid_input", 400)),
    };
    Reply::of(&db.set_route_retention(c.dsp_id(), Some(c.actor()), days)?)
}

/// Queues one day, the most recent completed one unless `date` names another, or up to
/// five days ending there; `mode` is `final` unless `snapshot` asks for today as it stands.
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId", "date", "mode", "days"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let date = if b.get("date").is_some() {
        let date = v::text(b, "date", 10, 10)?;
        v::date(date)?;
        Some(date)
    } else {
        None
    };
    let mode = if b.get("mode").is_some() {
        Mode::parse(v::choice(b, "mode", &["final", "snapshot"])?)?
    } else {
        Mode::Final
    };
    let days = if b.get("days").is_some() {
        v::integer(b, "days", 1, MAX_DAYS_PER_REQUEST)?
    } else {
        1
    };
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let jobs = db.enqueue_routes(id, actor, key, date, mode, days)?;
    db.audit(
        actor,
        Some(id),
        "routes.collection_requested",
        date.unwrap_or(routedata::COLLECTION),
    )?;
    Ok(Reply::status(jobs, 202))
}
