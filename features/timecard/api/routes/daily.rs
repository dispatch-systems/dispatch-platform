//! Daily timecard pages and punches, sharing one cached day.
use super::{DAILY, KINDS, day_key, page_bounds, tab};
use crate::backend::TimecardStore;
use dispatch_core::{
    Error, Result,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, descending, optional},
        route::Member,
    },
};

const SORTS: &[&str] = &[
    "name",
    "hours",
    "inDay",
    "outLunch",
    "inLunch",
    "outDay",
    "totalHours",
    "condition",
];

pub(super) fn daily_key(db: &Store, c: &Member, input: &Input) -> Result<String> {
    tab(c, DAILY)?;
    day_key(db, c, input)
}

pub(super) fn timecards(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    let q = &input.query;
    v::fields(q, &["date", "sort", "direction"])?;
    let sort = optional(q, "sort", |q, key| v::choice(q, key, SORTS))?.unwrap_or("name");
    let date = v::text(q, "date", 10, 10)?;
    Reply::of(&db.daily_timecards(c.dsp_id(), date, sort, descending(q)?)?)
}

pub(super) fn timecard_page(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    let q = &input.query;
    v::fields(q, &["date", "sort", "direction", "offset", "limit"])?;
    let date = v::text(q, "date", 10, 10)?;
    let sort = optional(q, "sort", |q, key| v::choice(q, key, SORTS))?.unwrap_or("name");
    let desc = descending(q)?;
    let (offset, limit) = page_bounds(q)?;
    let day = cached_daily(db, c, date, sort, desc)?;
    Reply::of(&crate::api::types::DailyTimecardPage {
        total: day.rows.len(),
        timecards: crate::api::types::DailyTimecards {
            rows: day
                .rows
                .iter()
                .skip(offset)
                .take(limit.unwrap_or(usize::MAX))
                .cloned()
                .collect(),
            available: day.available,
            collected_at: day.collected_at.clone(),
        },
    })
}

pub(super) fn daily_punches(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let day = cached_daily(db, c, date, "name", false)?;
    let card = day
        .rows
        .iter()
        .find(|row| row.card.employee_code == input.param("code"))
        .ok_or_else(|| Error::new("not_found", 404))?;
    Reply::of(card)
}

fn cached_daily(
    db: &Store,
    c: &Member,
    date: &str,
    sort: &str,
    desc: bool,
) -> Result<std::sync::Arc<crate::api::types::DailyTimecards>> {
    let load = || db.daily_timecards(c.dsp_id(), date, sort, desc);
    let day = if db.any_active_job(c.dsp_id(), KINDS)? {
        std::sync::Arc::new(load()?)
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(
                crate::backend::punches::daily::CACHED,
                c.dsp_id(),
            ),
            format!("{date}:{sort}:{desc}"),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            load,
        )?
    };
    Ok(day)
}
