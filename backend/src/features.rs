//! What a DSP may use. A feature is a page with the permissions it owns, a tab
//! inside a page, or a connection to a provider. The platform owner switches
//! features per DSP: a switched-off feature's pages, permissions and automation
//! do not exist for that DSP, and nothing it stored is touched, so switching it
//! back on restores everything. Connections come from the collector registry,
//! each providing a capability; a page requires capabilities, never a provider
//! by name.
use super::{
    Result,
    audit::AuditChange,
    collectors::Provider,
    contracts::{DspFeatureReport, DspFeatures, DspStatus, FeatureChange, FeatureState},
    db::{FromRow, Row, Store, iso},
    ensure, job_statuses,
};
use rusqlite::params;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Page,
    /// A tab of the page with this id.
    Tab(&'static str),
    Connection,
}
#[derive(Clone, Copy, Debug)]
pub struct Feature {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// The permissions this feature owns. Without it, nobody in the DSP holds them.
    pub permissions: &'static [&'static str],
    /// What a connection supplies, such as timecards.
    pub provides: &'static [&'static str],
    /// What a page needs one enabled provider of.
    pub requires: &'static [&'static str],
    /// Whether a DSP gets it when created, or while it has no row of its own.
    pub default: bool,
}
// Every page. Connections are listed by the collector registry below.
pub const PAGES: &[Feature] = &[
    Feature {
        id: "timecard",
        label: "Timecard",
        kind: Kind::Page,
        permissions: &["timecard.view", "timecard.manage", "collections.run"],
        provides: &[],
        requires: &["timecards", "meal_breaks"],
        default: false,
    },
    Feature {
        id: "uniforms",
        label: "Uniform Inventory",
        kind: Kind::Page,
        permissions: &["uniforms.view", "uniforms.adjust", "uniforms.manage"],
        provides: &[],
        requires: &[],
        default: false,
    },
    Feature {
        id: "routes",
        label: "Routes",
        kind: Kind::Page,
        permissions: &["routes.view", "routes.collect", "routes.manage"],
        provides: &[],
        requires: &["routes"],
        default: false,
    },
    Feature {
        id: "dvic",
        label: "DVIC",
        kind: Kind::Page,
        permissions: &["dvic.view", "dvic.collect", "dvic.manage"],
        provides: &[],
        requires: &["dvic"],
        default: false,
    },
    // Amazon's weekly scorecard: no page of its own yet, but its collection, its schedules
    // and its weeks, apart from the Timecard page.
    Feature {
        id: "scorecard",
        label: "Scorecard",
        kind: Kind::Page,
        permissions: &["scorecard.view", "scorecard.collect", "scorecard.manage"],
        provides: &[],
        requires: &["scorecard"],
        default: false,
    },
    // A tab of Settings, not a page of its own: it matches Paycom's employees to the
    // drivers Amazon's routes and other collections name.
    Feature {
        id: "driver_match",
        label: "Driver Match",
        kind: Kind::Page,
        permissions: &["driver_match.manage"],
        provides: &[],
        requires: &["timecards", "routes"],
        default: false,
    },
];
impl Feature {
    /// How the audit log names it: a tab with its page, as "Timecard · Meal Breaks".
    fn name(&self) -> String {
        match self.kind {
            Kind::Tab(page) => format!("{} · {}", find(page).map_or(page, |p| p.label), self.label),
            _ => self.label.to_owned(),
        }
    }
}
/// Tabs switched on their own, each inside its page. A tab owns no permissions and
/// requires nothing: it exists while its page and its own switch are on, so the page's
/// permissions gate it and its routes ask `Context::has`. Switching one never touches
/// automation, which follows the page. It defaults on, so a page switched on shows every
/// tab until one is switched off; a DSP still starts with none, as its pages are off.
pub const TABS: &[Feature] = &[
    tab("timecard.daily", "Timecard", "timecard"),
    tab("timecard.meal_breaks", "Meal Breaks", "timecard"),
    tab("timecard.employees", "Employee Search", "timecard"),
    tab("dvic.day", "Day", "dvic"),
    tab("dvic.week", "Week", "dvic"),
];
const fn tab(id: &'static str, label: &'static str, page: &'static str) -> Feature {
    Feature {
        id,
        label,
        kind: Kind::Tab(page),
        permissions: &[],
        provides: &[],
        requires: &[],
        default: true,
    }
}
/// The tabs of `page`, in catalog order.
fn tabs(page: &str) -> impl Iterator<Item = &'static Feature> {
    TABS.iter()
        .filter(move |t| matches!(t.kind, Kind::Tab(p) if p == page))
}
/// The page whose schedules, collections and jobs run. Nothing collects without it,
/// except the collections another feature owns (`automation`).
pub const SCHEDULES: &str = "timecard";
/// The routes, DVIC and scorecard collections, each owned by its own feature rather than the
/// timecard page. Their schedules and jobs have routes of their own.
const COLLECTION_PAGES: &[(&str, &str, &str)] = &[
    ("routes", "cortex.routes.collect", "routes"),
    ("dvic", "cortex.dvic.collect", "dvic"),
    ("scorecard", "cortex.scorecard.collect", "scorecard"),
];
pub fn automation(kind_or_collection: &str) -> &'static str {
    COLLECTION_PAGES
        .iter()
        .find(|(collection, kind, _)| {
            *collection == kind_or_collection || *kind == kind_or_collection
        })
        .map_or(SCHEDULES, |(_, _, page)| *page)
}
pub fn automates(enabled: &[String]) -> bool {
    enabled
        .iter()
        .any(|f| f == SCHEDULES || COLLECTION_PAGES.iter().any(|(_, _, page)| f == page))
}
pub fn collection_permission(kind: &str) -> String {
    let page = automation(kind);
    if page == SCHEDULES {
        "collections.run".into()
    } else {
        format!("{page}.collect")
    }
}
/// The permission every connection shares; it exists while any connection does.
const CONNECTIONS: &str = "connections.manage";

fn connection(provider: Provider) -> Feature {
    let collector = provider.collector();
    Feature {
        id: collector.id(),
        label: collector.label(),
        kind: Kind::Connection,
        permissions: &[],
        provides: collector.capabilities(),
        requires: &[],
        default: false,
    }
}
/// The catalog: pages, their tabs, then every registered connection.
static CATALOG: LazyLock<Vec<Feature>> = LazyLock::new(|| {
    PAGES
        .iter()
        .chain(TABS)
        .copied()
        .chain(Provider::ALL.iter().map(|p| connection(*p)))
        .collect()
});
pub fn catalog() -> &'static [Feature] {
    &CATALOG
}
pub fn find(id: &str) -> Option<&'static Feature> {
    catalog().iter().find(|f| f.id == id)
}
/// Whether `permission` exists in a DSP with `enabled` features: its owning page
/// is on, or for the connections permission any connection is on. A permission
/// no feature owns always exists.
pub fn grants(enabled: &[String], permission: &str) -> bool {
    let on = |f: &Feature| enabled.iter().any(|e| e == f.id);
    if permission == CONNECTIONS {
        return catalog()
            .iter()
            .any(|f| f.kind == Kind::Connection && on(f));
    }
    PAGES
        .iter()
        .find(|f| f.permissions.contains(&permission))
        .is_none_or(on)
}
/// The permissions of `stored` that exist with `enabled` features.
pub fn visible<'a>(
    enabled: &'a [String],
    stored: &'a [String],
) -> impl Iterator<Item = &'a String> {
    stored.iter().filter(move |p| grants(enabled, p))
}
/// The features of `switches` that exist: a tab only while its page is on too.
pub fn effective(switches: &[String]) -> Vec<String> {
    switches
        .iter()
        .filter(|id| match find(id).map(|f| f.kind) {
            Some(Kind::Tab(page)) => switches.iter().any(|s| s == page),
            _ => true,
        })
        .cloned()
        .collect()
}
/// Whether an enabled connection provides `capability`.
fn provided(capability: &str, enabled: &[String]) -> bool {
    catalog()
        .iter()
        .any(|f| f.provides.contains(&capability) && enabled.iter().any(|e| e == f.id))
}
/// Whether every requirement of `feature` has an enabled provider.
fn satisfied(feature: &Feature, enabled: &[String]) -> bool {
    feature.requires.iter().all(|c| provided(c, enabled))
}

impl FromRow for FeatureState {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            feature: row.get("feature")?,
            enabled: row.get("enabled")?,
            changed_at: row.get("changed_at")?,
            changed_by: row.get("changed_by")?,
        })
    }
}
impl Store {
    /// The DSP's features, in catalog order: what is switched on, less the tabs of a
    /// page that is off.
    pub fn features(&self, dsp: &str) -> Result<Vec<String>> {
        Ok(effective(&self.switches(dsp)?))
    }
    /// Every feature switched on, in catalog order. A feature without a row of its own
    /// is at its default, which is how a DSP made before the feature existed reads.
    fn switches(&self, dsp: &str) -> Result<Vec<String>> {
        let stored: BTreeMap<String, bool> = self
            .platform
            .query_as(
                "SELECT feature,enabled FROM dsp_features WHERE dsp_id=?",
                [dsp],
            )?
            .into_iter()
            .collect();
        Ok(catalog()
            .iter()
            .filter(|f| stored.get(f.id).copied().unwrap_or(f.default))
            .map(|f| f.id.to_owned())
            .collect())
    }
    /// Every feature of the catalog as the DSP has it, with what switching the
    /// schedules' page off would stop, for the platform's DSP page.
    pub fn feature_report(&self, dsp: &str) -> Result<DspFeatureReport> {
        let row = self.find_dsp(dsp)?;
        let stored: Vec<FeatureState> = self.platform.query_as(
            "SELECT f.feature,f.enabled,f.changed_at,u.first_name||' '||u.last_name changed_by \
             FROM dsp_features f LEFT JOIN users u ON u.id=f.changed_by WHERE f.dsp_id=?",
            [dsp],
        )?;
        let features = catalog()
            .iter()
            .map(|f| {
                stored
                    .iter()
                    .find(|s| s.feature == f.id)
                    .cloned()
                    .unwrap_or(FeatureState {
                        feature: f.id.to_owned(),
                        enabled: f.default,
                        changed_at: None,
                        changed_by: None,
                    })
            })
            .collect();
        let provisioned = [DspStatus::Active, DspStatus::Suspended].contains(&row.status);
        Ok(DspFeatureReport {
            features,
            schedules: if provisioned {
                self.dsp(dsp)?.count(
                    "SELECT count(*) FROM collection_schedules WHERE enabled=1",
                    [],
                )?
            } else {
                0
            },
            active_jobs: self.jobs.count(
                concat!(
                    "SELECT count(*) FROM jobs WHERE dsp_id=? AND status IN ",
                    job_statuses!(active)
                ),
                [dsp],
            )?,
        })
    }
    pub fn feature_enabled(&self, dsp: &str, id: &str) -> Result<bool> {
        Ok(self.features(dsp)?.iter().any(|f| f == id))
    }
    /// Switches every feature on for a development or preview DSP, so its demo shows every
    /// page. Real DSPs start with none and get theirs from the platform owner.
    pub fn enable_all_features(&self, dsp: &str) -> Result<()> {
        self.write_features(dsp, |_| true, true)
    }
    /// Writes a new DSP's rows, so a later change of a default leaves it as it was made.
    pub fn seed_features(&self, dsp: &str) -> Result<()> {
        self.write_features(dsp, |f| f.default, false)
    }
    /// A row for every feature, `on` or not; `overwrite` replaces rows the DSP has.
    fn write_features(&self, dsp: &str, on: fn(&Feature) -> bool, overwrite: bool) -> Result<()> {
        let sql = if overwrite {
            "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,?,?,?) \
             ON CONFLICT(dsp_id,feature) DO UPDATE SET enabled=excluded.enabled,changed_at=excluded.changed_at"
        } else {
            "INSERT OR IGNORE INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,?,?,?)"
        };
        for feature in catalog() {
            self.platform
                .exec(sql, params![dsp, feature.id, on(feature), iso()])?;
        }
        Ok(())
    }
    /// Switches one feature, and with it whatever depends on it: enabling a page
    /// enables a provider of each capability it lacks, and its tabs when none is on;
    /// enabling a provider switches off another of the same capability; disabling a
    /// provider disables the pages left without one; disabling a page's last tab
    /// disables the page. A page switched off keeps its tabs' switches as they are.
    /// Every switch is audited; the answer lists them.
    pub fn set_feature(
        &self,
        dsp: &str,
        id: &str,
        enabled: bool,
        actor: &str,
    ) -> Result<DspFeatures> {
        let all = catalog();
        let feature = find(id).ok_or_else(|| super::Error::new("feature_not_found", 404))?;
        self.platform.transaction(|| {
            self.find_dsp(dsp)?;
            let mut current = self.switches(dsp)?;
            let mut changed: Vec<(&Feature, bool)> = Vec::new();
            let mut flip = |current: &mut Vec<String>, f: &'static Feature, on: bool| {
                let is_on = current.iter().any(|e| e == f.id);
                if is_on == on {
                    return;
                }
                if on {
                    current.push(f.id.to_owned());
                } else {
                    current.retain(|e| e != f.id);
                }
                changed.push((f, on));
            };
            if enabled {
                for capability in feature.provides {
                    for other in all
                        .iter()
                        .filter(|f| f.provides.contains(capability) && f.id != feature.id)
                    {
                        flip(&mut current, other, false);
                    }
                }
                for capability in feature.requires {
                    if provided(capability, &current) {
                        continue;
                    }
                    let providers: Vec<_> = all
                        .iter()
                        .filter(|f| f.provides.contains(capability))
                        .collect();
                    ensure(providers.len() == 1, "provider_required", 409)?;
                    flip(&mut current, providers[0], true);
                }
                flip(&mut current, feature, true);
                let own: Vec<_> = tabs(feature.id).collect();
                if !own.iter().any(|t| current.iter().any(|e| e == t.id)) {
                    for tab in own {
                        flip(&mut current, tab, true);
                    }
                }
            } else {
                flip(&mut current, feature, false);
                if let Kind::Tab(page) = feature.kind
                    && current.iter().any(|e| e == page)
                    && !tabs(page).any(|t| current.iter().any(|e| e == t.id))
                {
                    flip(&mut current, find(page).expect("a tab's page"), false);
                }
                for page in all.iter().filter(|f| f.kind == Kind::Page) {
                    if !satisfied(page, &current) {
                        flip(&mut current, page, false);
                    }
                }
            }
            for (f, on) in &changed {
                self.platform.exec(
                    "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_by,changed_at) \
                     VALUES (?1,?2,?3,?4,?5) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                     enabled=?3,changed_by=?4,changed_at=?5",
                    params![dsp, f.id, on, actor, iso()],
                )?;
                let cause: Vec<AuditChange> = if f.id == id {
                    vec![]
                } else {
                    vec![("cause", None, Some(feature.name()))]
                };
                self.audit_with(
                    Some(actor),
                    Some(dsp),
                    if *on {
                        "dsp.feature_enabled"
                    } else {
                        "dsp.feature_disabled"
                    },
                    f.id,
                    Some(&f.name()),
                    &cause,
                )?;
            }
            if !changed.is_empty() {
                // Open views sign the DSP revision, so members pick up the change.
                self.platform
                    .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
            }
            Ok(DspFeatures {
                features: self.features(dsp)?,
                changed: changed
                    .into_iter()
                    .map(|(f, on)| FeatureChange {
                        feature: f.id.to_owned(),
                        enabled: on,
                    })
                    .collect(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
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
        assert!(find(SCHEDULES).is_some_and(|f| f.kind == Kind::Page));
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
}
