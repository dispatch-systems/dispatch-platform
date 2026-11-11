//! Conduct's logic. It is private to the crate: the routes in `api/` call it.
//!
//! - `categories`: the kinds of writeup and the wording each starts with.
//! - `writeups`: each writeup, from written to signed.
//! - `copies`: photos of signed paper.
//! - `backups`: backing those photos up where the DSP keeps its files.
//! - `standing`: the rules a category's tiers climb and step down by.
//! - `overview`: what the page reads, in one answer.
//! - `settings`: what a DSP sets on Conduct's page of Settings.
pub(crate) mod backups;
pub(crate) mod categories;
pub(crate) mod copies;
pub(crate) mod history;
pub(crate) mod overview;
mod requests;
pub(crate) mod settings;
pub(crate) mod standing;
pub(crate) mod writeups;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;

use dispatch_core::{
    Result,
    db::{Db, Store},
    server::cache::DataDomain,
};

/// The kind of data Conduct's writes change.
pub const DOMAIN: DataDomain = DataDomain::new("conduct");
pub const CACHED: &str = "conduct.overview";

/// Runs `change` in a transaction of the DSP's database `db` inside one of the platform's, for a
/// change with its event in the activity log: either commits with the other, unless the
/// platform's own commit fails after the DSP's.
pub(crate) fn logged<T>(store: &Store, db: &Db, change: impl FnOnce() -> Result<T>) -> Result<T> {
    store.platform.transaction(|| db.transaction(change))
}
