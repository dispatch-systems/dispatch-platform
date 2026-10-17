//! What each source knows about the people in it: the IDs, the names it writes, the contacts
//! it has and the days it saw them, as each feature switched on for the DSP whose data names
//! people says through the people slot. A feature switched off says nothing, and its IDs sit
//! idle until it is back. Read afresh each time; Driver Match stores only what was decided.
use super::contacts;
use dispatch_core::{
    Result,
    db::Store,
    manifest::{
        people::{DriverData, DriverSource, People, Workdays},
        registry,
    },
};
use std::collections::{BTreeMap, BTreeSet};

/// An ID as one source knows it.
pub(crate) type Key = (DriverSource, String);
/// Every ID the DSP's data holds now, as its sources describe it.
pub(crate) type Found = BTreeMap<Key, Identity>;

/// Every kind of data that names people of the features the DSP has on, in the order their
/// spellings are read.
pub(crate) fn people(store: &Store, dsp: &str) -> Result<Vec<&'static People>> {
    let on = store.features(dsp)?;
    let mut people: Vec<&'static People> = registry()
        .features
        .iter()
        .filter(|feature| on.iter().any(|id| id == feature.switch.id))
        .flat_map(|feature| feature.people.iter().copied())
        .collect();
    people.sort_by_key(|people| people.reading_order());
    Ok(people)
}
/// The sources only lists of people the DSP can reach name, as Slack's members. An ID such a
/// source no longer names went with its list, as a workspace disconnected: it sits idle, shown
/// nowhere and keeping nobody from another, and comes back with its list.
pub(crate) fn contacts_only(people: &[&'static People]) -> BTreeSet<DriverSource> {
    let employs: BTreeSet<DriverSource> = people
        .iter()
        .filter(|people| !people.contacts_only)
        .map(|people| people.source())
        .collect();
    people
        .iter()
        .map(|people| people.source())
        .filter(|source| !employs.contains(source))
        .collect()
}
/// The DSP's systems, in their order: the sources of the features it has on that name someone
/// now, or that someone holds an ID of. One that has named nobody yet, as a workspace whose
/// admin hasn't let Dispatch read its members, is none of its systems.
pub(crate) fn active(
    people: &[&'static People],
    found: &Found,
    held: impl IntoIterator<Item = DriverSource>,
) -> Vec<DriverSource> {
    let named: BTreeSet<DriverSource> = found
        .keys()
        .map(|(source, _)| *source)
        .chain(held)
        .collect();
    let set: BTreeSet<DriverSource> = people
        .iter()
        .map(|people| people.source())
        .filter(|source| named.contains(source))
        .collect();
    set.into_iter().collect()
}

/// One ID as its sources describe it. Amazon's sources each write a driver's name their
/// own way; `names` keeps every spelling once, the most readable first.
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub source: DriverSource,
    pub id: String,
    pub names: Vec<String>,
    pub department: Option<String>,
    pub position: Option<String>,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On a source's current roster.
    pub listed: bool,
    /// Its contacts, each written one way, to match by.
    pub emails: BTreeSet<String>,
    pub phones: BTreeSet<String>,
    /// The same contacts as its sources wrote them, to show.
    pub written_emails: Vec<String>,
    pub written_phones: Vec<String>,
    /// What its source says they do.
    pub roles: Vec<String>,
    /// Whether only lists of people the DSP can reach name it, as Slack's members: on one, it
    /// says nothing of whether they still work for the DSP.
    pub contacts_only: bool,
    /// The version of the photo held of them.
    pub photo: Option<String>,
    /// The source's other IDs for the same person, which go wherever this one does.
    pub same_as: BTreeSet<String>,
}
impl Identity {
    pub fn new(source: DriverSource, id: &str) -> Self {
        Self {
            source,
            id: id.to_owned(),
            names: vec![],
            department: None,
            position: None,
            first_seen: None,
            last_seen: None,
            listed: false,
            emails: BTreeSet::new(),
            phones: BTreeSet::new(),
            written_emails: vec![],
            written_phones: vec![],
            roles: vec![],
            contacts_only: true,
            photo: None,
            same_as: BTreeSet::new(),
        }
    }
    fn named(&mut self, name: &str) {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if !name.is_empty() && !self.names.contains(&name) {
            self.names.push(name);
        }
    }
    /// Extends the days it was seen by those a kind of data saw it.
    fn seen(&mut self, first: Option<String>, last: Option<String>) {
        if let Some(first) = first
            && self
                .first_seen
                .as_deref()
                .is_none_or(|f| first.as_str() < f)
        {
            self.first_seen = Some(first);
        }
        if let Some(last) = last
            && self.last_seen.as_deref().is_none_or(|l| last.as_str() > l)
        {
            self.last_seen = Some(last);
        }
    }
    /// The name to show and store: the first, most readable spelling.
    pub fn name(&self) -> &str {
        self.names.first().map_or("", String::as_str)
    }
    /// Whether its source says it drives or helps.
    pub fn drives(&self) -> bool {
        self.roles
            .iter()
            .any(|role| role == "Driver" || role == "Helper")
    }
}

/// Every ID the DSP's collections hold, of the features it has on, as they say: each kind of
/// data's spellings read in the kinds' order, the first kept first (the README's table).
pub(crate) fn identities(
    people: &[&'static People],
    store: &Store,
    dsp: &str,
) -> Result<Vec<Identity>> {
    let mut found: Found = BTreeMap::new();
    for people in people {
        let source = people.source();
        for named in (people.read)(store, dsp)? {
            let id = named.id.trim();
            if id.is_empty() {
                continue;
            }
            let entry = found
                .entry((source, id.to_owned()))
                .or_insert_with(|| Identity::new(source, id));
            entry.named(&named.name);
            entry.department = entry.department.take().or(named.department);
            entry.position = entry.position.take().or(named.position);
            entry.seen(named.first_seen, named.last_seen);
            entry.listed |= named.listed;
            entry.contacts_only &= people.contacts_only;
            entry.photo = entry.photo.take().or(named.photo);
            entry
                .same_as
                .extend(named.same_as.into_iter().filter(|other| other != id));
            for email in &named.emails {
                if let Some(written) = contacts::email(email)
                    && entry.emails.insert(written)
                {
                    entry.written_emails.push(email.trim().to_owned());
                }
            }
            // A number that isn't North American is shown, though nobody is matched by it.
            for phone in &named.phones {
                let written = phone.trim().to_owned();
                let new = match contacts::phone(phone) {
                    Some(digits) => entry.phones.insert(digits),
                    None => !written.is_empty() && !entry.written_phones.contains(&written),
                };
                if new {
                    entry.written_phones.push(written);
                }
            }
            for role in named.roles {
                if !entry.roles.contains(&role) {
                    entry.roles.push(role);
                }
            }
        }
    }
    Ok(found
        .into_values()
        .filter(|identity| !identity.names.is_empty())
        .collect())
}

/// Every ID the DSP's data holds now, by its key, as `identities` reads them.
pub(crate) fn found(people: &[&'static People], store: &Store, dsp: &str) -> Result<Found> {
    Ok(identities(people, store, dsp)?
        .into_iter()
        .map(|x| ((x.source, x.id.clone()), x))
        .collect())
}

/// How much a source has of one ID, and the last day it has.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seen {
    pub count: usize,
    pub last: Option<String>,
}
/// Everything each ID appears in. Only current publications count, as every page reads.
pub(crate) fn activity(
    people: &[&'static People],
    store: &Store,
    dsp: &str,
) -> Result<BTreeMap<Key, BTreeMap<DriverData, Seen>>> {
    let mut out: BTreeMap<Key, BTreeMap<DriverData, Seen>> = BTreeMap::new();
    for people in people {
        let Some(records) = people.records else {
            continue;
        };
        for row in records(store, dsp)? {
            out.entry((people.source(), row.id)).or_default().insert(
                people.data(),
                Seen {
                    count: row.count,
                    last: Some(row.last),
                },
            );
        }
    }
    Ok(out)
}

/// The days each ID worked, and the days each source was collected for at all: a day
/// outside them says nothing either way.
#[derive(Debug, Default)]
pub(crate) struct Days {
    pub worked: BTreeMap<Key, BTreeSet<String>>,
    pub collected: BTreeMap<DriverSource, BTreeSet<String>>,
}
impl Days {
    pub fn of(&self, key: &Key) -> Option<&BTreeSet<String>> {
        self.worked.get(key)
    }
}
/// When each source shows people at work: Paycom's hours; Amazon's routes, meal breaks and
/// inspections.
pub(crate) fn days(people: &[&'static People], store: &Store, dsp: &str) -> Result<Days> {
    let mut out = Days::default();
    for people in people {
        let Some(workdays) = people.workdays else {
            continue;
        };
        let source = people.source();
        let Workdays { worked, collected } = workdays(store, dsp)?;
        out.collected.entry(source).or_default().extend(collected);
        for (id, day) in worked {
            out.worked.entry((source, id)).or_default().insert(day);
        }
    }
    out.collected.retain(|_, days| !days.is_empty());
    Ok(out)
}

/// The departments each source's settings count as drivers: Paycom's, from the DSP's
/// Timecard settings. Everyone only such sources know outside them is office staff; without
/// the setting, nobody is.
pub(crate) type DriverDepartments = BTreeMap<DriverSource, BTreeSet<String>>;
pub(crate) fn driver_departments(
    people: &[&'static People],
    store: &Store,
    dsp: &str,
) -> Result<DriverDepartments> {
    let mut out = DriverDepartments::new();
    for people in people {
        if let Some(departments) = people.driver_departments
            && let Some(list) = departments(store, dsp)?
        {
            out.insert(people.source(), list);
        }
    }
    Ok(out)
}
