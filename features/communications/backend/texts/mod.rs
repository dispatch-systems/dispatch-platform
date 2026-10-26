//! Texting: the texts the DSP's dispatch phones relay, through whatever offers it `texting`
//! (Phones), read by the page beside the Slack conversations and answered by text. A work phone's
//! texts are a driver's, across days, which Today's Routes will show; any other number's are
//! Messaging's: the number's conversation, with whoever Driver Match has the number for, else the
//! name the DSP saved for it, else the number itself.
//!
//! - `storage`: conversations by text, each text filed in one, how far Phones' changes were read,
//!   and the names and blocks the DSP keeps for numbers.
//! - `filing`: filing each text Phones relays in its conversation, and today's again as who held
//!   the work phones settles.
//! - `numbers`: one number written one way, how one reads, and what the DSP keeps of them.
//! - `reading`: the conversations and their texts, as the page reads them.
//! - `reply`: answering by text, and sending again one that failed.
//! - `contacts`: whom the DSP can text, and what someone at the DSP does with a number: saving a
//!   name for it, blocking it, texting it first.
pub(crate) mod contacts;
pub(crate) mod filing;
pub(crate) mod numbers;
pub(crate) mod reading;
pub(crate) mod reply;
pub(crate) mod storage;

use dispatch_core::{Error, Result, db::Store};
use dispatch_phones_contract::{self as texting, Texting};

/// Whether the DSP texts: something offers it `texting`.
pub fn offered(db: &Store, dsp: &str) -> Result<bool> {
    Ok(texting::for_dsp(db, dsp)?.is_some())
}
/// What offers the DSP texting, refused while nothing does.
fn offering(db: &Store, dsp: &str) -> Result<&'static dyn Texting> {
    texting::for_dsp(db, dsp)?.ok_or_else(|| Error::new("communications_texting_off", 409))
}
