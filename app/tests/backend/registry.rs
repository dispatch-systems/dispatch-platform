//! What the registry refuses of what features declare about one another.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{
        Feature, Registry, ScheduleAlias, feature, optional,
        people::{Appearances, DriverData, DriverSource, Named, People, PeopleData},
        perm, sub,
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
        agents: crate::REGISTRY.agents,
    }
}

#[test]
#[should_panic(expected = "loop.view grants itself through what it implies")]
fn a_permission_that_grants_itself_however_far_away_is_refused() {
    // Edit sits under view, a sub-feature's upload under edit, and view implies upload.
    static LOOP: Feature = Feature {
        switch: optional("loop", "Loop", &[]),
        permissions: &[
            perm("loop.view", "View", 9000).implies(&["loop.upload"]),
            perm("loop.edit", "Edit", 9001).under("loop.view"),
        ],
        subfeatures: &[sub("loop.uploads", "Uploads").permissions(&[perm(
            "loop.upload",
            "Upload",
            9002,
        )
        .under("loop.edit")])],
        ..feature("loop")
    };
    crate::install();
    with(&LOOP).check();
}

#[test]
#[should_panic(expected = "lonely depends on elsewhere, which is not registered")]
fn a_feature_that_depends_on_one_not_registered_is_refused() {
    static LONELY: Feature = Feature {
        switch: optional("lonely", "Lonely", &[]),
        depends_on: &["elsewhere"],
        ..feature("lonely")
    };
    crate::install();
    with(&LONELY).check();
}

#[test]
#[should_panic(expected = "foreign retires an identifier into one it does not own")]
fn a_retirement_cannot_rewrite_another_features_identifiers() {
    static FOREIGN: Feature = Feature {
        switch: optional("foreign", "Foreign", &[]),
        retired_identifiers: &[("prior.view", "timecard.view")],
        ..feature("foreign")
    };
    crate::install();
    with(&FOREIGN).check();
}

#[test]
#[should_panic(
    expected = "greedy's everything schedule runs cortex.dvic.collect, which it does not keep"
)]
fn a_schedule_alias_of_a_collection_its_feature_does_not_keep_is_refused() {
    static GREEDY: Feature = Feature {
        switch: optional("greedy", "Greedy", &[]),
        schedule_aliases: &[ScheduleAlias {
            schedule: "everything",
            runs: &["cortex.dvic.collect"],
        }],
        ..feature("greedy")
    };
    crate::install();
    with(&GREEDY).check();
}

/// Routes' drivers, named a second time.
struct Again;
const AGAIN: PeopleData = PeopleData {
    id: "routes",
    order: 99,
};
impl People for Again {
    fn data(&self) -> DriverData {
        DriverData::new(&AGAIN)
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
        switch: optional("twice", "Twice", &[]),
        people: &[&Again],
        ..feature("twice")
    };
    crate::install();
    with(&TWICE).check();
}

#[test]
#[should_panic(expected = "undecided says neither whether it is mandatory nor optional")]
fn a_feature_that_says_neither_mandatory_nor_optional_is_refused() {
    static UNDECIDED: Feature = feature("undecided");
    crate::install();
    with(&UNDECIDED).check();
}

#[test]
#[should_panic(
    expected = "halfway is optional, so its parts are too: halfway.always can't be mandatory"
)]
fn an_optional_feature_with_a_mandatory_part_is_refused() {
    static HALFWAY: Feature = Feature {
        switch: optional("halfway", "Halfway", &[]),
        subfeatures: &[sub("halfway.always", "Always").mandatory()],
        ..feature("halfway")
    };
    crate::install();
    with(&HALFWAY).check();
}
