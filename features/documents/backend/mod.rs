//! Documents's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod connection;
pub(crate) mod google;
pub(crate) mod maintenance;
pub(crate) mod storage;
