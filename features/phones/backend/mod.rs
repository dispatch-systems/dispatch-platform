//! Phones' logic. It is private to the crate: `feature.rs` re-exports what the app or a
//! declared dependant uses.
pub(crate) mod demo;
pub(crate) mod exposed;
pub(crate) mod files;
pub(crate) mod live;
pub(crate) mod pairing;
pub(crate) mod storage;
pub(crate) mod tidy;

/// How Phones writes a number, as its contract offers it (`dispatch_phones_contract::normal`).
#[cfg(test)]
#[path = "../tests/backend/numbers.rs"]
mod numbers;
