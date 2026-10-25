//! Routes: a station's day of itineraries, as the execution pages read them.
mod browser;
mod capture;
pub(crate) mod collect;
mod native;
mod signed;

pub use browser::Browser;
pub use capture::{
    Capture, Collection, ItineraryCapture, JOB_KIND, MAX_BODY, MAX_CAPTURE_BYTES, MAX_ITINERARIES,
    Mode, Request, add_capture_bytes, fixture, listed, token,
};
pub use collect::{Day, INVALID, NATIVE};
pub use native::{Failure, Native};
