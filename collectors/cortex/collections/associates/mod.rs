//! Associates: everyone Amazon lists for the DSP's company, active, inactive or offboarded,
//! with their contacts, roles and photos, as the DA Console's associates and offboarded pages
//! read them.
mod capture;
pub(crate) mod collect;
mod fixture;
mod shrink;

pub use capture::{
    Associate, Capture, Collection, JOB_KIND, KeptPhotos, MAX_ASSOCIATES, MAX_PHOTO_BYTES, Photo,
    PhotoFormat, Request,
};
pub use fixture::fixture;
pub use shrink::MAX_KEPT_PHOTO_BYTES;
