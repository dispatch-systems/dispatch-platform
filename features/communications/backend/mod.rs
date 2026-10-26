//! Communications' logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
//!
//! - `storage`: its conversations and messages, and whether each waits on an answer; `files`,
//!   the photos and files attached, kept in the DSP's own data, and downloading members' from
//!   Slack.
//! - `conversations`: the conversations as the page reads them, with who each is with;
//!   `contacts`, whom the DSP can message; `search`, finding people by a name typed; `profile`,
//!   what else is known of whom a conversation is with: their route today (`today`) and how to
//!   reach them.
//! - `reply`: answering, and messaging someone first; `sender`, the name and photo an answer
//!   goes out under, and `initials`, the image of someone's initials when they have no photo.
//! - `groups`: messaging a group, each in their own direct message, and their answers together.
//! - `live`: what reaches the page as it happens, and who is writing.
//! - `texts`: the texts the DSP's dispatch phones relay, each filed in a conversation, and
//!   answering by text.
//! - `text`: Slack's markup, as Communications keeps and sends text.
//! - `display`: how the page looks to each member, their own in Settings.
pub(crate) mod contacts;
pub(crate) mod conversations;
pub(crate) mod display;
pub(crate) mod files;
pub(crate) mod groups;
pub(crate) mod initials;
pub(crate) mod live;
pub(crate) mod profile;
pub(crate) mod reply;
pub(crate) mod search;
pub(crate) mod sender;
pub(crate) mod storage;
pub(crate) mod text;
pub(crate) mod texts;
pub(crate) mod today;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;
