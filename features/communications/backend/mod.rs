//! Communications' private backend: Slack conversations, text conversations, phone transport
//! and shared inbox, people, attachment and notification services. `feature.rs` re-exports the
//! app's and declared dependants' entry points.
pub(crate) mod attachments;
pub(crate) mod bytes;
pub(crate) mod changes;
pub(crate) mod compose;
pub(crate) mod contacts;
pub(crate) mod conversations;
pub(crate) mod copies;
pub(crate) mod dispatcher;
pub(crate) mod display;
pub(crate) mod elsewhere;
pub(crate) mod files;
pub(crate) mod gifs;
pub(crate) mod groups;
pub(crate) mod inbox;
pub(crate) mod initials;
pub(crate) mod live;
pub(crate) mod logged;
pub(crate) mod model;
pub(crate) mod people;
pub(crate) mod phone;
pub(crate) mod previews;
pub(crate) mod profile;
pub(crate) mod push;
pub(crate) mod quick_replies;
pub(crate) mod reply;
pub(crate) mod search;
pub(crate) mod sections;
pub(crate) mod sender;
pub(crate) mod storage;
pub(crate) mod text;
pub(crate) mod texts;
pub(crate) mod today;
pub(crate) mod windows;

#[cfg(test)]
#[path = "../tests/backend/support/mod.rs"]
pub(crate) mod support;
