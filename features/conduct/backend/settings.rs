//! Conduct's settings, on its own page of a DSP's Settings: the categories, with their tiers'
//! step downs and wording, and whether signed copies are backed up, each drawn by Conduct's own
//! component.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, section, toggle},
    settings,
};
use serde::Deserialize;

/// The categories of writeup, with their starting tiers, step downs and wording: drawn by
/// Conduct's own component, saved through its own routes. Its id is permanent once shipped.
pub const CATEGORIES: Section = section("conduct.categories", "Categories", "conduct.manage", &[])
    .custom()
    .keywords(&[
        "writeups",
        "templates",
        "wording",
        "tiers",
        "reduce",
        "reset",
    ]);

/// Whether signed copies are backed up where the DSP keeps its files: drawn by Conduct's own
/// component, only while something keeps them, with whether the backup is failing. Its id and
/// its field's are permanent once shipped.
pub const BACKUP: Section = section(
    "conduct.backup",
    "Backup",
    "conduct.manage",
    &[toggle("signed_copies", "Back up signed copies", false)],
)
.custom()
.order(10)
.keywords(&["photos", "files", "folder"]);

/// `BACKUP`'s values, as Conduct reads them.
#[derive(Deserialize)]
pub struct Backup {
    pub signed_copies: bool,
}

/// Whether the DSP backs its signed copies up.
pub(crate) fn backs_up(db: &Store, dsp: &str) -> Result<bool> {
    Ok(settings::read::<Backup>(db, dsp, &BACKUP)?.signed_copies)
}
