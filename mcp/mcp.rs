//! The MCP: how outside agents connect to Dispatch, and all they use there, in one place.
//! Agent keys (`keys`) and the apps the owner connects with Sign in with Dispatch (`oauth`),
//! a key of kind `app`, sign in; the MCP server (`server`) offers them the tools
//! (`tools`) their connection allows; what keys and apps call is kept for the Agents page's
//! Activity log (`activity`). Core reaches it only through the agents' piece the app installs
//! (`piece`), and it reaches core and features as any owner above them does.
#[path = "backend/activity.rs"]
pub mod activity;
pub mod api;
#[path = "backend/keys.rs"]
mod keys;
#[path = "backend/migrations.rs"]
mod migrations;
#[path = "backend/oauth/mod.rs"]
pub mod oauth;
#[path = "backend/piece.rs"]
pub mod piece;
#[path = "backend/server.rs"]
pub mod server;
/// The MCP's test support, for its own tests and, behind the `testing` feature, the app's.
#[cfg(any(test, feature = "testing"))]
#[path = "tests/support/testing.rs"]
pub mod testing;
#[path = "backend/token.rs"]
mod token;
#[path = "backend/tools/mod.rs"]
pub mod tools;
#[path = "backend/usage.rs"]
mod usage;

pub use activity::{Activity, ActivityStore};
pub use keys::{Caller, KeyStore, client_label};
pub use oauth::{OAuthStore, clients::ClientStore, guard::GuardStore};
pub use piece::{AGENTS, Agent};
pub use usage::{LastUse, PER_MINUTE, Usage};
