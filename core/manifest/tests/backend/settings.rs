//! What the settings slot refuses of a feature's sections, as the registry is installed.
use super::*;
use crate::manifest::{choice, feature, optional, perm, toggle};

/// A registry of `feature` alone, beside core's own sections.
fn checked(feature: &'static Feature) {
    check(&Registry {
        collectors: &[],
        features: Box::leak(Box::new([feature])),
        agents: None,
    });
}
const FIELDS: &[Field] = &[toggle("enabled", "On", true)];

#[test]
fn aliases_cannot_claim_another_setting_owner_or_member_key() {
    let declared = |aliases| {
        Box::leak(Box::new(Feature {
            switch: optional("x", "X", &[]),
            settings: Box::leak(Box::new([
                section("x.policy", "Policy", "access", FIELDS).aliases(aliases)
            ])),
            ..feature("x")
        }))
    };
    checked(declared(&["next_x.policy"]));
    for aliases in [
        &["x.policy"][..],
        &["accounts.theme"][..],
        &["x.policy:member"][..],
        &["next_x.policy", "next_x.policy"][..],
    ] {
        assert!(std::panic::catch_unwind(|| checked(declared(aliases))).is_err());
    }
}

#[test]
#[should_panic(expected = "x's section x.policy is under y.manage, which it doesn't declare")]
fn a_section_under_another_features_permission_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        permissions: &[perm("x.manage", "Manage X", 1)],
        settings: &[section("x.policy", "Policy", "y.manage", FIELDS)],
        ..feature("x")
    };
    checked(&X);
}

#[test]
#[should_panic(expected = "x's section policy isn't named <feature>.<what>")]
fn a_section_not_named_under_its_feature_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        settings: &[section("policy", "Policy", "access", FIELDS)],
        ..feature("x")
    };
    checked(&X);
}

#[test]
#[should_panic(expected = "x's section x.a:b isn't named <owner>.<what> with its what snake_case")]
fn a_section_whose_id_could_read_as_a_members_own_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        settings: &[section("x.a:b", "Policy", "access", FIELDS)],
        ..feature("x")
    };
    checked(&X);
}

#[test]
#[should_panic(expected = "x's section x.empty has no fields, so it must be custom")]
fn a_section_with_no_fields_that_its_owner_does_not_draw_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        settings: &[section("x.empty", "Empty", "access", &[])],
        ..feature("x")
    };
    checked(&X);
}

#[test]
#[should_panic(expected = "x.policy's field mode: its default \"draft\" isn't a value it holds")]
fn a_default_its_field_does_not_hold_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        settings: &[section(
            "x.policy",
            "Policy",
            "access",
            &[choice("mode", "Mode", &[("final", "Final")], "draft")],
        )],
        ..feature("x")
    };
    checked(&X);
}

#[test]
#[should_panic(
    expected = "x's section x.policy goes with routes, which isn't a collection it keeps under x.manage"
)]
fn a_section_with_a_collection_its_feature_does_not_keep_is_refused() {
    static X: Feature = Feature {
        switch: optional("x", "X", &[]),
        permissions: &[perm("x.manage", "Manage X", 1)],
        settings: &[section("x.policy", "Policy", "x.manage", FIELDS).collection("routes")],
        ..feature("x")
    };
    checked(&X);
}
