//! Meal comparisons, period gaps and Cortex publications.
use super::{KINDS, MEAL_BREAKS, day_key, page_bounds, sent_period, tab};
use crate::backend::{TimecardStore, settings};
use dispatch_core::{
    Result,
    db::Store,
    foundation::validate as v,
    manifest::people,
    server::http::{
        input::{Input, Reply, descending, optional, optional_text},
        route::Member,
    },
};

pub(super) fn meal_key(db: &Store, c: &Member, input: &Input) -> Result<String> {
    tab(c, MEAL_BREAKS)?;
    day_key(db, c, input)
}

pub(super) fn period_key(db: &Store, c: &Member, input: &Input) -> Result<String> {
    tab(c, MEAL_BREAKS)?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let period = crate::backend::meals::period::gap_period(db, c.dsp_id(), date)?;
    Ok(format!(
        "{}:{}",
        serde_json::to_string(&period)?,
        c.dsp.timezone
    ))
}

pub(super) fn meal_page(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    let q = &input.query;
    v::fields(
        q,
        &[
            "date",
            "q",
            "filter",
            "sort",
            "direction",
            "offset",
            "limit",
        ],
    )?;
    let date = v::text(q, "date", 10, 10)?;
    let search = optional_text(q, "q", 100)?;
    let filter = optional(q, "filter", |q, key| {
        v::choice(q, key, &["all", "late", "different", "missing", "gaps"])
    })?
    .unwrap_or("all");
    let sort = optional(q, "sort", |q, key| {
        v::choice(
            q,
            key,
            &[
                "employee",
                "inDay",
                "outDay",
                "outLunch",
                "inLunch",
                "firstDelivery",
                "lastDelivery",
                "comparison",
            ],
        )
    })?
    .unwrap_or("employee");
    let desc = descending(q)?;
    let (offset, limit) = page_bounds(q)?;
    let load = || {
        let people = people::everyone(db, c.state, c.dsp_id())?;
        db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str(), &people)
    };
    let live = db.any_active_job(c.dsp_id(), KINDS)?;
    let day = if live {
        std::sync::Arc::new(load()?)
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
            format!("meal-table:{date}:{}", c.dsp.timezone),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            load,
        )?
    };
    let listing: settings::Listing =
        dispatch_core::settings::read(db, c.dsp_id(), &settings::ROSTER)?;
    let query = crate::backend::meals::page::Query {
        search,
        filter,
        sort,
        desc,
        offset,
        limit,
        name_order: listing.name_order.as_str(),
    };
    let ordering = if live {
        std::sync::Arc::new(crate::backend::meals::page::order(&day, &query))
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
            format!(
                "meal-order:{date}:{}:{}:{sort}:{desc}",
                c.dsp.timezone, query.name_order
            ),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            || Ok(crate::backend::meals::page::order(&day, &query)),
        )?
    };
    Reply::of(&crate::backend::meals::page::page(&day, query, &ordering))
}

pub(super) fn meal_comparison(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let load = || {
        let people = people::everyone(db, c.state, c.dsp_id())?;
        db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str(), &people)
    };
    // Live pages depend on current job leases, actor permissions and connection revision.
    // Check them afresh during collection rather than caching that moving visibility.
    if db.any_active_job(c.dsp_id(), KINDS)? {
        return Reply::of(&load()?);
    }
    let comparison = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
        format!("meals:{}:{}:{}", c.dsp_id(), date, c.dsp.timezone),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        load,
    )?;
    Ok(Reply::encoded(comparison))
}

pub(super) fn meal_period(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    let q = &input.query;
    v::fields(q, &["source", "date", "from", "to"])?;
    let source = v::text(q, "source", 1, 200)?;
    let date = optional(q, "date", |q, key| v::text(q, key, 10, 10))?;
    dispatch_core::ensure(
        date.is_none() || (q.get("from").is_none() && q.get("to").is_none()),
        "invalid_period",
        400,
    )?;
    let requested = if q.get("from").is_some() || q.get("to").is_some() {
        Some(sent_period(q)?)
    } else {
        None
    };
    Reply::of(&db.meal_period(
        c.dsp_id(),
        source,
        date,
        requested.as_ref(),
        c.dsp.timezone.as_str(),
        &*people::everyone(db, c.state, c.dsp_id())?,
    )?)
}

pub(super) fn meal_period_gaps(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let timezone = c.dsp.timezone.as_str();
    // As the day's comparison: read afresh while a collection runs, and cached otherwise.
    let people = || people::everyone(db, c.state, c.dsp_id());
    if db.any_active_job(c.dsp_id(), KINDS)? {
        return Reply::of(&db.meal_period_gaps(c.dsp_id(), date, timezone, &*people()?)?);
    }
    let period = crate::backend::meals::period::gap_period(db, c.dsp_id(), date)?;
    let gaps = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
        format!(
            "meal-period-gaps:{}:{}",
            serde_json::to_string(&period)?,
            timezone
        ),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || crate::backend::meals::period::gaps_for(db, c.dsp_id(), period, timezone, &*people()?),
    )?;
    Ok(Reply::encoded(gaps))
}

pub(super) fn cortex_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(Reply::json(db.meal_publications(c.dsp_id(), date)?))
}
