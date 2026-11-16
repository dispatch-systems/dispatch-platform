//! Another feature's line of the DSP's phones, as its own card on Settings' DSP Connections manages
//! it from routes of its own, behind its own permission: the line's phones, the code that pairs
//! its next, renaming one and removing it, by Dispatch Phone's rules and words, each seeing and
//! changing only that line's ([Lines](../README.md#dispatch-phones)). Each is refused, as not
//! found, while Dispatch Phone is off for the DSP: its phones' address is Dispatch Phone's.
pub use super::routes::phones::{
    listed_of as listed, new_pairing_of as new_pairing, pairing_status_of as pairing_status,
    remove_of as remove, rename_of as rename,
};
pub use crate::backend::phone::storage::Line;
