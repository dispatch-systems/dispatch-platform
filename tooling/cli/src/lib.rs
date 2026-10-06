//! `dispatchdev`: developing Dispatch from this machine and in CI. It takes a change from its
//! worktree and preview through its checks and PR to Dev, builds the backend through an
//! exact-input cache, and ships PRs through the merge queue.
pub mod api;
pub mod build;
pub mod check;
pub mod finish;
pub mod logs;
pub mod pr;
pub mod preview;
pub mod ship;
pub mod start;
pub mod status;
pub mod test;
pub mod workspace;
pub use dispatch_shared::{Native, REPOSITORY, Result, Runner, hex, require, to_hex};
