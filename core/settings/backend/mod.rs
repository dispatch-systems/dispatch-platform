//! A DSP's settings: the values of the sections features and core declare
//! (`manifest::settings`), each kept for the DSP or for each member in the DSP's own database,
//! checked as its fields' kinds say, and recorded in the log as they change. A feature reads
//! its own with `read`; Settings, and anything else that changes one, saves through `save`;
//! a feature moves values it kept itself before into its section with `carry_over`; a removed
//! member's own go with their account (`forget_member`).
//!
//! `store` keeps the records, `kinds` says what each kind of field holds and how it reads,
//! `read` and `save` are what the rest of the product calls, and `api` what members reach.
#[path = "../api/mod.rs"]
pub mod api;
pub(crate) mod kinds;
mod read;
mod save;
mod store;

pub use read::{read, read_member};
pub(crate) use save::save_listing;
pub use save::{Saved, carry_over, save};
pub(crate) use store::forget_member;

/// The read cache's name for what a member's Settings shows of a DSP, evicted by every write
/// to that DSP's data but a collection's live pages.
pub const CACHED: &str = "settings";

#[cfg(test)]
#[path = "../tests/backend/store.rs"]
mod tests;
