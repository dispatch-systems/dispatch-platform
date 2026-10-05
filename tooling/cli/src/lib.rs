//! `dispatchdev`: developing Dispatch from this machine and in CI. It builds the backend
//! through an exact-input cache, checks a branch before it is pushed and ships a PR through
//! the merge queue.
pub mod build;
pub mod check;
pub mod ship;
pub use dispatch_shared::{Native, REPOSITORY, Result, Runner, hex, require, to_hex};
