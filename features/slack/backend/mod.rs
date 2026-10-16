//! Slack's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
//!
//! - `app`: the Slack app as Slack knows it: its permissions, events, addresses and manifest.
//! - `slack`: talking to Slack, and `fixture`, fixture mode's stand-in for it.
//! - `bot`: the app's bot in one DSP's workspace, through which everything acts in Slack.
//! - `connection` and `link`: adding the app to a DSP's workspaces, and taking it out.
//! - `channels`: the channels of a workspace Dispatch is in, on its settings.
//! - `integrations`: what other features do through Slack, switched on in each workspace's
//!   settings.
//! - `members` and `people`: the members of its workspaces, and how Driver Match reads them.
//! - `inbound`: Slack's requests, checked and routed to what handles them, such as `home`.
//! - `storage`, `maintenance` and `pages`: what's kept, the hourly check, and the install
//!   link's own page.
pub(crate) mod app;
pub(crate) mod bot;
pub(crate) mod channels;
pub(crate) mod connection;
pub(crate) mod fixture;
pub(crate) mod home;
pub(crate) mod inbound;
pub(crate) mod integrations;
pub(crate) mod link;
pub(crate) mod maintenance;
pub(crate) mod members;
pub(crate) mod pages;
pub(crate) mod people;
pub(crate) mod slack;
pub(crate) mod storage;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;
