//! Home: a DSP's home page, which each member makes their own from the widgets the features
//! supply, placed where they choose on its grid.
mod api;
mod backend;

use dispatch_core::manifest::{Feature, feature, mandatory};

pub const FEATURE: Feature = Feature {
    place: 10,
    switch: mandatory("home", "Home Page"),
    preferences: &[backend::layout::LAYOUT],
    ..feature("home")
};

#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, api::types::HomeLayout, api::types::PlacedWidget)
}
