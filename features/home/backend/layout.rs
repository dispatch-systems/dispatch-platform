//! A member's Home page layout, kept as their preference `home.layout` by core, which sends it
//! with their view of the DSP. The dashboard knows the widgets, so any id is kept; what is held
//! here is that each fits the grid, once.
use crate::api::types::HomeLayout;
use dispatch_core::{
    Result, ensure,
    foundation::validate as v,
    manifest::{Preference, PreferenceValue, preference},
};
use std::collections::BTreeSet;

pub const LAYOUT: Preference = preference::<HomeLayout>("home.layout");

/// The grid's columns.
const COLUMNS: u8 = 12;
/// The most widgets a layout places, far past what a page shows.
const MOST: usize = 100;
/// The tallest a widget is, and the lowest row one starts on.
const TALLEST: u8 = 24;
const LOWEST: u16 = 1000;

impl PreferenceValue for HomeLayout {
    fn check(&self) -> Result<()> {
        let ids: Vec<String> = self.widgets.iter().map(|each| each.id.clone()).collect();
        v::ids(&ids, MOST)?;
        let once = ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
        let fit = self.widgets.iter().all(|each| {
            (1..=COLUMNS).contains(&each.w)
                && each.x + each.w <= COLUMNS
                && (1..=TALLEST).contains(&each.h)
                && each.y <= LOWEST
        });
        ensure(once && fit, "invalid_input", 400)
    }
}

#[cfg(test)]
#[path = "../tests/backend/layout.rs"]
mod tests;
