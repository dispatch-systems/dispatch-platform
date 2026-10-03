//! The server: HTTP, `State`, the response cache, live updates, presence, operations and mail.
#[path = "../api/mod.rs"]
pub mod api;
pub mod cache;
pub mod http;
pub mod live;
pub mod mail;
pub mod operations;
pub mod presence;
pub(crate) mod state;
