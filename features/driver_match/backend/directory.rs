//! Everyone, told apart, for the features that show people or use what is known of them:
//! core's directory, answered from the codes Driver Match keeps and what the DSP's features
//! say of each ID, as the page lists them.
use super::contacts::Contact;
use super::gather::{Gathered, gather};
use super::{IdRow, photos};
use dispatch_core::{
    Result,
    db::Store,
    foundation::names,
    manifest::people::{self, Everyone, Person, Photo, Source},
};
use std::collections::BTreeMap;

/// Everyone, for the features that show people: Timecard's employees and meal breaks name
/// each by their code and show their photos.
pub(crate) struct Directory;
impl people::Directory for Directory {
    fn everyone(&self, store: &Store, dsp: &str) -> Result<Everyone> {
        let gathered = gather(store, dsp)?;
        let people = gathered
            .held()
            .iter()
            .map(|(code, ids)| person(&gathered, code, ids))
            .collect();
        let merged = store
            .dsp(dsp)?
            .query_as::<(String, String)>(
                "SELECT code,merged_into FROM people WHERE merged_into IS NOT NULL",
                [],
            )?
            .into_iter()
            .collect();
        Ok(Everyone::new(people, merged))
    }
    fn photo(&self, store: &Store, dsp: &str, code: &str) -> Result<Option<Photo>> {
        photos::photo_of(store, dsp, code)
    }
}

/// A person as the page lists them, with every name and contact their IDs' sources have, from
/// their IDs in the sources' order.
fn person(gathered: &Gathered, code: &str, ids: &[(&IdRow, Source)]) -> Person {
    let listed = gathered.person(code, ids);
    let mut person = Person {
        code: listed.code,
        names: vec![listed.name.clone()],
        name: listed.name,
        role: listed.role,
        title: listed.title,
        ids: BTreeMap::new(),
        emails: vec![],
        phones: vec![],
        confirmed: ids.iter().any(|(row, _)| row.linked_by.decided()),
        photo: listed.photo,
    };
    for (row, source) in ids {
        person
            .ids
            .entry(*source)
            .or_default()
            .push(row.external_id.clone());
        let Some(x) = gathered.found.get(&(*source, row.external_id.clone())) else {
            continue;
        };
        once(
            &mut person.names,
            x.names.iter().map(|name| names::display(name)),
        );
        once(
            &mut person.emails,
            x.contacts.written(Contact::Email).iter().cloned(),
        );
        once(
            &mut person.phones,
            x.contacts.written(Contact::Phone).iter().cloned(),
        );
    }
    person
}
/// Adds each of `more` not already in `list`.
fn once(list: &mut Vec<String>, more: impl Iterator<Item = String>) {
    for item in more {
        if !list.contains(&item) {
            list.push(item);
        }
    }
}
