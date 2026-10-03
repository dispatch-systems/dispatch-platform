//! Dispatch's core: the platform with every collector and feature removed. Each part is a
//! directory of its own whose module root is its `backend/mod.rs`. Core names no collector or
//! feature: it reaches them through the registry the app installs at startup (`manifest`).
#[path = "accounts/backend/mod.rs"]
pub mod accounts;
#[path = "collection/backend/mod.rs"]
pub mod collection;
#[path = "db/backend/mod.rs"]
pub mod db;
#[path = "foundation/backend/mod.rs"]
pub mod foundation;
#[path = "manifest/backend/mod.rs"]
pub mod manifest;
#[path = "mcp/backend/mod.rs"]
pub mod mcp;
#[path = "platform_owner/backend/mod.rs"]
pub mod platform_owner;
#[path = "server/backend/mod.rs"]
pub mod server;
#[path = "tenancy/backend/mod.rs"]
pub mod tenancy;
/// Core's test support, for core's tests and, behind the `testing` feature, every other
/// crate's.
#[cfg(any(test, feature = "testing"))]
#[path = "db/tests/support/common.rs"]
pub mod testing;

pub use foundation::error::{Code, Error, Result, ensure};
pub use server::state::{State, cancelled, supervise};
