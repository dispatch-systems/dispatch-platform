//! What the registry refuses of what features declare about one another.
use crate::manifest::{Feature, Registry, feature};

#[test]
#[should_panic(expected = "lonely depends on elsewhere, which is not registered")]
fn a_feature_that_depends_on_one_not_registered_is_refused() {
    static LONELY: Feature = Feature {
        depends_on: &["elsewhere"],
        ..feature("lonely")
    };
    let features: Vec<&'static Feature> = crate::REGISTRY
        .features
        .iter()
        .copied()
        .chain([&LONELY])
        .collect();
    Registry {
        collectors: crate::REGISTRY.collectors,
        features: Box::leak(features.into_boxed_slice()),
    }
    .check();
}
