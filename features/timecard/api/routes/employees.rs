//! Employee search, period reads and collection requests.
use super::{EMPLOYEES, sent_period, tab};
use crate::backend::{TimecardStore, punches::employees::identify};
use dispatch_core::{
    Result,
    db::Store,
    foundation::validate as v,
    manifest::people,
    server::http::{
        input::{Input, Reply, descending, optional, optional_text, query_number},
        route::Member,
    },
};
use dispatch_paycom::timecards::EmployeeTimecardPeriod;

pub(super) fn roster_key(_: &Store, c: &Member, _: &Input) -> Result<String> {
    tab(c, EMPLOYEES)?;
    Ok("roster".into())
}

pub(super) fn employees(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let q = &input.query;
    v::fields(q, &["q", "direction", "offset", "limit", "status"])?;
    let query = optional_text(q, "q", 100)?;
    let desc = descending(q)?;
    let offset = query_number(q, "offset", 0, 0, 100000)?;
    let limit = if q["limit"] == "all" {
        None
    } else {
        Some(query_number(q, "limit", 50, 1, 100)?)
    };
    let status = optional(q, "status", |q, key| {
        v::choice(q, key, &["all", "active", "inactive"])
    })?
    .unwrap_or("all");
    let active = match status {
        "active" => Some(true),
        "inactive" => Some(false),
        _ => None,
    };
    let roster = c.state.read_cache.shared(
        dispatch_core::server::cache::Scope::tenant(
            crate::backend::punches::employees::CACHED,
            c.dsp_id(),
        ),
        "roster".into(),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || crate::backend::punches::employees::roster(db, c.dsp_id()),
    )?;
    let mut page = roster.page(query, offset, limit, desc, active);
    let people = people::everyone(db, c.state, c.dsp_id())?;
    for employee in &mut page.employees {
        identify(employee, &people);
    }
    Reply::of(&page)
}

pub(super) fn employee(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let q = &input.query;
    v::fields(q, &["from", "to", "date"])?;
    let date = optional(q, "date", |q, key| v::text(q, key, 10, 10))?;
    dispatch_core::ensure(
        date.is_none() || (q.get("from").is_none() && q.get("to").is_none()),
        "invalid_period",
        400,
    )?;
    let period = if q.get("from").is_some() || q.get("to").is_some() {
        let from = v::text(q, "from", 10, 10)?;
        let to = v::text(q, "to", 10, 10)?;
        v::date(from)?;
        v::date(to)?;
        dispatch_core::ensure(from <= to, "invalid_period", 400)?;
        Some(EmployeeTimecardPeriod {
            from: from.into(),
            to: to.into(),
        })
    } else {
        None
    };
    let period = if date.is_some() {
        Some(db.timecard_period(c.dsp_id(), Some(input.param("code")), date, None)?)
    } else {
        period
    };
    let mut timecard = db.employee_timecard(c.dsp_id(), input.param("code"), period.as_ref())?;
    identify(
        &mut timecard.employee,
        &*people::everyone(db, c.state, c.dsp_id())?,
    );
    Reply::of(&timecard)
}

pub(super) fn sync_employee(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let body = &input.body;
    v::fields(body, &["requestId", "from", "to"])?;
    let period = sent_period(body)?;
    let code = input.param("code");
    let job = db.enqueue_employee_timecard(
        c.dsp_id(),
        Some(c.actor()),
        v::text(body, "requestId", 1, 128)?,
        code,
        &period,
    )?;
    c.audit(
        db,
        "collection.requested",
        &format!("{code} {}–{}", period.from, period.to),
    )?;
    Ok(Reply::status(job, 202))
}

pub(super) fn employee_sync_status(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let query = &input.query;
    v::fields(query, &["from", "to", "requestId"])?;
    let period = sent_period(query)?;
    let request_id = optional(query, "requestId", |query, key| v::text(query, key, 1, 128))?;
    Reply::of(&db.employee_sync_status(c.dsp_id(), &period, request_id)?)
}

pub(super) fn sync_employees(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let body = &input.body;
    v::fields(body, &["requestId", "from", "to"])?;
    let period = sent_period(body)?;
    let status = db.enqueue_employee_sync(
        c.dsp_id(),
        c.actor(),
        v::text(body, "requestId", 1, 128)?,
        &period,
    )?;
    c.audit(
        db,
        "collection.requested",
        &format!("{}–{}", period.from, period.to),
    )?;
    Ok(Reply::status(serde_json::to_value(status)?, 202))
}
