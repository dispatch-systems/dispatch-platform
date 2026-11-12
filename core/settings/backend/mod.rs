//! DSP and member settings declared through `manifest::settings`.
//! Storage and saving policy: `core/settings/README.md`.
//!
//! `store` owns records, `kinds` validates fields, `read` and `save` serve product callers,
//! and `api` serves members. `forget_member` removes a member's own settings.
mod aliases;
#[path = "../api/mod.rs"]
pub mod api;
pub(crate) mod kinds;
mod read;
mod save;
mod store;

pub(crate) use aliases::prepare;
pub use aliases::{read_key, write_key};
pub use read::{read, read_member};
pub(crate) use save::save_listing;
pub use save::{Saved, save};
pub(crate) use store::forget_member;

/// The read cache's name for what a member's Settings shows of a DSP, evicted by every write
/// to that DSP's data but a collection's live pages.
pub const CACHED: &str = "settings";

#[cfg(test)]
#[path = "../tests/backend/store.rs"]
mod tests;
