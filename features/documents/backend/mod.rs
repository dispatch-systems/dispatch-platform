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
pub(crate) mod query;
pub(crate) mod service;
pub(crate) mod storage;
pub(crate) mod team;
pub(crate) mod tree;

use std::sync::{Mutex, MutexGuard};

/// Holds `mutex`, as a panic while another held it left it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;
