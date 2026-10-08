//! Home: a DSP's home page, still a placeholder in the frontend.
use dispatch_core::manifest::{Feature, feature};

pub const FEATURE: Feature = Feature {
    place: 10,
    ..feature("home")
};
