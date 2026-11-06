//! Request paths, response ownership and the shared budget for a day's route reads.
use super::capture::{MAX_CAPTURE_BYTES, add_capture_bytes};
use dispatch_core::{Error, Result};
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const SUMMARIES: &str = "/operations/execution/api/summaries";
pub(super) const ROUTE_SUMMARIES: &str = "/operations/execution/api/route-summaries";
pub(super) const ITINERARY: &str = "/operations/execution/api/itineraries/";
/// Where the application's API action asks for itinerary `id`.
pub(crate) fn itinerary_path(id: &str) -> String {
    format!(
        "{ITINERARY}{}",
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
    )
}
/// Only the owner of an itinerary response, read without building the whole tree.
#[derive(Deserialize)]
struct Whose {
    #[serde(rename = "itineraryDetails")]
    details: WhoseDetails,
}
#[derive(Deserialize)]
struct WhoseDetails {
    #[serde(rename = "itineraryId")]
    itinerary: String,
    #[serde(rename = "transporterId")]
    transporter: String,
}
pub(super) fn owned_by(text: &str, id: &str, transporter: &str) -> bool {
    serde_json::from_str::<Whose>(text)
        .is_ok_and(|w| w.details.itinerary == id && w.details.transporter == transporter)
}

/// The itinerary lanes share one exact byte reservation for the day's accepted
/// responses. A failed reservation leaves the total unchanged.
pub(super) struct CaptureBudget(AtomicUsize);
impl CaptureBudget {
    pub(super) fn new(bytes: usize) -> Result<Self> {
        Ok(Self(AtomicUsize::new(add_capture_bytes(0, bytes)?)))
    }
    /// What the day may still hold.
    pub(super) fn remaining(&self) -> usize {
        MAX_CAPTURE_BYTES.saturating_sub(self.0.load(Ordering::SeqCst))
    }
    pub(super) fn add(&self, bytes: usize) -> Result<()> {
        self.0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |total| {
                add_capture_bytes(total, bytes).ok()
            })
            .map(|_| ())
            .map_err(|_| Error::new("routes_source_too_large", 502))
    }
}
