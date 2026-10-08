//! Home: a DSP's home page, still a placeholder in the frontend.
use dispatch_core::manifest::{Feature, feature, mandatory};

pub const FEATURE: Feature = Feature {
    place: 10,
    switch: mandatory("home", "Home Page"),
    ..feature("home")
};
