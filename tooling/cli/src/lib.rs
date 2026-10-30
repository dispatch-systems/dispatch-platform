//! `dispatchdev`: developing Dispatch from any checkout and in CI. It starts a change in a worktree
//! of its own, checks the branch and opens its PR, removes the change once it merged, runs the
//! tests a change touches, signs in to the dev server `npm run dev` runs, builds the
//! backend and support worker through an exact-input cache, and makes features, collectors and
//! the rest from what the owners declare.
pub mod api;
pub mod build;
pub mod change;
pub mod check;
pub mod machine;
pub mod observation;
pub mod pr;
pub mod scaffold;
pub mod shot;
pub mod slot;
pub mod test;
pub mod verify;
pub mod workspace;
pub use dispatch_shared::{Native, REPOSITORY, Result, Runner, hex, require, to_hex};
