//! Conduct's logic. It is private to the crate: the routes in `api/` call it.
//!
//! - `categories`: the kinds of writeup and the wording each starts with.
//! - `writeups`: each writeup, from written to signed.
//! - `copies`: photos of signed paper.
//! - `standing`: the rules a category's tiers climb by.
//! - `overview`: what the page reads, in one answer.
//! - `settings`: what a DSP sets on Conduct's page of Settings.
pub(crate) mod categories;
pub(crate) mod copies;
pub(crate) mod overview;
pub(crate) mod settings;
pub(crate) mod standing;
pub(crate) mod writeups;

use dispatch_core::server::cache::DataDomain;

/// The kind of data Conduct's writes change.
pub const DOMAIN: DataDomain = DataDomain::new("conduct");
