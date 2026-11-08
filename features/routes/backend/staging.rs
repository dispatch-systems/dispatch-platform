//! Staging a checked capture one itinerary at a time, with ownership rechecked at each write.
use super::{
    DOMAIN, RoutesStore,
    shaping::{Shaper, prepare_lists},
    storage::stage_prepared_routes_start,
};
use dispatch_core::{Error, Result, db::Store, foundation::observability};
use dispatch_cortex::routes::{Capture, Collection, Request};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A day stored as an inactive publication, one itinerary at a time, waiting for its
/// job to make it the day's active one.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Staged {
    pub collection: Collection,
    pub publication: String,
    pub day: String,
    pub station: String,
    pub service_area_id: String,
    pub provider: String,
    pub itineraries: usize,
}

/// Stores a finished day as an inactive publication of its job, one itinerary at a time, each
/// insert first checking the job is still this worker's.
pub async fn stage(
    state: &std::sync::Arc<dispatch_core::State>,
    dsp: &str,
    job: &str,
    owner: &str,
    capture: Capture,
) -> Result<Staged> {
    let (d, j) = (dsp.to_owned(), job.to_owned());
    let request = state
        .read(move |db| Ok(db.job_row(&j, Some(&d))?.request))
        .await?;
    let request = Request::parse(&serde_json::from_str(&request)?)?
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let (j, o) = (job.to_owned(), owner.to_owned());
    stage_capture(state, dsp, job, request, capture, move |db| {
        db.guard(&j, &o).map(drop)
    })
    .await
}
/// Stores a day as an inactive publication under `key`, one itinerary at a time, once
/// `capture` is checked whole against `request`. Checking and shaping happen on blocking
/// threads without the platform lock; each insert takes it only for that itinerary's rows,
/// and first asks `mine` whether the work is still the caller's. Only one itinerary's
/// reading and rows exist at once.
pub(super) async fn stage_capture(
    state: &std::sync::Arc<dispatch_core::State>,
    dsp: &str,
    key: &str,
    request: Request,
    mut capture: Capture,
    mine: impl Fn(&Store) -> Result<()> + Clone + Send + 'static,
) -> Result<Staged> {
    let blocking = blocking("routes.stage_failed");
    let (capture_checked, responses) = tokio::task::spawn_blocking(move || {
        capture.validate(&request)?;
        capture.validate_response_budget()?;
        let responses = prepare_lists(&capture)?;
        Ok::<_, Error>((capture, responses))
    })
    .await
    .map_err(blocking)??;
    capture = capture_checked;
    let shaper = std::sync::Arc::new(Shaper::new(&capture));
    let itineraries = std::mem::take(&mut capture.itineraries);
    let count = itineraries.len();
    let (d, k, check) = (dsp.to_owned(), key.to_owned(), mine.clone());
    let staged = state
        .run_scoped(dsp, DOMAIN, move |db| {
            check(db)?;
            stage_prepared_routes_start(db, &d, &k, &capture, count, &responses)
        })
        .await?;
    for captured in itineraries {
        let shaper = shaper.clone();
        let shaped = tokio::task::spawn_blocking(move || shaper.shape(&captured))
            .await
            .map_err(blocking)??;
        let (d, staged, check) = (dsp.to_owned(), staged.clone(), mine.clone());
        state
            .run_scoped(dsp, DOMAIN, move |db| {
                check(db)?;
                db.stage_routes_itinerary(&d, &staged, &shaped)
            })
            .await?;
    }
    Ok(staged)
}

/// A blocking thread's end, logged as `event`, when it ends before its work does: only a panic
/// ends one.
pub(super) fn blocking(event: &'static str) -> impl Fn(tokio::task::JoinError) -> Error + Copy {
    move |error| {
        observability::event("error", event, json!({"panic":error.is_panic()}));
        Error::new("routes_capture_invalid", 500)
    }
}
