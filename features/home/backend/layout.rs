//! A member's Home page layout, kept as their preference `home.layout` by core, which sends it
//! with their view of the DSP. The dashboard knows the widgets, so any id is kept; what is held
//! here is that each fits the grid, once: twelve columns by seven rows, which the page fills.
use crate::api::types::HomeLayout;
use dispatch_core::{
    Result, ensure,
    foundation::validate as v,
    manifest::{Preference, PreferenceValue, preference},
};
use std::collections::BTreeSet;

pub const LAYOUT: Preference = preference::<HomeLayout>("home.layout");

/// The grid's columns and rows, which the page fills without scrolling.
const COLUMNS: u32 = 12;
const ROWS: u32 = 7;
/// The most widgets a layout places, past what the grid holds.
const MOST: usize = 100;

impl PreferenceValue for HomeLayout {
    fn check(&self) -> Result<()> {
        let ids: Vec<String> = self.widgets.iter().map(|each| each.id.clone()).collect();
        v::ids(&ids, MOST)?;
        let once = ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
        let fit = self.widgets.iter().all(|each| {
            let [x, y, w, h]: [u32; 4] =
                [each.x.into(), each.y.into(), each.w.into(), each.h.into()];
            w >= 1 && x + w <= COLUMNS && h >= 1 && y + h <= ROWS
        });
        ensure(once && fit, "invalid_input", 400)
    }
}

#[cfg(test)]
#[path = "../tests/backend/layout.rs"]
mod tests;
