//! What the registry refuses of what features declare about one another.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{
        Feature, Registry, ScheduleAlias, Switch, feature,
        people::{Appearances, Named, People},
        perm, sub,
    },
    mcp::api::types::{DriverData, DriverSource},
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
#[should_panic(expected = "loop.view grants itself through what it implies")]
fn a_permission_that_grants_itself_however_far_away_is_refused() {
    // Edit sits under view, a sub-feature's upload under edit, and view implies upload.
    static LOOP: Feature = Feature {
        switch: Some(Switch {
            id: "loop",
            label: "Loop",
            requires: &[],
        }),
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
    crate::install();
    with(&TWICE).check();
}

#[cfg(all(feature = "daily_performance", feature = "weekly_scorecard"))]
#[test]
#[should_panic(expected = "feedback repeats a source variant")]
fn a_tool_source_variant_declared_twice_is_refused() {
    use dispatch_core::mcp::pieces::Mcp;
    static AGAIN: Feature = Feature {
        mcp: Mcp {
            variants: &[dispatch_daily_performance::FEATURE.mcp.variants[0]],
            ..Mcp::NONE
        },
        ..feature("again")
    };
    crate::install();
    with(&AGAIN).check();
}
