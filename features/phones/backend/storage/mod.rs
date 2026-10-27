//! Phones' storage, in each DSP's own database, by concern: its phones and the code that pairs the
//! next one (`devices`), every text they relay and how Dispatch's went (`texts`), and the texts'
//! pictures and files (`files`). Only this crate writes SQL against its tables.
mod devices;
mod files;
mod texts;

pub(crate) use devices::add_pairing_ids;
pub use devices::{Device, DeviceStore, PAIRING_MS, Pairing};
pub use files::{FileStore, ReportedFile};
pub use texts::{Kept, MOST_TEXTS, Reported, StatusReport, TextStore};

use dispatch_core::{
    Error, Result,
    db::{Db, n},
    server::cache::DataDomain,
};

/// What Phones' writes change, for the read cache.
pub const DOMAIN: DataDomain = DataDomain::new("phones");

/// The next change, after every one given before it.
fn next_change(db: &Db) -> Result<i64> {
    let row = db
        .one(
            "UPDATE phones_change SET last=last+1 WHERE id=1 RETURNING last",
            [],
        )?
        .ok_or_else(|| Error::new("phones_change_missing", 500))?;
    Ok(n(&row, "last"))
}
