//! Daily Performance's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod exposed;
pub(crate) mod keeper;
pub(crate) mod people;
pub(crate) mod settings;
pub(crate) mod storage;
