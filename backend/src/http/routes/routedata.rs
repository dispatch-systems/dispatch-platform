//! Daily routes: what is stored, and collecting a day. The collection is `routes` here
//! and in its paths; its module and database are `routedata`.
use crate::{
    Error, Result,
    db::Store,
    ensure,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
    routedata::{self, MAX_DAYS_PER_REQUEST, Mode},
    validate as v,
};

const VIEW: Dsp = Dsp("routes.view");
const COLLECT: Dsp = Dsp("routes.collect");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/routes/days", VIEW, days),
        read("/api/dsp/routes/days/{day}", VIEW, day),
        read(
            "/api/dsp/routes/days/{day}/itineraries/{id}",
            VIEW,
            itinerary,
        ),
        read("/api/dsp/routes/packages/{tracking}", VIEW, package),
        write("/api/dsp/routes/collect", COLLECT, collect),
    ]
}

fn itinerary(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let day = input.param("day");
    v::date(day)?;
    let id = input.param("id");
    ensure(routedata::token(id, 128), "invalid_input", 400)?;
    let view = db
        .route_itinerary(c.dsp_id(), day, id)?
        .ok_or_else(|| Error::new("not_found", 404))?;
    Reply::of(&view)
}

fn package(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let tracking = input.param("tracking");
    ensure(routedata::token(tracking, 64), "invalid_input", 400)?;
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
