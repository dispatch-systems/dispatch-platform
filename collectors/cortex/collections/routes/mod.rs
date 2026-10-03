//! Routes: a station's day of itineraries, as the execution pages read them.
mod capture;
pub(crate) mod collect;

pub use capture::{
    Capture, Collection, ItineraryCapture, JOB_KIND, MAX_BODY, MAX_CAPTURE_BYTES, MAX_ITINERARIES,
    Mode, Request, add_capture_bytes, fixture, listed, token,
};
