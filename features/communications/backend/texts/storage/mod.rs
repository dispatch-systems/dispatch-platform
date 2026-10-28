//! Texting's storage, in each DSP's own database, beside the Slack conversations', by concern:
//! each conversation by text and the texts filed in it, on the phone side's rows of them
//! (`messages`); the names and blocks the DSP keeps for numbers (`contacts`); and the drivers
//! picked for work phones nobody has (`picks`). What each keeps is `model`'s. Each write is one
//! transaction of its own, or a part of its caller's. Whether a conversation waits follows from
//! its texts, and is kept beside them each time they change, as a Slack conversation's is.
mod contacts;
mod messages;
mod picks;

pub use contacts::ContactStore;
pub use messages::TextStore;
pub(crate) use messages::{add_filing, add_text_done};
pub use picks::PickStore;

#[cfg(test)]
#[path = "../../../tests/backend/texts/storage.rs"]
mod tests;
