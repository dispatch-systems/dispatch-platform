//! Source health and the daily collection commands and status.
use crate::{
    api::status::{PaycomStatus, PaycomWorkforce},
    backend::{TimecardStore, settings},
};
use dispatch_core::{
    Error, Result,
    accounts::api::requests::CollectionRequest,
    collection::api::routes::connections,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Member,
    },
};
use dispatch_cortex::discovery::Scope;
use dispatch_paycom as paycom;

pub(super) fn paycom_status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let publication: Option<(String,)> = db
        .collector(c.dsp_id(), paycom::PROVIDER)?
        .one_as("SELECT collected_at FROM publications WHERE active=1", [])?;
    Ok(Reply::json(serde_json::to_value(PaycomStatus {
        connection: connections::summary(db, c, paycom::PROVIDER)?,
        workforce: PaycomWorkforce {
            collected_at: publication.map(|(at,)| at),
        },
    })?))
}

/// What the page shows by, of the DSP's settings, which Settings saves.
pub(super) fn paycom_settings(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&settings::shown(db, c.dsp_id())?)
}

pub(super) fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let request = CollectionRequest::parse(&input.body, false)?;
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let job = if let Some(date) = &request.date {
        db.enqueue_paycom_date(id, actor, &request.request_id, date)?
    } else {
        db.enqueue_timecards(id, actor, &request.request_id)?
    };
    let date = request.date.as_deref().unwrap_or("");
    db.audit(actor, Some(id), "collection.requested", date)?;
    Ok(Reply::status(job, 202))
}

pub(super) fn meal_sync_status(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(Reply::json(serde_json::to_value(
        db.meal_sync_status(c.dsp_id(), date)?,
    )?))
}

pub(super) fn sync_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let request = CollectionRequest::parse(&input.body, true)?;
    let date = request
        .date
        .as_deref()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let (id, actor) = (c.dsp_id(), c.actor());
    let result = db.enqueue_meal_sync(id, actor, &request.request_id, date)?;
    db.audit(Some(actor), Some(id), "meal_breaks.sync_requested", date)?;
    Ok(Reply::status(result, 202))
}

pub(super) fn collect_cortex_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    let scope = Scope::request(b, c.dsp.timezone.as_str())?;
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let job = db.enqueue_meals(id, actor, v::text(b, "requestId", 1, 128)?, &scope)?;
    db.audit(actor, Some(id), "cortex.collection.requested", "")?;
    Ok(Reply::status(job, 202))
}
