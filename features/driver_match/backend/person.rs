//! A person as the page lists them: what each source says of their IDs, what they do for the
//! DSP, and their photo.
use super::IdRow;
use super::contacts::Contact;
use super::photos;
use super::sources::{DriverDepartments, Found, Identity, Key, Seen};
use crate::api::types::{Person, PersonId, PersonRole, PersonTitle};
use dispatch_core::{
    foundation::names,
    manifest::people::{Kind, Source},
};
use std::collections::{BTreeMap, BTreeSet};

/// Whether someone works in the office: outside the departments their source's settings
/// count as drivers.
fn office(x: &Identity, drivers: &DriverDepartments) -> bool {
    drivers
        .get(&x.source)
        .is_some_and(|list| !list.contains(x.department.as_deref().unwrap_or("")))
}

impl IdRow {
    /// The ID as its sources describe it now; one no source holds any more shows bare.
    fn public(&self, source: Source, found: &Found) -> PersonId {
        let x = found.get(&(source, self.external_id.clone()));
        PersonId {
            source,
            id: self.external_id.clone(),
            name: x.map(|x| x.name().to_owned()).unwrap_or_default(),
            department: x.and_then(|x| x.department.clone()),
            emails: x.map_or(vec![], |x| x.contacts.written(Contact::Email).to_vec()),
            phones: x.map_or(vec![], |x| x.contacts.written(Contact::Phone).to_vec()),
            roles: x.map(|x| x.roles.clone()).unwrap_or_default(),
            listed: x.is_some_and(|x| x.listed),
            photo: x.is_some_and(|x| x.photo.is_some()),
            first_seen: x.and_then(|x| x.first_seen.clone()),
            last_seen: x.and_then(|x| x.last_seen.clone()),
            linked_by: self.linked_by,
            confirmable: self.linked_by.confirmable(),
        }
    }
}

/// What a person is drawn from.
pub(super) struct Context<'a> {
    pub found: &'a Found,
    pub activity: &'a BTreeMap<Key, BTreeMap<Kind, Seen>>,
    pub departments: &'a DriverDepartments,
    /// The kinds whose records show someone driving.
    pub driving: &'a BTreeSet<Kind>,
    /// The titles someone gave people, by their codes.
    pub titles: &'a BTreeMap<String, PersonTitle>,
    /// The first day that still counts as recent.
    pub recent: &'a str,
}

/// What a person does for the DSP: no roster lists them and nothing has seen them lately,
/// they have left, unless only lists of people the DSP can reach know them, and one still
/// does, so they work in the office; someone gave them a title, they are management; none of
/// their sources shows them driving and the departments the DSP counts as drivers leave them
/// out, they work in the office; else the roles their sources give say whether they drive or
/// only help.
fn role(
    keys: &[Key],
    described: &[&Identity],
    last_seen: Option<&str>,
    titled: bool,
    context: &Context,
) -> PersonRole {
    // A list of people the DSP can reach says nothing of who still works for it.
    let employed = described.iter().any(|x| x.listed && !x.contacts_only);
    if !employed && last_seen.is_none_or(|last| last < context.recent) {
        let reached = !described.is_empty()
            && described.iter().all(|x| x.contacts_only)
            && described.iter().any(|x| x.listed);
        return if reached {
            PersonRole::Office
        } else {
            PersonRole::Former
        };
    }
    // A title says what no source can: a dispatcher who covers a route is still dispatch.
    if titled {
        return PersonRole::Office;
    }
    // Driving shows in a source that says what they do, or in records of a kind that says each
    // shows them driving (`.driving()`).
    let drives = described.iter().any(|x| x.drives())
        || keys.iter().any(|key| {
            context
                .activity
                .get(key)
                .is_some_and(|data| data.keys().any(|kind| context.driving.contains(kind)))
        });
    let counted: Vec<&&Identity> = described
        .iter()
        .filter(|x| context.departments.contains_key(&x.source))
        .collect();
    if !drives && !counted.is_empty() && counted.iter().all(|x| office(x, context.departments)) {
        return PersonRole::Office;
    }
    let roles: BTreeSet<&str> = described
        .iter()
        .flat_map(|x| x.roles.iter().map(String::as_str))
        .collect();
    if roles.contains("Helper") && !roles.contains("Driver") {
        PersonRole::Helper
    } else {
        PersonRole::Driver
    }
}
/// The name a person is shown by: as the first source in the sources' order writes it; `ids`
/// are in that order. None while no source holds any of their IDs.
pub(super) fn shown_name(ids: &[(&IdRow, Source)], found: &Found) -> Option<String> {
    ids.iter().find_map(|(row, source)| {
        let x = found.get(&(*source, row.external_id.clone()))?;
        Some(names::display(x.name()))
    })
}

/// A person as the page lists them, from their IDs in the sources' order.
pub(super) fn person(code: &str, ids: &[(&IdRow, Source)], context: &Context) -> Person {
    let keys: Vec<Key> = ids
        .iter()
        .map(|(row, source)| (*source, row.external_id.clone()))
        .collect();
    let seen: Vec<&BTreeMap<Kind, Seen>> = keys
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
    let title = context.titles.get(code).copied();
    let role = role(
        &keys,
        &described,
        last_seen.as_deref(),
        title.is_some(),
        context,
    );
    Person {
        code: code.to_owned(),
        // An ID no source holds any more names a person no source describes.
        name: shown_name(ids, context.found)
            .or_else(|| ids.first().map(|(row, _)| row.external_id.clone()))
            .unwrap_or_default(),
        role,
        // Someone who has left keeps their title for when they come back.
        title: title.filter(|_| !role.left()),
        ids: ids
            .iter()
            .map(|(row, source)| row.public(*source, context.found))
            .collect(),
        photo: photos::version(&described),
    }
}
