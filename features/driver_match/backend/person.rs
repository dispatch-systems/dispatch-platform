//! A person as the tab lists them: what each source says of their IDs, what they do for the
//! DSP, how surely their IDs are one person's, and their photo.
use super::IdRow;
use super::sources::{DriverDepartments, Found, Identity, Key, Seen};
use crate::api::types::{Driver, DriverId, DriverLink, DriverRole};
use dispatch_core::{
    foundation::names,
    manifest::people::{DriverData, DriverSource},
};
use std::collections::{BTreeMap, BTreeSet};

/// How sure a link is, surest last.
fn sureness(link: DriverLink) -> u8 {
    match link {
        DriverLink::New => 0,
        DriverLink::Variant => 1,
        DriverLink::Name => 2,
        DriverLink::Phone => 3,
        DriverLink::Email => 4,
        DriverLink::Saved => 5,
        DriverLink::Person => 6,
    }
}
/// Whether someone works in the office: outside the departments their source's settings
/// count as drivers.
fn office(x: &Identity, drivers: &DriverDepartments) -> bool {
    drivers
        .get(&x.source)
        .is_some_and(|list| !list.contains(x.department.as_deref().unwrap_or("")))
}

impl IdRow {
    /// The ID as its sources describe it now; one no source holds any more shows bare.
    fn public(&self, source: DriverSource, found: &Found) -> DriverId {
        let x = found.get(&(source, self.external_id.clone()));
        DriverId {
            source,
            id: self.external_id.clone(),
            name: x.map(|x| x.name().to_owned()).unwrap_or_default(),
            department: x.and_then(|x| x.department.clone()),
            position: x.and_then(|x| x.position.clone()),
            emails: x.map(|x| x.written_emails.clone()).unwrap_or_default(),
            phones: x.map(|x| x.written_phones.clone()).unwrap_or_default(),
            roles: x.map(|x| x.roles.clone()).unwrap_or_default(),
            listed: x.is_some_and(|x| x.listed),
            photo: x.is_some_and(|x| x.photo.is_some()),
            first_seen: x.and_then(|x| x.first_seen.clone()),
            last_seen: x.and_then(|x| x.last_seen.clone()),
            linked_by: self.linked_by,
            linked_at: self.linked_at.clone(),
        }
    }
}

/// What a person is drawn from.
pub(super) struct Context<'a> {
    pub found: &'a Found,
    pub activity: &'a BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    pub departments: &'a DriverDepartments,
    /// The first day that still counts as recent.
    pub recent: &'a str,
}

/// What a person does for the DSP: no source lists them and nothing has seen them lately,
/// they have left; none of their sources shows them driving and the departments the DSP
/// counts as drivers leave them out, they work in the office; else Amazon's roster says
/// whether they drive or only help.
fn role(
    keys: &[Key],
    described: &[&Identity],
    last_seen: Option<&str>,
    context: &Context,
) -> DriverRole {
    let gone =
        !described.iter().any(|x| x.listed) && last_seen.is_none_or(|last| last < context.recent);
    if gone {
        return DriverRole::Former;
    }
    // Driving shows in a source that says what they do, or in data no department setting
    // speaks for: routes, meal breaks, inspections.
    let drives = described.iter().any(|x| x.drives())
        || keys.iter().any(|key| {
            !context.departments.contains_key(&key.0)
                && context
                    .activity
                    .get(key)
                    .is_some_and(|data| !data.is_empty())
        });
    let counted: Vec<&&Identity> = described
        .iter()
        .filter(|x| context.departments.contains_key(&x.source))
        .collect();
    if !drives && !counted.is_empty() && counted.iter().all(|x| office(x, context.departments)) {
        return DriverRole::Office;
    }
    let roles: BTreeSet<&str> = described
        .iter()
        .flat_map(|x| x.roles.iter().map(String::as_str))
        .collect();
    if roles.contains("Helper") && !roles.contains("Driver") {
        DriverRole::Helper
    } else {
        DriverRole::Driver
    }
}
/// How surely a person's IDs are one person's: the least sure link among them, `new` while
/// they hold one ID.
fn weakest(ids: &[(&IdRow, DriverSource)]) -> DriverLink {
    ids.iter()
        .map(|(row, _)| row.linked_by)
        .filter(|link| *link != DriverLink::New)
        .min_by_key(|link| sureness(*link))
        .unwrap_or(DriverLink::New)
}
/// The name a person is shown by: as the first source in the sources' order writes it, so a
/// payroll's legal name comes before Amazon's; `ids` are in that order. None while no source
/// holds any of their IDs.
pub(super) fn shown_name(ids: &[(&IdRow, DriverSource)], found: &Found) -> Option<String> {
    ids.iter().find_map(|(row, source)| {
        let x = found.get(&(*source, row.external_id.clone()))?;
        Some(names::display(x.name()))
    })
}

/// A person as the tab lists them, before review marks the ones in a pair.
pub(super) fn driver(code: &str, ids: &[(&IdRow, DriverSource)], context: &Context) -> Driver {
    let keys: Vec<Key> = ids
        .iter()
        .map(|(row, source)| (*source, row.external_id.clone()))
        .collect();
    let seen: Vec<&BTreeMap<DriverData, Seen>> = keys
        .iter()
        .filter_map(|key| context.activity.get(key))
        .collect();
    let described: Vec<&Identity> = keys
        .iter()
        .filter_map(|key| context.found.get(key))
        .collect();
    let last_seen = seen
        .iter()
        .flat_map(|data| data.values().filter_map(|s| s.last.clone()))
        .chain(described.iter().filter_map(|x| x.last_seen.clone()))
        .max();
    Driver {
        code: code.to_owned(),
        // An ID no source holds any more names a person no source describes.
        name: shown_name(ids, context.found)
            .or_else(|| ids.first().map(|(row, _)| row.external_id.clone()))
            .unwrap_or_default(),
        role: role(&keys, &described, last_seen.as_deref(), context),
        link: weakest(ids),
        review: false,
        ids: ids
            .iter()
            .map(|(row, source)| row.public(*source, context.found))
            .collect(),
        appears: DriverData::all()
            .filter(|d| seen.iter().any(|data| data.contains_key(d)))
            .collect(),
        last_seen,
        photo: described.iter().find_map(|x| x.photo.clone()),
    }
}
