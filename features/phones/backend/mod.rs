//! Phones' logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
use dispatch_phones_contract::Limits;

pub(crate) mod demo;
pub(crate) mod exposed;
pub(crate) mod files;
pub(crate) mod live;
pub(crate) mod pairing;
pub(crate) mod storage;
pub(crate) mod tidy;

#[cfg(test)]
#[path = "../tests/backend/support.rs"]
pub(crate) mod support;

/// What one text may carry either way: as `texting` tells other features and holds what they send
/// to, and as a phone's reports are held to. A picture's most is as much as a phone sends in one
/// text and more.
pub const LIMITS: Limits = Limits {
    body: 10_000,
    files: 10,
    largest: 20 * 1024 * 1024,
};

/// What an ID is, as its contract says (`dispatch_phones_contract::identifier`).
#[cfg(test)]
#[path = "../tests/backend/identifiers.rs"]
mod identifiers;
/// How Phones writes a number, as its contract offers it (`dispatch_phones_contract::normal`).
#[cfg(test)]
#[path = "../tests/backend/numbers.rs"]
mod numbers;
