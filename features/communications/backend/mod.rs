//! Communications' logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
//!
//! - `model`: what every kind of conversation shares: its kind, whether it waits, and who wrote
//!   a message.
//! - `storage`: the Slack conversations and messages, and whether each waits on an answer;
//!   `changes`, each change to a conversation of either kind, in order, for the pages waiting on
//!   them; `files`, the photos and files attached, kept in the DSP's own data, and downloading
//!   members' from Slack.
//! - `dispatcher`: one conversation of either transport, by its reference: reading it, its
//!   profile, sending in it, Done and who's writing.
//! - `inbox`: the list of every kind as the page reads it, what changed in it, and what waits;
//!   `sections`, the page's sections, each listing a kind of conversation: whether the DSP has it,
//!   what waits in it, what's listed of it and how it's searched.
//! - `conversations`: Slack's conversations as the page reads them; `people`, who each is with and
//!   who at the DSP wrote in it, as every kind shows them; `elsewhere`, a person's conversation of
//!   each kind; `contacts`, whom the DSP can message; `search`, finding people by a name typed;
//!   `profile`, what else is known of whom a conversation is with: their route today (`today`) and
//!   how to reach them.
//! - `reply`: answering, and messaging someone first; `compose`, what a message may carry, checked
//!   before it goes; `sender`, the name and photo an answer goes out under, and `initials`, the
//!   image of someone's initials when they have no photo.
//! - `groups`: messaging a group, each in their own direct message, and their answers together.
//! - `live`: what reaches the page as it happens, and who is writing.
//! - `texts`: the texts the DSP's dispatch phones relay, each filed in a conversation, and
//!   answering by text; `phone`, the phones themselves, pairing them, the texts they relay and
//!   Dispatch's queued for them, and their bytes, knowing nothing of conversations.
//! - `text`: Slack's markup, as Communications keeps and sends text.
//! - `display`: how the page looks to each member, their own in Settings.
pub(crate) mod changes;
pub(crate) mod compose;
pub(crate) mod contacts;
pub(crate) mod conversations;
pub(crate) mod dispatcher;
pub(crate) mod display;
pub(crate) mod elsewhere;
pub(crate) mod files;
pub(crate) mod groups;
pub(crate) mod inbox;
pub(crate) mod initials;
pub(crate) mod live;
pub(crate) mod model;
pub(crate) mod people;
pub(crate) mod phone;
pub(crate) mod profile;
pub(crate) mod reply;
pub(crate) mod search;
pub(crate) mod sections;
pub(crate) mod sender;
pub(crate) mod storage;
pub(crate) mod text;
pub(crate) mod texts;
pub(crate) mod today;

#[cfg(test)]
#[path = "../tests/backend/support/mod.rs"]
pub(crate) mod support;
