//! Meal breaks: the meal evidence of a station's day, itinerary by itinerary, shown live
//! as a collection finds it.
mod capture;
pub(crate) mod collect;
mod live;

pub use capture::{Capture, Coverage, Itinerary, JOB_KIND, Meal, fixture};
pub use live::Writer;
