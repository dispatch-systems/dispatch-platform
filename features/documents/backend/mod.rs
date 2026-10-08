//! Documents's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod connection;
pub(crate) mod drive;
pub(crate) mod files;
pub(crate) mod google;
pub(crate) mod maintenance;
pub(crate) mod storage;
pub(crate) mod tree;
