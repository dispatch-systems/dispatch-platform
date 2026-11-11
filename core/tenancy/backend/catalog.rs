//! What a DSP may use. A feature is a page with the permissions it owns, a part of a page
//! with the permissions it owns, such as one of its tabs, or a connection to a provider. A
//! mandatory feature or part is on for every DSP; the platform owner switches the optional
//! ones per DSP: a switched-off feature's pages, permissions and automation do not exist for
//! that DSP, and nothing it stored is touched, so switching it back on restores everything,
//! its parts as they were. Pages and their parts come from the features' manifests, and
//! connections from the collector registry, each providing a capability; a page or a part
//! requires capabilities, never a provider by name.
use crate::{
    collection::registry::Provider,
    manifest::{self, registry},
};
use std::sync::LazyLock;

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
    pub owner: Option<&'static str>,
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
    /// For a page, whether it adds one of the DSP's own accounts to its DSP Connections, as a
    /// connection does.
    pub connects: bool,
    /// For a page, the features it grew from: new, it starts on for the DSPs with one on.
    pub starts_on_with: &'static [&'static str],
}
impl Feature {
    /// How the audit log names it: a part with its page, as "Timecard · Meal Breaks".
    pub(super) fn name(&self) -> String {
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
        .filter(|permission| permission.group.is_none() || feature.owner.is_some())
        .map(|permission| permission.id)
        .collect();
    Feature {
        id: switch.id,
        owner: feature.owner,
        label: switch.label,
        kind: Kind::Page,
        // Made once, with the catalog, which lasts as long as the process.
        permissions: Box::leak(permissions.into_boxed_slice()),
        provides: &[],
        requires: switch.requires,
        default: switch.mandatory,
        tab: false,
        mandatory: switch.mandatory,
        connects: feature.dsp_connection,
        starts_on_with: switch.starts_on_with,
    }
}
/// The parts of each page switched on their own: its tabs, and any other part. A part
/// exists while its page and its own switch are on, with the permissions it owns, and its
/// routes ask `Context::has` or one of its permissions; it may need a connection of its own
/// beyond its page's. Switching one never touches automation, which follows the page. An
/// optional part defaults off, as a new feature does: switching its page on brings only the
/// first of its tabs, which the page needs to show.
fn page_subs(feature: &manifest::Feature) -> impl Iterator<Item = Feature> {
    let switch = feature.switch;
    feature.subfeatures.iter().map(move |sub| {
        let permissions: Vec<_> = sub.permissions.iter().map(|p| p.id).collect();
        Feature {
            id: sub.id,
            owner: feature.owner,
            label: sub.label,
            kind: Kind::Sub(switch.id),
            // Made once, with the catalog, which lasts as long as the process.
            permissions: Box::leak(permissions.into_boxed_slice()),
            provides: &[],
            requires: sub.requires,
            default: sub.mandatory,
            tab: sub.tab,
            mandatory: sub.mandatory,
            connects: false,
            starts_on_with: &[],
        }
    })
}
/// Every page, in catalog order.
pub fn pages() -> impl Iterator<Item = &'static Feature> {
    catalog().iter().filter(|f| f.kind == Kind::Page)
}
/// The tabs of `page`, in catalog order.
pub(super) fn tabs(page: &str) -> impl Iterator<Item = &'static Feature> {
    catalog()
        .iter()
        .filter(move |t| t.tab && matches!(t.kind, Kind::Sub(p) if p == page))
}
/// The page that runs a job kind or a schedule collection: the page of the feature that
/// keeps the collection, or declares the alias, as Timecard declares `both`. None for what
/// no feature keeps, which nothing runs.
pub fn automation(kind_or_collection: &str) -> Option<&'static str> {
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
        .collections()
        .find(|c| c.job_kind == kind_or_collection || c.schedule == kind_or_collection)
        .and_then(|collection| registry.keeping(collection.job_kind))
        .or_else(aliasing)
        .map(|feature| feature.switch.id)
}
/// Whether `feature` is the page of a feature that reads a provider live (`live_readers`).
pub(super) fn reads_live(feature: &Feature) -> bool {
    registry()
        .features
        .iter()
        .any(|page| page.switch.id == feature.id && !page.live_readers.is_empty())
}
/// Whether a page that runs collections is on: one whose feature keeps a collection.
pub fn automates(enabled: &[String]) -> bool {
    let runs = |id: &str| {
        registry()
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
        owner: None,
        label: collector.label(),
        kind: Kind::Connection,
        permissions: &[],
        // Made once, with the catalog, which lasts as long as the process.
        provides: Box::leak(provides.into_boxed_slice()),
        requires: &[],
        default: false,
        tab: false,
        mandatory: false,
        connects: false,
        starts_on_with: &[],
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
    // Manage DSP Connections manages the DSP's own accounts: a collector's connection, and
    // any a page adds.
    if permission == CONNECTIONS {
        return catalog
            .iter()
            .any(|f| (f.kind == Kind::Connection || f.connects) && on(f));
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
pub(super) fn provided(capability: &str, enabled: &[String]) -> bool {
    catalog()
        .iter()
        .any(|f| f.provides.contains(&capability) && enabled.iter().any(|e| e == f.id))
}
/// Whether every requirement of `feature` has an enabled provider.
pub(super) fn satisfied(feature: &Feature, enabled: &[String]) -> bool {
    feature.requires.iter().all(|c| provided(c, enabled))
}

#[cfg(test)]
#[path = "../tests/backend/catalog.rs"]
mod tests;
