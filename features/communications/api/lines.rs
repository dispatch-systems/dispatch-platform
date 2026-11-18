//! Another feature's line of the DSP's phones, as its own card on Settings' DSP Connections manages
//! it from routes of its own, behind its own permission: the line's phones, the code that pairs
//! its next, renaming one and removing it, by Dispatch Phone's rules and words, each seeing and
//! changing only that line's ([Lines](../README.md#dispatch-phones)); and its texting, as its own
//! page reads and answers it: its conversations, every one a number's, those waiting and what
//! changes in them, each opened, answered, tried again or marked done, the names it saved and the
//! numbers it blocked, texting a number first, its texts' pictures, and telling its members in a
//! browser (`Pushed`). Each sees and changes only that line's: another line's conversation, or a
//! Slack one, isn't found. Each is refused, as not found, while Dispatch Phone is off for the DSP:
//! its phones' address is Dispatch Phone's. Its page's display is a section of its own on Settings'
//! Display page, with Dispatch Phone's choices (`display`), and its writes change Dispatch Phone's
//! data (`DOMAIN`).
pub use super::routes::{
    conversations::{
        conversation_of as conversation, done_of as done, send_of as send, typing_of as typing,
        window_of as window,
    },
    inbox::{
        found_of as found, overview_of as overview, updates_of as updates, waiting_of as waiting,
    },
    phones::{
        listed_of as listed, new_pairing_of as new_pairing, pairing_status_of as pairing_status,
        remove_of as remove, rename_of as rename,
    },
    push::{key as push_key, subscribe_of as push_subscribe, unsubscribe_of as push_unsubscribe},
    texts::{
        add_text_photo_of as upload, block_of as block, fetch_text_file_of as fetch_file,
        retry_of as retry, save_contact_of as save_contact, text_contacts_of as contacts,
        text_file_of as file, text_file_preview_of as file_preview, text_first_of as text_first,
        unblock_of as unblock,
    },
};
pub use crate::backend::{
    display::display,
    phone::storage::Line,
    push::{Pushed, pushing},
    storage::DOMAIN,
};
