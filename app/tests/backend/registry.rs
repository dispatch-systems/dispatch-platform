//! What the registry refuses of what features declare about one another.
use dispatch_core::manifest::{
    Feature, Registry, ScheduleAlias, feature, optional,
    people::{KindInfo, People, Source, SourceInfo},
    perm, sub,
};

/// The app's registry with one more feature, as `install` would check it.
fn with(extra: &'static Feature) -> Registry {
    with_all(&[extra])
}

/// The app's registry with more features, as `install` would check it.
fn with_all(extra: &[&'static Feature]) -> Registry {
    let features: Vec<&'static Feature> = crate::REGISTRY
        .features
        .iter()
        .chain(extra)
        .copied()
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
const AGAIN: People = People::new(
    KindInfo {
        id: "routes",
        order: 99,
        label: "Routes",
        icon: "Route",
    },
    dispatch_cortex::SOURCE,
    |_, _| Ok(vec![]),
);

#[test]
#[should_panic(expected = "routes names people twice")]
fn a_kind_of_data_that_names_people_twice_is_refused() {
    static TWICE: Feature = Feature {
        switch: optional("twice", "Twice", &[]),
        people: &[&AGAIN],
        ..feature("twice")
    };
    crate::install();
    with(&TWICE).check();
}

/// Amazon's transporter IDs, declared a second time under another name.
const ELSEWHERE: People = People::new(
    KindInfo {
        id: "elsewhere",
        order: 99,
        label: "Elsewhere",
        icon: "Route",
    },
    Source::new(&SourceInfo {
        id: "amazon",
        label: "Amazon elsewhere",
        order: 20,
    }),
    |_, _| Ok(vec![]),
);

#[test]
#[should_panic(expected = "the source amazon is declared twice")]
fn a_source_declared_twice_is_refused() {
    static TWICE: Feature = Feature {
        switch: optional("elsewhere", "Elsewhere", &[]),
        people: &[&ELSEWHERE],
        ..feature("elsewhere")
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

/// A contract no feature of the app's offers.
trait Greeting: Send + Sync {}
struct Hello;
impl Greeting for Hello {}

#[test]
#[should_panic(expected = "first and second each offer the same contract; one feature at most may")]
fn a_contract_two_features_offer_is_refused() {
    static FIRST: Feature = Feature {
        switch: optional("first", "First", &[]),
        exposes: &[dispatch_core::expose!(dyn Greeting, &Hello, "greeting", "A greeting.")],
        ..feature("first")
    };
    static SECOND: Feature = Feature {
        switch: optional("second", "Second", &[]),
        exposes: &[dispatch_core::expose!(dyn Greeting, &Hello, "welcome", "A welcome.")],
        ..feature("second")
    };
    crate::install();
    with_all(&[&FIRST, &SECOND]).check();
}

/// Another contract no feature of the app's offers.
trait Farewell: Send + Sync {}
impl Farewell for Hello {}

#[test]
#[should_panic(expected = "first and second each expose greeting; one feature at most may")]
fn a_name_two_features_expose_is_refused() {
    static FIRST: Feature = Feature {
        switch: optional("first", "First", &[]),
        exposes: &[dispatch_core::expose!(dyn Greeting, &Hello, "greeting", "A greeting.")],
        ..feature("first")
    };
    static SECOND: Feature = Feature {
        switch: optional("second", "Second", &[]),
        exposes: &[dispatch_core::expose!(dyn Farewell, &Hello, "greeting", "A farewell.")],
        ..feature("second")
    };
    crate::install();
    with_all(&[&FIRST, &SECOND]).check();
}

#[test]
fn a_contract_is_found_by_its_type_in_the_feature_offering_it() {
    static OFFERING: Feature = Feature {
        switch: optional("offering", "Offering", &[]),
        exposes: &[dispatch_core::expose!(dyn Greeting, &Hello, "greeting", "A greeting.")],
        ..feature("offering")
    };
    crate::install();
    let registry = with(&OFFERING);
    registry.check();
    let (feature, _) = registry
        .exposed::<dyn Greeting>()
        .expect("Offering exposes it");
    assert_eq!(feature.name, "offering");
    assert!(registry.exposed::<dyn Farewell>().is_none());
}
