//! Slack persistence in each DSP's database. Each write keeps its transaction boundary;
//! conversation and group settlement share the same rules and change notifications.
pub(crate) mod catch_up;
pub(crate) mod conversations;
pub(crate) mod messages;
mod migrations;
pub(crate) mod replies;
pub(crate) mod settlement;

pub(crate) use migrations::{add_done_places, add_groups};

use dispatch_core::server::cache::DataDomain;

/// What Communications' writes change, for the read cache: its own data alone.
pub const DOMAIN: DataDomain = DataDomain::new("communications");

#[cfg(test)]
#[path = "../../tests/backend/storage.rs"]
mod tests;
