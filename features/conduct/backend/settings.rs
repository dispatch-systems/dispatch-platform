//! Conduct's settings, on its own page of a DSP's Settings: the categories, with their tiers'
//! step downs and wording, which Conduct's own component draws.
use dispatch_core::manifest::{Section, section};

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
