//! Associates: everyone Amazon lists for the DSP's company, active or not, with their
//! contacts, roles and photos, as the DA Console's associates page reads them.
mod capture;
pub(crate) mod collect;
mod fixture;

pub use capture::{
    Associate, Capture, Collection, JOB_KIND, MAX_ASSOCIATES, MAX_PHOTO_BYTES, Photo, PhotoFormat,
    Request,
};
pub use fixture::fixture;
