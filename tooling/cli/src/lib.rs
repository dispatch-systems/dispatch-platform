//! `dispatchdev`: developing Dispatch from any checkout and in CI. It checks a branch before its
//! PR, runs the tests a change touches, signs in to the dev server `npm run dev` runs, builds the
//! backend and support worker through an exact-input cache, and makes features, collectors and
//! the rest from what the owners declare.
pub mod api;
pub mod build;
pub mod check;
pub mod machine;
pub mod observation;
pub mod scaffold;
pub mod shot;
pub mod slot;
pub mod test;
pub mod verify;
pub mod workspace;
pub use dispatch_shared::{Native, REPOSITORY, Result, Runner, hex, require, to_hex};
