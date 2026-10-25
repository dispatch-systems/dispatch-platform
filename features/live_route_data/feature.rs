//! Live Route Data: each DSP's routes read every five minutes inside a window it sets, kept as
//! today's live route data, and saved into historical route data as the window ends if it
//! chooses. Its README says more.
mod backend;

use dispatch_core::manifest::{Feature, feature, optional, perm};

pub const FEATURE: Feature = Feature {
    place: 140,
    // It reads the DSP's route source, Cortex, which switches on with it.
    switch: optional("live_route_data", "Live Route Data", &["routes"]),
    depends_on: &["routes", "cortex"],
    permissions: &[perm(
        "live_route_data.manage",
        "Manage Live Route Data",
        140,
    )],
    // When it reads, on its own page of Settings.
    settings: &[backend::settings::COLLECTION],
    live_readers: &[&backend::reader::READER],
    ..feature("live_route_data")
};
