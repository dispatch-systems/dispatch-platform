use super::*;
#[test]
fn a_new_dsp_starts_with_no_features_and_a_demo_dsp_with_all() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = super::super::config::Config::load().unwrap();
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
    assert!(db.features(&dsp.id).unwrap().is_empty());
    db.enable_all_features(&dsp.id).unwrap();
    let all: Vec<_> = catalog().iter().map(|f| f.id.to_owned()).collect();
    assert_eq!(db.features(&dsp.id).unwrap(), all);
}
#[test]
fn a_tab_follows_its_page_and_the_last_one_takes_the_page() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = super::super::config::Config::load().unwrap();
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
        db.features(&dsp).unwrap(),
        ["dvic", "dvic.day", "dvic.week", "cortex"]
    );
    // One tab goes alone; the last takes its page, which keeps the tab's switch.
    let day = db.set_feature(&dsp, "dvic.day", false, &owner.id).unwrap();
    assert_eq!(changes(day), [off("dvic.day")]);
    let week = db.set_feature(&dsp, "dvic.week", false, &owner.id).unwrap();
    assert_eq!(changes(week), [off("dvic.week"), off("dvic")]);
    assert_eq!(db.features(&dsp).unwrap(), ["cortex"]);
    // A page switched on without a tab brings every tab back.
    let back = db.set_feature(&dsp, "dvic", true, &owner.id).unwrap();
    assert_eq!(changes(back), [on("dvic"), on("dvic.day"), on("dvic.week")]);
    // A page switched off keeps its tabs as they were, and switching it on finds them.
    db.set_feature(&dsp, "dvic.week", false, &owner.id).unwrap();
    db.set_feature(&dsp, "dvic", false, &owner.id).unwrap();
    assert_eq!(db.features(&dsp).unwrap(), ["cortex"]);
    let report = db.feature_report(&dsp).unwrap().features;
    let state = |id: &str| report.iter().find(|s| s.feature == id).unwrap().enabled;
    assert!(state("dvic.day") && !state("dvic.week"));
    let again = db.set_feature(&dsp, "dvic", true, &owner.id).unwrap();
    assert_eq!(changes(again), [on("dvic")]);
    assert_eq!(db.features(&dsp).unwrap(), ["dvic", "dvic.day", "cortex"]);
}
#[test]
fn the_catalog_is_consistent() {
    let all = catalog();
    let mut ids: Vec<_> = all.iter().map(|f| f.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "feature ids repeat");
    let mut owned = Vec::new();
    for feature in all {
        for permission in feature.permissions {
            assert!(
                super::super::roles::PERMISSIONS.contains(permission),
                "{permission} is not a permission"
            );
            assert!(!owned.contains(permission), "{permission} has two features");
            owned.push(*permission);
        }
        match feature.kind {
            Kind::Page => assert!(feature.provides.is_empty()),
            Kind::Tab(page) => {
                assert!(find(page).is_some_and(|p| p.kind == Kind::Page));
                assert!(feature.permissions.is_empty() && feature.provides.is_empty());
                assert!(feature.requires.is_empty() && feature.default);
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
#[test]
fn permissions_follow_their_feature() {
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
