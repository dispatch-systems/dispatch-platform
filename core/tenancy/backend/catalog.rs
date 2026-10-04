//! What a DSP may use. A feature is a page with the permissions it owns, a tab
//! inside a page, or a connection to a provider. The platform owner switches
//! features per DSP: a switched-off feature's pages, permissions and automation
//! do not exist for that DSP, and nothing it stored is touched, so switching it
//! back on restores everything. Pages and their tabs come from the features'
//! manifests, and connections from the collector registry, each providing a
//! capability; a page requires capabilities, never a provider by name.
use super::audit::AuditChange;
use crate::{
    Result,
    collection::registry::Provider,
    db::{FromRow, Row, Store, iso},
    ensure, job_statuses,
    manifest::{self, registry},
    platform_owner::api::types::{DspFeatureReport, DspFeatures, FeatureChange, FeatureState},
    tenancy::api::types::DspStatus,
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
impl Feature {
    /// How the audit log names it: a tab with its page, as "Timecard · Meal Breaks".
    fn name(&self) -> String {
        match self.kind {
            Kind::Tab(page) => format!("{} · {}", find(page).map_or(page, |p| p.label), self.label),
            _ => self.label.to_owned(),
        }
    }
}
/// A feature's page, as its switch declares it, with the permissions the role sheet lists
/// under it.
fn page(feature: &manifest::Feature) -> Option<Feature> {
    let switch = feature.switch?;
    let permissions: Vec<_> = feature
        .permissions
        .iter()
        .filter(|permission| permission.group.is_none())
        .map(|permission| permission.id)
        .collect();
    Some(Feature {
        id: switch.id,
        label: switch.label,
        kind: Kind::Page,
        // Made once, with the catalog, which lasts as long as the process.
        permissions: Box::leak(permissions.into_boxed_slice()),
        provides: &[],
        requires: switch.requires,
        default: false,
    })
}
/// Tabs switched on their own, each inside its page. A tab owns no permissions and
/// requires nothing: it exists while its page and its own switch are on, so the page's
/// permissions gate it and its routes ask `Context::has`. Switching one never touches
/// automation, which follows the page. It defaults on, so a page switched on shows every
/// tab until one is switched off; a DSP still starts with none, as its pages are off.
fn page_tabs(feature: &manifest::Feature) -> impl Iterator<Item = Feature> {
    feature.switch.into_iter().flat_map(|switch| {
        feature.tabs.iter().map(move |tab| Feature {
            id: tab.id,
            label: tab.label,
            kind: Kind::Tab(switch.id),
            permissions: &[],
            provides: &[],
            requires: &[],
            default: true,
        })
    })
}
/// Every page, in catalog order.
pub fn pages() -> impl Iterator<Item = &'static Feature> {
    catalog().iter().filter(|f| f.kind == Kind::Page)
}
/// The tabs of `page`, in catalog order.
fn tabs(page: &str) -> impl Iterator<Item = &'static Feature> {
    catalog()
        .iter()
        .filter(move |t| matches!(t.kind, Kind::Tab(p) if p == page))
}
/// The page whose schedules, collections and jobs run, as its feature's manifest says.
/// Nothing collects without it, except the collections another feature keeps
/// (`automation`).
pub fn schedules() -> &'static str {
    registry()
        .features
        .iter()
        .find(|feature| feature.schedules)
        .and_then(|feature| feature.switch)
        .map(|switch| switch.id)
        .expect("a page runs the schedules")
}
/// The page that runs a job kind or a schedule collection: the page of the feature that
/// keeps the collection, or declares the alias, as Timecard declares `both`. Anything else
/// is the schedules' page's.
pub fn automation(kind_or_collection: &str) -> &'static str {
    let registry = registry();
    let aliasing = || {
        registry.features.iter().copied().find(|feature| {
            feature
                .schedule_aliases
                .iter()
                .any(|alias| alias.schedule == kind_or_collection)
        })
    };
    registry
        .collectors
        .iter()
        .flat_map(|collector| collector.collections())
        .find(|c| c.job_kind == kind_or_collection || c.schedule == kind_or_collection)
        .and_then(|collection| registry.keeping(collection.job_kind))
        .or_else(aliasing)
        .and_then(|feature| feature.switch)
        .map_or_else(schedules, |switch| switch.id)
}
/// Whether a page that runs collections is on: the schedules' page, or one whose
/// feature keeps a collection.
pub fn automates(enabled: &[String]) -> bool {
    let runs = |id: &str| {
        id == schedules()
            || registry().features.iter().any(|feature| {
                !feature.keeps.is_empty() && feature.switch.is_some_and(|switch| switch.id == id)
            })
    };
    enabled.iter().any(|f| runs(f))
}
/// The permission a job of `kind` runs under, as its collection's keeper declares it.
pub fn collection_permission(kind: &str) -> String {
    registry().keeper(kind).permission().to_owned()
}
/// The permission every connection shares; it exists while any connection does.
pub const CONNECTIONS: &str = crate::collection::registry::CONNECTIONS.id;

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
/// The catalog: every feature's page, their tabs, then every registered connection.
static CATALOG: LazyLock<Vec<Feature>> = LazyLock::new(|| {
    let features = registry().features;
    let pages = features.iter().filter_map(|feature| page(feature));
    let tabs = features.iter().flat_map(|feature| page_tabs(feature));
    pages
        .chain(tabs)
        .chain(Provider::all().map(connection))
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
    pages()
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
        let feature = find(id).ok_or_else(|| crate::Error::new("feature_not_found", 404))?;
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
