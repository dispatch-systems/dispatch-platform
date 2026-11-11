//! Weekly Scorecards from Cortex's performance API: the datasets behind each scorecard
//! page, one week at a time, published into the DSP's scorecard database. Every row
//! Amazon sends is kept as JSON beside the keys reads filter on. Browser and HTTP
//! data is untrusted input.
pub(crate) mod exposed;
pub(crate) mod keeper;
pub(crate) mod people;
pub(crate) mod publication;
pub(crate) mod reads;
pub(crate) mod settings;
pub(crate) mod storage;

pub use storage::{DATABASE, WeeklyScorecardStore};

#[cfg(test)]
#[path = "../tests/backend/mod.rs"]
mod tests;
