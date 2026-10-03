//! The access catalog as it stood before the manifests declared it: pages, tabs, the
//! collections each page runs, schedule collections, and the role sheet's permissions with
//! their labels, implications, groups and defaults. Their order reaches the generated
//! TypeScript, API answers and audit entries, so it is held here too.
use crate::{contracts::ScheduleCollection, features, roles};

const PERMISSIONS: &[(&str, &str)] = &[
    ("uniforms.view", "View Uniform Inventory"),
    ("uniforms.adjust", "Adjust Uniform Inventory"),
    ("uniforms.manage", "Manage Uniform Inventory"),
    ("timecard.view", "View Timecard"),
    ("timecard.manage", "Manage Timecard"),
    ("collections.run", "Run Collections"),
    ("routes.view", "View Routes"),
    ("routes.collect", "Collect Routes"),
    ("routes.manage", "Manage Routes"),
    ("dvic.view", "View DVIC"),
    ("dvic.collect", "Collect DVIC"),
    ("dvic.manage", "Manage DVIC"),
    ("scorecard.view", "View Scorecard"),
    ("scorecard.collect", "Collect Scorecard"),
    ("scorecard.manage", "Manage Scorecard"),
    ("driver_match.manage", "Manage Driver Match"),
    ("connections.manage", "Manage Connections"),
    ("members.invite", "Invite Members"),
    ("members.manage", "Manage Members"),
    ("roles.manage", "Manage Roles"),
    ("settings.manage", "Manage DSP Settings"),
];
const IMPLIED: &[(&str, &str)] = &[
    ("timecard.manage", "timecard.view"),
    ("uniforms.adjust", "uniforms.view"),
    ("uniforms.manage", "uniforms.view"),
    ("routes.collect", "routes.view"),
    ("routes.manage", "routes.view"),
    ("dvic.collect", "dvic.view"),
    ("dvic.manage", "dvic.view"),
    ("scorecard.collect", "scorecard.view"),
    ("scorecard.manage", "scorecard.view"),
];
const GROUPS: &[(&str, &[&str])] = &[
    ("Connections", &["connections.manage"]),
    (
        "Team",
        &["members.invite", "members.manage", "roles.manage"],
    ),
    ("DSP", &["settings.manage"]),
];
const DEFAULTS: &[(&str, &str, &[&str])] = &[
    (
        "manager",
        "Manager",
        &[
            "uniforms.view",
            "uniforms.adjust",
            "timecard.view",
            "collections.run",
        ],
    ),
    ("member", "Member", &["uniforms.view", "timecard.view"]),
];
type Page = (
    &'static str,
    &'static str,
    Vec<&'static str>,
    Vec<&'static str>,
    bool,
);
fn pages() -> Vec<Page> {
    vec![
        (
            "timecard",
            "Timecard",
            vec!["timecard.view", "timecard.manage", "collections.run"],
            vec!["timecards", "meal_breaks"],
            false,
        ),
        (
            "uniforms",
            "Uniform Inventory",
            vec!["uniforms.view", "uniforms.adjust", "uniforms.manage"],
            vec![],
            false,
        ),
        (
            "routes",
            "Routes",
            vec!["routes.view", "routes.collect", "routes.manage"],
            vec!["routes"],
            false,
        ),
        (
            "dvic",
            "DVIC",
            vec!["dvic.view", "dvic.collect", "dvic.manage"],
            vec!["dvic"],
            false,
        ),
        (
            "scorecard",
            "Scorecard",
            vec!["scorecard.view", "scorecard.collect", "scorecard.manage"],
            vec!["scorecard"],
            false,
        ),
        (
            "driver_match",
            "Driver Match",
            vec!["driver_match.manage"],
            vec!["timecards", "routes"],
            false,
        ),
    ]
}
const TABS: &[(&str, &str, &str)] = &[
    ("timecard.daily", "Timecard", "timecard"),
    ("timecard.meal_breaks", "Meal Breaks", "timecard"),
    ("timecard.employees", "Employee Search", "timecard"),
    ("dvic.day", "Day", "dvic"),
    ("dvic.week", "Week", "dvic"),
];
const CATALOG: &[&str] = &[
    "timecard",
    "uniforms",
    "routes",
    "dvic",
    "scorecard",
    "driver_match",
    "timecard.daily",
    "timecard.meal_breaks",
    "timecard.employees",
    "dvic.day",
    "dvic.week",
    "paycom",
    "cortex",
];
/// What each job kind or schedule collection names: the page that runs it.
const AUTOMATION: &[(&str, &str)] = &[
    ("paycom.collect", "timecard"),
    ("cortex.meal_breaks.collect", "timecard"),
    ("cortex.scorecard.collect", "scorecard"),
    ("cortex.routes.collect", "routes"),
    ("cortex.dvic.collect", "dvic"),
    ("paycom", "timecard"),
    ("meal_break", "timecard"),
    ("both", "timecard"),
    ("scorecard", "scorecard"),
    ("routes", "routes"),
    ("dvic", "dvic"),
    ("schedules", "timecard"),
    ("unknown", "timecard"),
    ("", "timecard"),
];
/// The permission a job of each kind runs under.
const COLLECTION_PERMISSIONS: &[(&str, &str)] = &[
    ("paycom.collect", "collections.run"),
    ("cortex.meal_breaks.collect", "collections.run"),
    ("cortex.scorecard.collect", "scorecard.collect"),
    ("cortex.routes.collect", "routes.collect"),
    ("cortex.dvic.collect", "dvic.collect"),
];
const SCHEDULE_COLLECTIONS: &[&str] = &[
    "paycom",
    "meal_break",
    "both",
    "scorecard",
    "routes",
    "dvic",
];

#[test]
fn the_permissions_keep_their_order_labels_implications_groups_and_defaults() {
    let ids: Vec<_> = PERMISSIONS.iter().map(|(id, _)| *id).collect();
    assert_eq!(roles::PERMISSIONS.to_vec(), ids);
    assert_eq!(roles::all(), ids);
    assert_eq!(roles::LABELS.to_vec(), PERMISSIONS);
    assert_eq!(roles::IMPLIED.to_vec(), IMPLIED);
    let groups: Vec<_> = roles::GROUPS
        .iter()
        .map(|(group, permissions)| (*group, permissions.as_slice()))
        .collect();
    assert_eq!(groups, GROUPS);
    let defaults: Vec<_> = roles::DEFAULTS
        .iter()
        .map(|(key, name, permissions)| (*key, *name, permissions.as_slice()))
        .collect();
    assert_eq!(defaults, DEFAULTS);
}

#[test]
fn the_catalog_keeps_its_pages_tabs_and_order() {
    let pages: Vec<Page> = features::pages()
        .map(|page| {
            assert_eq!(page.kind, features::Kind::Page);
            assert!(page.provides.is_empty());
            let (permissions, requires) = (page.permissions.to_vec(), page.requires.to_vec());
            (page.id, page.label, permissions, requires, page.default)
        })
        .collect();
    assert_eq!(pages, self::pages());
    let tabs: Vec<_> = features::catalog()
        .iter()
        .filter(|feature| matches!(feature.kind, features::Kind::Tab(_)))
        .map(|tab| {
            assert!(tab.default && tab.permissions.is_empty() && tab.requires.is_empty());
            match tab.kind {
                features::Kind::Tab(page) => (tab.id, tab.label, page),
                kind => panic!("{} is a {kind:?}", tab.id),
            }
        })
        .collect();
    assert_eq!(tabs, TABS);
    let catalog: Vec<_> = features::catalog().iter().map(|f| f.id).collect();
    assert_eq!(catalog, CATALOG);
    assert_eq!(features::schedules(), "timecard");
}

#[test]
fn each_collection_is_run_by_the_page_that_keeps_it() {
    for (named, page) in AUTOMATION {
        assert_eq!(features::automation(named), *page, "{named}");
    }
    for (kind, permission) in COLLECTION_PERMISSIONS {
        assert_eq!(features::collection_permission(kind), *permission, "{kind}");
    }
    for id in CATALOG {
        let automates = ["timecard", "routes", "dvic", "scorecard"].contains(id);
        assert_eq!(features::automates(&[(*id).to_owned()]), automates, "{id}");
    }
    assert!(!features::automates(&[]));
}

#[test]
fn schedules_collect_the_same_collections() {
    let all: Vec<_> = ScheduleCollection::all()
        .map(ScheduleCollection::as_str)
        .collect();
    assert_eq!(all, SCHEDULE_COLLECTIONS);
    for collection in SCHEDULE_COLLECTIONS {
        let parsed = ScheduleCollection::parse(collection).unwrap();
        assert_eq!(parsed.as_str(), *collection);
    }
    for other in ["", "cortex", "timecard", "paycom.collect", "Both"] {
        assert!(ScheduleCollection::parse(other).is_none(), "{other}");
    }
}
