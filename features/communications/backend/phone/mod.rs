//! The phone side: the DSP's dispatch phones, Android phones running Dispatch's app, each relaying
//! the texts and pictures its number receives and sends, and sending Dispatch's from that number.
//! It knows phones, the texts they relay, each attempt at Dispatch's and how it went, and their
//! bytes; never who a text is from or which conversation it's filed in, which `texts` keeps on the
//! same row of each text. Whoever changes a text here files it in the same write
//! (`texts::filing::changing`).
//!
//! - `model`: what it speaks in: phones, texts, their files, and what one text may carry;
//!   `numbers`, how it writes a number.
//! - `pairing`: the codes that pair a phone, and the key it's given.
//! - `outbox`: queuing a text Dispatch sends, or one that failed again, inside the write that
//!   checks it may go; `live`, waking the phones waiting on their outbox.
//! - `files`: the bytes of the texts' pictures and files, kept in the DSP's own data (`bytes`).
//! - `storage`: its phones, the codes that pair them, the texts and their files, in each DSP's
//!   own database.
//! - `tidy`: its upkeep; `demo`, a development DSP's stand-in phone.
//!
//! Its address, which the phones' app talks to, is `api::phone`'s; managing the DSP's phones,
//! `api::routes::phones`'.
pub(crate) mod demo;
pub(crate) mod files;
pub(crate) mod live;
pub(crate) mod model;
pub(crate) mod numbers;
pub(crate) mod outbox;
pub(crate) mod pairing;
pub(crate) mod storage;
pub(crate) mod tidy;

use dispatch_core::{
    Result,
    db::{self, Store},
};
use model::{Limits, Phone};
use storage::DeviceStore;

/// What one text may carry either way: as answering is held to, and a phone's reports. A
/// picture's most is as much as a phone sends in one text and more.
pub const LIMITS: Limits = Limits {
    body: 10_000,
    files: 10,
    largest: 20 * 1024 * 1024,
};

/// The DSP's phones, those removed left out, each with whether it's online.
pub fn phones(db: &Store, dsp: &str) -> Result<Vec<Phone>> {
    let now = db::now();
    Ok(db
        .phones_devices(dsp)?
        .iter()
        .map(|device| device.phone(now))
        .collect())
}

#[cfg(test)]
#[path = "../../tests/backend/phone/support.rs"]
pub(crate) mod support;
