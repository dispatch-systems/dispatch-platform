//! Conduct's settings, on its own page of a DSP's Settings: how long a writeup counts toward the
//! next tier, which Settings draws from this declaration alone, and the categories, which
//! Conduct's own component draws.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, section},
};
use serde::Deserialize;

/// How long a writeup counts toward the next tier in its category. Its id and its fields' are
/// permanent once shipped: each DSP's values are kept under them.
pub const GENERAL: Section = section(
    "conduct.general",
    "Tiers",
    "conduct.manage",
    &[
        number("months", "Counts toward next tier for", 1, 36, Some(12))
            .unit("months")
            .presets(&[
                (3, "3 months"),
                (6, "6 months"),
                (12, "12 months"),
                (24, "24 months"),
            ]),
    ],
)
.keywords(&["writeups", "escalation", "tiers"]);

/// The categories of writeup, with their starting tiers and wording: drawn by Conduct's own
/// component, saved through its own routes.
pub const CATEGORIES: Section = section("conduct.categories", "Categories", "conduct.manage", &[])
    .custom()
    .order(10)
    .keywords(&["writeups", "templates", "wording"]);

/// `GENERAL`'s values, each field by its id: its default until the DSP saves one.
#[derive(Deserialize)]
pub struct General {
    /// How many months a writeup counts toward the next tier.
    pub months: i64,
}

/// The DSP's `GENERAL`, as Conduct reads it: one keyed lookup.
pub(crate) fn general(db: &Store, dsp: &str) -> Result<General> {
    dispatch_core::settings::read(db, dsp, &GENERAL)
}

#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
