//! The access catalog as it stood before the manifests declared it: pages, tabs, the
//! collections each page runs, schedule collections, and the role sheet's permissions with
//! their labels, implications, groups and defaults. Their order reaches the generated
//! TypeScript, API answers and audit entries, so it is held too. Each is a snapshot in
//! app/tests/backend/snapshots/, which a new feature or collector changes.
use crate::snapshot;
use dispatch_core::{
    collection::{api::types::ScheduleCollection, registry::Provider},
    tenancy::{catalog, roles},
};
use serde_json::json;

/// Names no collection or alias has: the schedules' page runs them.
const UNKNOWN: &[&str] = &["schedules", "unknown", ""];

#[cfg(feature = "default")]
#[test]
fn the_permissions_keep_their_order_labels_implications_groups_and_demo_roles() {
    crate::install();
    let ids: Vec<_> = roles::LABELS.iter().map(|(id, _)| *id).collect();
    assert_eq!(roles::PERMISSIONS.to_vec(), ids);
    assert_eq!(roles::all(), ids);
    snapshot::check(
        "catalog-permissions.json",
        &json!({
            "permissions": *roles::LABELS,
            "implied": *roles::IMPLIED,
            "groups": *roles::GROUPS,
            "demo": *roles::DEMO,
        }),
    );
}

#[cfg(feature = "default")]
#[test]
fn the_catalog_keeps_its_pages_tabs_and_order() {
    crate::install();
    let pages: Vec<_> = catalog::pages()
        .map(|page| {
            assert_eq!(page.kind, catalog::Kind::Page);
            assert!(page.provides.is_empty());
            json!({"id": page.id, "label": page.label, "permissions": page.permissions,
                "requires": page.requires, "default": page.default, "mandatory": page.mandatory})
        })
        .collect();
    let subs: Vec<_> = catalog::catalog()
        .iter()
        .filter(|feature| matches!(feature.kind, catalog::Kind::Sub(_)))
        .map(|sub| match sub.kind {
            catalog::Kind::Sub(page) => json!({"id": sub.id, "label": sub.label, "page": page,
                    "tab": sub.tab, "permissions": sub.permissions, "requires": sub.requires,
                    "default": sub.default, "mandatory": sub.mandatory}),
            kind => panic!("{} is a {kind:?}", sub.id),
        })
        .collect();
    let catalog: Vec<_> = catalog::catalog().iter().map(|f| f.id).collect();
    snapshot::check(
        "catalog-pages.json",
        &json!({"pages": pages, "subfeatures": subs, "catalog": catalog,
            "schedules": catalog::schedules()}),
    );
}

#[cfg(feature = "default")]
#[test]
fn each_collection_is_run_by_the_page_that_keeps_it() {
    crate::install();
    let kinds: Vec<_> = Provider::all().flat_map(|p| p.job_kinds()).collect();
    let named = kinds
        .iter()
        .copied()
        .chain(ScheduleCollection::all().map(ScheduleCollection::as_str))
        .chain(UNKNOWN.iter().copied());
    // What each job kind or schedule collection names: the page that runs it.
    let automation: Vec<_> = named
        .map(|named| (named, catalog::automation(named)))
        .collect();
    // The permission a job of each kind runs under.
    let permissions: Vec<_> = kinds
        .iter()
        .map(|kind| (*kind, catalog::collection_permission(kind)))
        .collect();
    let automates: Vec<_> = catalog::catalog()
        .iter()
        .map(|feature| feature.id)
        .filter(|id| catalog::automates(&[(*id).to_owned()]))
        .collect();
    assert!(!catalog::automates(&[]));
    snapshot::check(
        "catalog-collections.json",
        &json!({"automation": automation, "permissions": permissions, "automates": automates}),
    );
}

// `both` is Timecard's.
#[cfg(feature = "timecard")]
#[test]
fn schedules_collect_the_same_collections() {
    crate::install();
    let all: Vec<_> = ScheduleCollection::all()
        .map(ScheduleCollection::as_str)
        .collect();
    snapshot::check("schedule-collections.json", &json!(all));
    for collection in &all {
        let parsed = ScheduleCollection::parse(collection).unwrap();
        assert_eq!(parsed.as_str(), *collection);
    }
    for other in ["", "cortex", "timecard", "paycom.collect", "Both"] {
        assert!(ScheduleCollection::parse(other).is_none(), "{other}");
    }
}
