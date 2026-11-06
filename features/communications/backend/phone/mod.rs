//! Phone transport: paired devices, relayed texts, send attempts and attachment bytes.
//! `texts` owns conversation attribution; every text mutation is filed in the same write
//! through `texts::filing::changing`.
pub(crate) mod app;
pub(crate) mod demo;
pub(crate) mod files;
pub(crate) mod live;
pub(crate) mod model;
pub(crate) mod numbers;
pub(crate) mod outbox;
pub(crate) mod pairing;
pub(crate) mod storage;
pub(crate) mod tidy;

use dispatch_core::{Result, db::Store};
use model::Limits;
use storage::{Device, DeviceStore};

/// What one text may carry either way: as answering is held to, and a phone's reports. A
/// picture's most is as much as a phone sends in one text and more.
pub const LIMITS: Limits = Limits {
    body: 10_000,
    files: 10,
    largest: 20 * 1024 * 1024,
};

/// Notes the DSP's phone `device`, as read in this write, checked in at `at`, once `SEEN_MS` passed
/// since it last was: another call of the phone's alongside may have noted it since this one first
/// read it. Inside a write.
pub fn seen(db: &Store, dsp: &str, device: &Device, at: i64) -> Result<()> {
    if device.noting(at) {
        db.phones_seen(dsp, &device.id, at)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/phone/support.rs"]
pub(crate) mod support;
