//! What a DSP may use. A feature is a page with the permissions it owns, a part of a page
//! with the permissions it owns, such as one of its tabs, or a connection to a provider. A
//! mandatory feature or part is on for every DSP; the platform owner switches the optional
//! ones per DSP: a switched-off feature's pages, permissions and automation do not exist for
//! that DSP, and nothing it stored is touched, so switching it back on restores everything,
//! its parts as they were. Pages and their parts come from the features' manifests, and
//! connections from the collector registry, each providing a capability; a page or a part
//! requires capabilities, never a provider by name.
use super::audit::AuditChange;
use crate::{
    Result,
    collection::registry::Provider,
    db::{FromRow, Row, Store, iso},
    ensure, job_statuses,
    manifest::{self, registry},
    platform_owner::api::types::{
        DspFeatureReport, DspFeatures, DspHidden, FeatureChange, FeatureState,
    },
    tenancy::api::types::DspStatus,
};
use rusqlite::params;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Page,
    /// A part of the page with this id, switched on its own: one of its tabs, or another.
    Sub(&'static str),
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
    /// What a page or a part needs one enabled provider of.
    pub requires: &'static [&'static str],
    /// Whether a DSP gets it when created, or while it has no row of its own.
    pub default: bool,
    /// For a part of a page, whether it is one of the page's tabs.
    pub tab: bool,
    /// Whether every DSP has it, with no switch.
    pub mandatory: bool,
}
impl Feature {
    /// How the audit log names it: a part with its page, as "Timecard · Meal Breaks".
    fn name(&self) -> String {
        match self.kind {
            Kind::Sub(page) => format!("{} · {}", find(page).map_or(page, |p| p.label), self.label),
            _ => self.label.to_owned(),
        }
    }
}
/// A feature's page, as its switch declares it, with the permissions the role sheet lists
/// under it.
fn page(feature: &manifest::Feature) -> Feature {
    let switch = feature.switch;
    let permissions: Vec<_> = feature
        .permissions
        .iter()
        .filter(|permission| permission.group.is_none())
        .map(|permission| permission.id)
        .collect();
    Feature {
        id: switch.id,
        label: switch.label,
        kind: Kind::Page,
        // Made once, with the catalog, which lasts as long as the process.
        permissions: Box::leak(permissions.into_boxed_slice()),
        provides: &[],
        requires: switch.requires,
        default: switch.mandatory,
        tab: false,
        mandatory: switch.mandatory,
    }
}
/// The parts of each page switched on their own: its tabs, and any other part. A part
/// exists while its page and its own switch are on, with the permissions it owns, and its
/// routes ask `Context::has` or one of its permissions; it may need a connection of its own
/// beyond its page's. Switching one never touches automation, which follows the page. An
/// optional page's part defaults on, so switching the page on shows every part until one is
/// switched off; a DSP still starts with none, as its pages are off. A mandatory page's
/// optional part defaults off, as a new feature does: its page is never switched on to show
/// it.
fn page_subs(feature: &manifest::Feature) -> impl Iterator<Item = Feature> {
    let switch = feature.switch;
    feature.subfeatures.iter().map(move |sub| {
        let permissions: Vec<_> = sub.permissions.iter().map(|p| p.id).collect();
        Feature {
            id: sub.id,
            label: sub.label,
            kind: Kind::Sub(switch.id),
            // Made once, with the catalog, which lasts as long as the process.
            permissions: Box::leak(permissions.into_boxed_slice()),
            provides: &[],
            requires: sub.requires,
            default: sub.mandatory || !switch.mandatory,
            tab: sub.tab,
            mandatory: sub.mandatory,
        }
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
        .filter(move |t| t.tab && matches!(t.kind, Kind::Sub(p) if p == page))
}
/// The page whose schedules, collections and jobs run, as its feature's manifest says.
/// Nothing collects without it, except the collections another feature keeps
/// (`automation`).
pub fn schedules() -> &'static str {
    registry()
        .features
        .iter()
        .find(|feature| feature.schedules)
        .map(|feature| feature.switch.id)
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
        .map_or_else(schedules, |feature| feature.switch.id)
}
/// Whether a page that runs collections is on: the schedules' page, or one whose
/// feature keeps a collection.
pub fn automates(enabled: &[String]) -> bool {
    let runs = |id: &str| {
        id == schedules()
            || registry()
                .features
                .iter()
                .any(|feature| !feature.keeps.is_empty() && feature.switch.id == id)
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
    let provides: Vec<_> = collector.capabilities().iter().map(|c| c.id).collect();
    Feature {
        id: collector.id(),
        label: collector.label(),
        kind: Kind::Connection,
        permissions: &[],
        // Made once, with the catalog, which lasts as long as the process.
        provides: Box::leak(provides.into_boxed_slice()),
        requires: &[],
        default: false,
        tab: false,
        mandatory: false,
    }
}
/// The catalog: every feature's page, their parts, then every registered connection.
static CATALOG: LazyLock<Vec<Feature>> = LazyLock::new(|| {
    let features = registry().features;
    let pages = features.iter().map(|feature| page(feature));
    let subs = features.iter().flat_map(|feature| page_subs(feature));
    pages
        .chain(subs)
        .chain(Provider::all().map(connection))
        .collect()
});
pub fn catalog() -> &'static [Feature] {
    &CATALOG
}
pub fn find(id: &str) -> Option<&'static Feature> {
    catalog().iter().find(|f| f.id == id)
}
/// Whether `permission` exists in a DSP with `enabled` features: the page or part that
/// owns it is on, or for the connections permission any connection is on. A permission
/// no feature owns always exists.
pub fn grants(enabled: &[String], permission: &str) -> bool {
    grants_in(catalog(), enabled, permission)
}
/// `grants`, in `catalog`.
fn grants_in(catalog: &[Feature], enabled: &[String], permission: &str) -> bool {
    let on = |f: &Feature| enabled.iter().any(|e| e == f.id);
    if permission == CONNECTIONS {
        return catalog.iter().any(|f| f.kind == Kind::Connection && on(f));
    }
    catalog
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
/// The features of `switches` that exist: a part of a page only while its page is on too.
pub fn effective(switches: &[String]) -> Vec<String> {
    switches
        .iter()
        .filter(|id| match find(id).map(|f| f.kind) {
            Some(Kind::Sub(page)) => switches.iter().any(|s| s == page),
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
            shown: row.get("shown")?,
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
    /// What the DSP's members see of its features: what it has, less what the platform owner
    /// hides from them, a hidden page's parts with it. A hidden feature keeps running.
    pub fn shown_features(&self, dsp: &str) -> Result<Vec<String>> {
        let hidden = self.hidden_features(dsp)?;
        let shown = |id: &str| !hidden.iter().any(|h| h == id);
        Ok(self
            .features(dsp)?
            .into_iter()
            .filter(|id| {
                shown(id)
                    && match find(id).map(|f| f.kind) {
                        Some(Kind::Sub(page)) => shown(page),
                        _ => true,
                    }
            })
            .collect())
    }
    /// The optional features and parts the platform owner hides from the DSP's members, in
    /// catalog order.
    fn hidden_features(&self, dsp: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = self.platform.query_as(
            "SELECT feature FROM dsp_features WHERE dsp_id=? AND shown=0",
            [dsp],
        )?;
        Ok(catalog()
            .iter()
            .filter(|f| !f.mandatory && f.kind != Kind::Connection)
            .filter(|f| rows.iter().any(|(id,)| id == f.id))
            .map(|f| f.id.to_owned())
            .collect())
    }
    /// Hides an optional feature or part from the DSP's members, or shows it to them again.
    /// Whatever it runs keeps running, and whether it is on stays as it is; the platform
    /// owner's own view still sees it. Audited; open views expire.
    pub fn show_feature(&self, dsp: &str, id: &str, shown: bool, actor: &str) -> Result<DspHidden> {
        let feature = find(id).ok_or_else(|| crate::Error::new("feature_not_found", 404))?;
        ensure(!feature.mandatory, "feature_mandatory", 409)?;
        ensure(feature.kind != Kind::Connection, "invalid_input", 400)?;
        self.platform.transaction(|| {
            self.find_dsp(dsp)?;
            let was = !self.hidden_features(dsp)?.iter().any(|h| h == id);
            if was != shown {
                let on = self.switches(dsp)?.iter().any(|e| e == id);
                self.platform.exec(
                    "INSERT INTO dsp_features(dsp_id,feature,enabled,shown,changed_by,changed_at) \
                     VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                     shown=?4,changed_by=?5,changed_at=?6",
                    params![dsp, id, on, shown, actor, iso()],
                )?;
                self.audit_with(
                    Some(actor),
                    Some(dsp),
                    if shown {
                        "dsp.feature_shown"
                    } else {
                        "dsp.feature_hidden"
                    },
                    id,
                    Some(&feature.name()),
                    &[],
                )?;
                // Open views sign the DSP revision, so members lose or regain it at once.
                self.platform
                    .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [dsp])?;
            }
            Ok(DspHidden {
                hidden: self.hidden_features(dsp)?,
            })
        })
    }
    /// Every feature switched on, in catalog order: the mandatory ones whatever is stored. A
    /// feature without a row of its own is at its default, which is how a DSP made before the
    /// feature existed reads.
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
            .filter(|f| f.mandatory || stored.get(f.id).copied().unwrap_or(f.default))
            .map(|f| f.id.to_owned())
            .collect())
    }
    /// Keeps what every DSP had: a feature or part that was mandatory when the server last
    /// started, and is optional now, is switched on, once, for every DSP there is, so making
    /// it optional takes it from none of them; a DSP made later starts with it off, as with
    /// any optional feature. Then records what is mandatory now.
    pub fn keep_mandatory_features(&self) -> Result<()> {
        self.platform.transaction(|| {
            let recorded: BTreeMap<String, bool> = self
                .platform
                .query_as("SELECT feature,mandatory FROM feature_availability", [])?
                .into_iter()
                .collect();
            let made_optional: Vec<&Feature> = catalog()
                .iter()
                .filter(|f| !f.mandatory && recorded.get(f.id) == Some(&true))
                .collect();
            if !made_optional.is_empty() {
                for row in self.platform.all("SELECT id FROM dsps", [])? {
                    for feature in &made_optional {
                        self.platform.exec(
                            "INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) \
                             VALUES (?,?,1,?) ON CONFLICT(dsp_id,feature) DO UPDATE SET \
                             enabled=1,changed_by=NULL,changed_at=excluded.changed_at",
                            params![crate::db::s(&row, "id"), feature.id, iso()],
                        )?;
                    }
                }
            }
            for feature in catalog().iter().filter(|f| f.kind != Kind::Connection) {
                self.platform.exec(
                    "INSERT INTO feature_availability(feature,mandatory) VALUES (?,?) \
                     ON CONFLICT(feature) DO UPDATE SET mandatory=excluded.mandatory",
                    params![feature.id, feature.mandatory],
                )?;
            }
            Ok(())
        })
    }
    /// Every feature of the catalog as the DSP has it, with what switching the
    /// schedules' page off would stop, for the platform's DSP page.
    pub fn feature_report(&self, dsp: &str) -> Result<DspFeatureReport> {
        let row = self.find_dsp(dsp)?;
        let stored: Vec<FeatureState> = self.platform.query_as(
            "SELECT f.feature,f.enabled,f.shown,f.changed_at,u.first_name||' '||u.last_name changed_by \
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
                    .map(|state| FeatureState {
                        enabled: state.enabled || f.mandatory,
                        shown: state.shown || f.mandatory || f.kind == Kind::Connection,
                        ..state
                    })
                    .unwrap_or(FeatureState {
                        feature: f.id.to_owned(),
                        enabled: f.default,
                        shown: true,
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
    /// Switches one optional feature, and with it whatever depends on it: enabling a page
    /// or a part enables a provider of each capability it, or a page's parts on with it,
    /// lacks, and a page's tabs when none is on; enabling a provider switches off another of
    /// the same capability; disabling a provider disables the pages and parts left without
    /// one; a page left with none of its tabs on is disabled. A page switched off keeps its
    /// parts' switches as they are, so they come back as they were. A mandatory feature or
    /// part has no switch. Every switch is audited; the answer lists them.
    pub fn set_feature(
        &self,
        dsp: &str,
        id: &str,
        enabled: bool,
        actor: &str,
    ) -> Result<DspFeatures> {
        let all = catalog();
        let feature = find(id).ok_or_else(|| crate::Error::new("feature_not_found", 404))?;
        ensure(!feature.mandatory, "feature_mandatory", 409)?;
        fn flip(
            current: &mut Vec<String>,
            changed: &mut Vec<(&'static Feature, bool)>,
            f: &'static Feature,
            on: bool,
        ) {
            if current.iter().any(|e| e == f.id) == on {
                return;
            }
            if on {
                current.push(f.id.to_owned());
            } else {
                current.retain(|e| e != f.id);
            }
            changed.push((f, on));
        }
        // On, and with its page on for a part: what exists.
        let live = |current: &[String], f: &Feature| {
            let on = |id: &str| current.iter().any(|e| e == id);
            on(f.id)
                && match f.kind {
                    Kind::Sub(page) => on(page),
                    _ => true,
                }
        };
        self.platform.transaction(|| {
            self.find_dsp(dsp)?;
            let mut current = self.switches(dsp)?;
            let mut changed: Vec<(&'static Feature, bool)> = Vec::new();
            if enabled {
                for capability in feature.provides {
                    for other in all
                        .iter()
                        .filter(|f| f.provides.contains(capability) && f.id != feature.id)
                    {
                        flip(&mut current, &mut changed, other, false);
                    }
                }
                // Each capability `f` requires that no connection on provides, from its one
                // provider.
                let connect = |current: &mut Vec<String>,
                               changed: &mut Vec<(&'static Feature, bool)>,
                               f: &Feature|
                 -> Result<()> {
                    for capability in f.requires {
                        if provided(capability, current) {
                            continue;
                        }
                        let providers: Vec<_> = all
                            .iter()
                            .filter(|f| f.provides.contains(capability))
                            .collect();
                        ensure(providers.len() == 1, "provider_required", 409)?;
                        flip(current, changed, providers[0], true);
                    }
                    Ok(())
                };
                connect(&mut current, &mut changed, feature)?;
                flip(&mut current, &mut changed, feature, true);
                let own: Vec<_> = tabs(feature.id).collect();
                if !own.iter().any(|t| current.iter().any(|e| e == t.id)) {
                    for tab in own {
                        flip(&mut current, &mut changed, tab, true);
                    }
                }
                // A page's parts that come on with it need their connections too.
                let parts: Vec<&'static Feature> = all
                    .iter()
                    .filter(|f| f.kind == Kind::Sub(feature.id) && live(&current, f))
                    .collect();
                for part in parts {
                    connect(&mut current, &mut changed, part)?;
                }
            } else {
                flip(&mut current, &mut changed, feature, false);
                // Then whatever is left short, until nothing more is: a page with none of
                // its tabs on, and a page or a part without a provider of what it requires.
                loop {
                    let before = changed.len();
                    for page in all.iter().filter(|f| f.kind == Kind::Page) {
                        let mut own = tabs(page.id).peekable();
                        if live(&current, page)
                            && own.peek().is_some()
                            && !own.any(|t| current.iter().any(|e| e == t.id))
                        {
                            flip(&mut current, &mut changed, page, false);
                        }
                    }
                    for f in all.iter().filter(|f| f.kind != Kind::Connection) {
                        if live(&current, f) && !satisfied(f, &current) {
                            flip(&mut current, &mut changed, f, false);
                        }
                    }
                    if changed.len() == before {
                        break;
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
#[path = "../tests/backend/catalog.rs"]
mod tests;
