//! Slack's logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod app;
pub(crate) mod author;
pub(crate) mod bot;
pub(crate) mod channel_links;
pub(crate) mod channels;
pub(crate) mod connection;
pub(crate) mod exposed;
pub(crate) mod fixture;
pub(crate) mod home;
pub(crate) mod inbound;
pub(crate) mod integrations;
pub(crate) mod kit;
pub(crate) mod link;
pub(crate) mod logo;
pub(crate) mod maintenance;
pub(crate) mod members;
pub(crate) mod messages;
pub(crate) mod pages;
pub(crate) mod people;
pub(crate) mod settings;
pub(crate) mod slack;
pub(crate) mod storage;
#[cfg(any(test, feature = "testing"))]
pub(crate) mod testing;
pub(crate) mod work;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;
