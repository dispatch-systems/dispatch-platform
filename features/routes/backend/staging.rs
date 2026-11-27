//! Staging a checked capture a few itineraries at a time, with ownership rechecked at each write.
use super::{
    RoutesStore,
    shaping::{Shaper, prepare_lists},
    storage::stage_prepared_routes_start,
};
use dispatch_core::{Error, Result, db::Store, foundation::observability};
use dispatch_cortex::routes::{Capture, Collection, ItineraryCapture, Request};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Itineraries staged in one transaction at most. Each commit waits on the disk, which was
/// most of an itinerary's write: a day of seventy itineraries commits nine times rather than
/// seventy, and a batch's turn holds the platform's shared transition about as long as one
/// slow itinerary's did.
const BATCH_ITINERARIES: usize = 8;
/// As much itinerary text as one batch holds, so the batch being written and the next being
/// shaped stay a few megabytes together. An itinerary longer than this is staged alone.
const BATCH_BYTES: usize = 4 * 1024 * 1024;

/// A day stored as an inactive publication, a few itineraries at a time, waiting for its
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

/// Stores a finished day as an inactive publication of its job, a few itineraries at a time,
/// each write first checking the job is still this worker's.
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
/// Stores a day as an inactive publication under `key`, a batch of itineraries at a time,
/// once `capture` is checked whole against `request`. Checking and shaping happen on blocking
/// threads without the platform lock, the next batch shaped while one is written; each write
/// takes it only for its batch's rows, in one transaction, and first asks `mine` whether the
/// work is still the caller's. At most two batches' rows exist at once.
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
        .run_unpublished(dsp, move |db| {
            check(db)?;
            stage_prepared_routes_start(db, &d, &k, &capture, count, &responses)
        })
        .await?;
    let shape = |batch: Vec<ItineraryCapture>| {
        let shaper = shaper.clone();
        tokio::task::spawn_blocking(move || {
            batch
                .iter()
                .map(|captured| shaper.shape(captured))
                .collect::<Result<Vec<_>>>()
        })
    };
    let mut batches = batches(itineraries).into_iter();
    let mut next = batches.next().map(shape);
    while let Some(shaping) = next {
        let shaped = shaping.await.map_err(blocking)??;
        next = batches.next().map(shape);
        let (d, staged, check) = (dsp.to_owned(), staged.clone(), mine.clone());
        state
            .run_unpublished(dsp, move |db| {
                check(db)?;
                db.stage_routes_itineraries(&d, &staged, &shaped)
            })
            .await?;
    }
    Ok(staged)
}

/// `itineraries` in order, in the batches they are staged in: `BATCH_ITINERARIES` at most,
/// and no more than `BATCH_BYTES` of text but for an itinerary longer alone.
fn batches(itineraries: Vec<ItineraryCapture>) -> Vec<Vec<ItineraryCapture>> {
    let mut batches: Vec<Vec<ItineraryCapture>> = Vec::new();
    let mut bytes = 0;
    for itinerary in itineraries {
        let size = itinerary.detail.bytes();
        match batches.last_mut() {
            Some(batch) if batch.len() < BATCH_ITINERARIES && bytes + size <= BATCH_BYTES => {
                bytes += size;
                batch.push(itinerary);
            }
            _ => {
                bytes = size;
                batches.push(vec![itinerary]);
            }
        }
    }
    batches
}

/// A blocking thread's end, logged as `event`, when it ends before its work does: only a panic
/// ends one.
pub(super) fn blocking(event: &'static str) -> impl Fn(tokio::task::JoinError) -> Error + Copy {
    move |error| {
        observability::event("error", event, json!({"panic":error.is_panic()}));
        Error::new("routes_capture_invalid", 500)
    }
}

#[cfg(test)]
#[path = "../tests/backend/staging.rs"]
mod tests;
