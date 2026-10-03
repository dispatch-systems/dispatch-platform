//! What the registry refuses of what features declare about one another.
use crate::{
    Result,
    contracts::{DriverData, DriverSource},
    db::Store,
    manifest::{
        Feature, Registry, feature,
        people::{Appearances, Named, People},
    },
};

/// The app's registry with one more feature, as `install` would check it.
fn with(extra: &'static Feature) -> Registry {
    let features: Vec<&'static Feature> = crate::REGISTRY
        .features
        .iter()
        .copied()
        .chain([extra])
        .collect();
    Registry {
        collectors: crate::REGISTRY.collectors,
        features: Box::leak(features.into_boxed_slice()),
    }
}

#[test]
#[should_panic(expected = "lonely depends on elsewhere, which is not registered")]
fn a_feature_that_depends_on_one_not_registered_is_refused() {
    static LONELY: Feature = Feature {
        depends_on: &["elsewhere"],
        ..feature("lonely")
    };
    with(&LONELY).check();
}

/// Routes' drivers, named a second time.
struct Again;
impl People for Again {
    fn data(&self) -> DriverData {
        DriverData::Routes
    }
    fn source(&self) -> DriverSource {
        DriverSource::Amazon
    }
    fn order(&self) -> u16 {
        99
    }
    fn named(&self, _: &Store, _: &str) -> Result<Vec<Named>> {
        Ok(vec![])
    }
    fn name(&self, _: &Store, _: &str, _: &str) -> Result<Option<String>> {
        Ok(None)
    }
    fn appearances(&self, _: &Store, _: &str) -> Result<Vec<Appearances>> {
        Ok(vec![])
    }
}

#[test]
#[should_panic(expected = "routes names people twice, or in another's place")]
fn a_kind_of_data_that_names_people_twice_is_refused() {
    static TWICE: Feature = Feature {
        people: &[&Again],
        ..feature("twice")
    };
    with(&TWICE).check();
}
