//! Feature switches over the catalog of every registered feature's pages and tabs.
use dispatch_core::{db::Store, platform_owner::api::types::DspFeatures, tenancy::catalog::*};

/// What a DSP has that it could be without: its features less the mandatory ones.
fn switched(db: &Store, dsp: &str) -> Vec<String> {
    db.features(dsp)
        .unwrap()
        .into_iter()
        .filter(|id| !find(id).is_some_and(|f| f.mandatory))
        .collect()
}
/// What every DSP has.
fn mandatory() -> Vec<String> {
    catalog()
        .iter()
        .filter(|f| f.mandatory)
        .map(|f| f.id.to_owned())
        .collect()
}

#[test]
fn a_new_dsp_starts_with_only_what_every_dsp_has_and_a_demo_dsp_with_all() {
    use std::os::unix::fs::PermissionsExt;
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let owner = db
        .create_user(
            "owner@example.test",
            "Platform",
            "Owner",
            "Features-test-2026!",
            true,
        )
        .unwrap();
    let dsp = db.new_dsp("New DSP", "UTC", &owner.id, false).unwrap();
    assert_eq!(db.features(&dsp.id).unwrap(), mandatory());
    assert!(
        ["home", "team", "settings"]
            .iter()
            .all(|id| mandatory().contains(&(*id).to_owned()))
    );
    // Nobody switches what every DSP has.
    let refused = db
        .set_feature(&dsp.id, "home", false, &owner.id)
        .unwrap_err();
    assert_eq!(
        (refused.code.as_str(), refused.status),
        ("feature_mandatory", 409)
    );
    // Every role but the owner's starts with no permission: the DSP's owner turns them on.
    let roles = db.roles(&dsp.id).unwrap();
    assert_eq!(
        roles
            .iter()
            .map(|role| role.name.as_str())
            .collect::<Vec<_>>(),
        ["Owner", "Manager", "Member"]
    );
    assert!(
        roles
            .iter()
            .filter(|role| !role.owner)
            .all(|role| role.permissions.is_empty())
    );
    db.enable_all_features(&dsp.id).unwrap();
    let all: Vec<_> = catalog().iter().map(|f| f.id.to_owned()).collect();
    assert_eq!(db.features(&dsp.id).unwrap(), all);
}
#[cfg(feature = "dvic")]
#[test]
fn a_tab_follows_its_page_and_the_last_one_takes_the_page() {
    use std::os::unix::fs::PermissionsExt;
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let owner = db
        .create_user(
            "owner@example.test",
            "Platform",
            "Owner",
            "Tabs-test-2026!",
            true,
        )
        .unwrap();
    let dsp = db.new_dsp("Tabs DSP", "UTC", &owner.id, false).unwrap().id;
    let changes = |r: DspFeatures| -> Vec<(String, bool)> {
        r.changed
            .into_iter()
            .map(|c| (c.feature, c.enabled))
            .collect()
    };
    let on = |id: &str| (id.to_owned(), true);
    let off = |id: &str| (id.to_owned(), false);
    // A new DSP's tabs are switched on, but none exists before its page does.
    let dvic = db.set_feature(&dsp, "dvic", true, &owner.id).unwrap();
    assert_eq!(changes(dvic), [on("cortex"), on("dvic")]);
    assert_eq!(
        switched(&db, &dsp),
        ["dvic", "dvic.day", "dvic.week", "cortex"]
    );
    // One tab goes alone; the last takes its page, which keeps the tab's switch.
    let day = db.set_feature(&dsp, "dvic.day", false, &owner.id).unwrap();
    assert_eq!(changes(day), [off("dvic.day")]);
    let week = db.set_feature(&dsp, "dvic.week", false, &owner.id).unwrap();
    assert_eq!(changes(week), [off("dvic.week"), off("dvic")]);
    assert_eq!(switched(&db, &dsp), ["cortex"]);
    // A page switched on without a tab brings every tab back.
    let back = db.set_feature(&dsp, "dvic", true, &owner.id).unwrap();
    assert_eq!(changes(back), [on("dvic"), on("dvic.day"), on("dvic.week")]);
    // A page switched off keeps its tabs as they were, and switching it on finds them.
    db.set_feature(&dsp, "dvic.week", false, &owner.id).unwrap();
    db.set_feature(&dsp, "dvic", false, &owner.id).unwrap();
    assert_eq!(switched(&db, &dsp), ["cortex"]);
    let report = db.feature_report(&dsp).unwrap().features;
    let state = |id: &str| report.iter().find(|s| s.feature == id).unwrap().enabled;
    assert!(state("dvic.day") && !state("dvic.week"));
    let again = db.set_feature(&dsp, "dvic", true, &owner.id).unwrap();
    assert_eq!(changes(again), [on("dvic")]);
    assert_eq!(switched(&db, &dsp), ["dvic", "dvic.day", "cortex"]);
}

#[cfg(feature = "timecard")]
#[test]
fn a_part_needs_a_connection_of_its_own_beside_its_pages() {
    use std::os::unix::fs::PermissionsExt;
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let owner = db
        .create_user(
            "owner@example.test",
            "Platform",
            "Owner",
            "Parts-test-2026!",
            true,
        )
        .unwrap();
    let dsp = db.new_dsp("Parts DSP", "UTC", &owner.id, false).unwrap().id;
    let changes = |r: DspFeatures| -> Vec<(String, bool)> {
        r.changed
            .into_iter()
            .map(|c| (c.feature, c.enabled))
            .collect()
    };
    let on = |id: &str| (id.to_owned(), true);
    let off = |id: &str| (id.to_owned(), false);
    // Timecard needs Paycom's timecards; its Meal Breaks tab, Cortex's meal breaks too.
    let timecard = db.set_feature(&dsp, "timecard", true, &owner.id).unwrap();
    assert_eq!(
        changes(timecard),
        [on("paycom"), on("timecard"), on("cortex")]
    );
    // Without Cortex, only the tab that needs it goes.
    let cortex = db.set_feature(&dsp, "cortex", false, &owner.id).unwrap();
    assert_eq!(
        changes(cortex),
        [off("cortex"), off("timecard.meal_breaks")]
    );
    assert!(db.feature_enabled(&dsp, "timecard").unwrap());
    // And the tab brings Cortex back.
    let tab = db
        .set_feature(&dsp, "timecard.meal_breaks", true, &owner.id)
        .unwrap();
    assert_eq!(changes(tab), [on("cortex"), on("timecard.meal_breaks")]);
}

#[test]
fn a_feature_made_optional_stays_on_for_every_dsp_that_had_it() {
    use std::os::unix::fs::PermissionsExt;
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let owner = db
        .create_user(
            "owner@example.test",
            "Platform",
            "Owner",
            "Kept-test-2026!",
            true,
        )
        .unwrap();
    let dsp = db.new_dsp("Kept DSP", "UTC", &owner.id, false).unwrap().id;
    let optional = catalog()
        .iter()
        .find(|f| f.kind == Kind::Page && !f.mandatory)
        .unwrap()
        .id;
    assert!(!db.feature_enabled(&dsp, optional).unwrap());
    // As the last start found it, every DSP had it; now the platform owner switches it.
    db.platform
        .exec(
            "UPDATE feature_availability SET mandatory=1 WHERE feature=?",
            [optional],
        )
        .unwrap();
    db.keep_mandatory_features().unwrap();
    assert!(db.feature_enabled(&dsp, optional).unwrap());
    // Once: a DSP that switches it off later keeps it off.
    db.set_feature(&dsp, optional, false, &owner.id).unwrap();
    db.keep_mandatory_features().unwrap();
    assert!(!db.feature_enabled(&dsp, optional).unwrap());
}
#[test]
fn the_catalog_is_consistent() {
    crate::install();
    let all = catalog();
    let mut ids: Vec<_> = all.iter().map(|f| f.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "feature ids repeat");
    let mut owned = Vec::new();
    for feature in all {
        for permission in feature.permissions {
            assert!(
                dispatch_core::tenancy::roles::PERMISSIONS.contains(permission),
                "{permission} is not a permission"
            );
            assert!(!owned.contains(permission), "{permission} has two features");
            owned.push(*permission);
        }
        match feature.kind {
            Kind::Page => {
                assert!(feature.provides.is_empty());
                // Every DSP has a mandatory one, connected or not.
                assert!(!feature.mandatory || (feature.default && feature.requires.is_empty()));
            }
            Kind::Sub(page) => {
                let page = find(page).filter(|p| p.kind == Kind::Page).unwrap();
                assert!(feature.provides.is_empty());
                // Only a mandatory feature's parts may be, and need no connection; an
                // optional feature's start on with it, a mandatory one's off.
                assert!(!feature.mandatory || (page.mandatory && feature.requires.is_empty()));
                assert_eq!(feature.default, feature.mandatory || !page.mandatory);
            }
            Kind::Connection => {
                assert!(!feature.provides.is_empty() && feature.requires.is_empty())
            }
        }
        for capability in feature.requires {
            assert!(
                all.iter().any(|f| f.provides.contains(capability)),
                "nothing provides {capability}"
            );
        }
    }
    assert!(!owned.contains(&CONNECTIONS));
    assert!(find(schedules()).is_some_and(|f| f.kind == Kind::Page));
}
#[cfg(feature = "uniforms")]
#[test]
fn permissions_follow_their_feature() {
    crate::install();
    let on = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(grants(&on(&["uniforms"]), "uniforms.view"));
    assert!(!grants(&on(&["timecard"]), "uniforms.view"));
    assert!(grants(&on(&[]), "members.invite"));
    assert!(grants(&on(&["cortex"]), "connections.manage"));
    assert!(!grants(
        &on(&["timecard", "uniforms"]),
        "connections.manage"
    ));
    let stored = on(&["uniforms.view", "roles.manage", "timecard.view"]);
    let enabled = on(&["timecard"]);
    let seen: Vec<_> = visible(&enabled, &stored).collect();
    assert_eq!(
        seen,
        [&"roles.manage".to_owned(), &"timecard.view".to_owned()]
    );
}
