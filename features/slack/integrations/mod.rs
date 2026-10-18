//! Slack's own integrations, a folder each: what it does in a DSP's workspaces once someone who
//! manages Slack switches it on there. Each folder holds all of one: its declaration and what
//! it keeps (`mod.rs`), its endpoints (`api.rs`), its frontend, its migrations and its tests.
//!
//! - `import_files`: Import Files, saving what's posted in linked channels to folders.
//! - `sync_canvases`: Sync Canvases, keeping a channel's canvases as documents in folders.
pub(crate) mod import_files;
pub(crate) mod sync_canvases;
