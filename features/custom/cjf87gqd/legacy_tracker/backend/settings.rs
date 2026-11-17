//! Legacy Tracker's settings, on its own page of a DSP's Settings, which draws them from this
//! declaration alone.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, section, text},
};
use serde::Deserialize;

/// The longest address kept.
const MAX_LINK: usize = 2000;

/// The tracker's address, for those holding `cjf87gqd_legacy_tracker.manage`. Its id and its
/// field's are permanent once shipped: each DSP's values are kept under them.
pub const GENERAL: Section = section(
    "cjf87gqd_legacy_tracker.general",
    "General",
    "cjf87gqd_legacy_tracker.manage",
    &[text("link", "Link", 0, MAX_LINK, "")],
);

/// `GENERAL`'s values, each field by its id: its default until the DSP saves one.
#[derive(Deserialize)]
pub struct General {
    /// The tracker's address as the DSP typed it; empty until it sets one.
    pub link: String,
}

/// The address the DSP's link opens: the one it set, while that is a secure web address. Anything
/// else typed there opens nothing, so the link never runs a script or opens an insecure page.
pub(crate) fn link(db: &Store, dsp: &str) -> Result<Option<String>> {
    let General { link } = dispatch_core::settings::read(db, dsp, &GENERAL)?;
    Ok(link.starts_with("https://").then_some(link))
}

#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
