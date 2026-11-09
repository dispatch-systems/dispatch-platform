//! Documents's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
mod access;
mod cache;
pub(crate) mod connection;
mod description;
pub(crate) mod drive;
pub(crate) mod files;
pub(crate) mod google;
mod grant;
pub(crate) mod maintenance;
pub(crate) mod mirror;
pub(crate) mod picker;
mod query;
pub(crate) mod service;
pub(crate) mod storage;
pub(crate) mod team;
pub(crate) mod tree;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;
