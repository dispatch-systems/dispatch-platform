//! What each source knows about the people in it: the IDs, the names it writes, the contacts
//! it has and the days it saw them, as each feature switched on for the DSP whose data names
//! people says through the people slot. A feature switched off says nothing, and its IDs sit
//! idle until it is back. Read afresh each time; Driver Match stores only what was decided.
use super::contacts::{Contact, Contacts};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{
        people::{Kind, People, Source, Workdays},
        registry,
    },
};
use std::collections::{BTreeMap, BTreeSet};

/// An ID as one source knows it.
pub(crate) type Key = (Source, String);
/// Every ID the DSP's data holds now, as its sources describe it.
pub(crate) type Found = BTreeMap<Key, Identity>;

/// Every kind of data that names people of the features the DSP has on, in the order their
/// spellings are read.
pub(crate) fn kinds(store: &Store, dsp: &str) -> Result<Vec<&'static People>> {
    let on = store.features(dsp)?;
    Ok(registry().people_of(|feature| on.iter().any(|id| id == feature.switch.id)))
}
/// The kinds whose records show someone driving (`.driving()`).
pub(crate) fn driving(kinds: &[&'static People]) -> BTreeSet<Kind> {
    kinds
        .iter()
        .filter(|kind| kind.driving)
        .map(|kind| kind.kind())
        .collect()
}
/// The sources only lists of people the DSP can reach name, as Slack's members. An ID such a
/// source no longer names went with its list, as a workspace disconnected: it sits idle, shown
/// nowhere and keeping nobody from another, and comes back with its list.
pub(crate) fn contacts_only(kinds: &[&'static People]) -> BTreeSet<Source> {
    let employs: BTreeSet<Source> = kinds
        .iter()
        .filter(|kind| !kind.contacts_only)
        .map(|kind| kind.source)
        .collect();
    kinds
        .iter()
        .map(|kind| kind.source)
        .filter(|source| !employs.contains(source))
        .collect()
}
/// The DSP's systems, in their order: the sources of the features it has on that name someone
/// now, or that someone holds an ID of. One that has named nobody yet, as a workspace whose
/// admin hasn't let Dispatch read its members, is none of its systems.
pub(crate) fn active(
    kinds: &[&'static People],
    found: &Found,
    held: impl IntoIterator<Item = Source>,
) -> Vec<Source> {
    let named: BTreeSet<Source> = found
        .keys()
        .map(|(source, _)| *source)
        .chain(held)
        .collect();
    let set: BTreeSet<Source> = kinds
        .iter()
        .map(|kind| kind.source)
        .filter(|source| named.contains(source))
        .collect();
    set.into_iter().collect()
}

/// One ID as its sources describe it. Each kind of data may write a name its own way; `names`
/// keeps every spelling once, the most readable first.
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub source: Source,
    pub id: String,
    pub names: Vec<String>,
    pub department: Option<String>,
    pub position: Option<String>,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On a source's current roster.
    pub listed: bool,
    /// Its contacts, to match by and to show.
    pub contacts: Contacts,
    /// What its source says they do.
    pub roles: Vec<String>,
    /// Whether its source only lists people the DSP can reach (`contacts_only`): it says
    /// nothing of whether they still work for the DSP.
    pub contacts_only: bool,
    /// The version of the photo held of them.
    pub photo: Option<String>,
    /// The source's other IDs for the same person, which go wherever this one does.
    pub same_as: BTreeSet<String>,
}
impl Identity {
    pub fn new(source: Source, id: &str) -> Self {
        Self {
            source,
            id: id.to_owned(),
            names: vec![],
            department: None,
            position: None,
            first_seen: None,
            last_seen: None,
            listed: false,
            contacts: Contacts::default(),
            roles: vec![],
            contacts_only: false,
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
    /// The person its source says it is, by the first of their IDs there: the same for every
    /// one of them.
    pub fn one(&self) -> &str {
        self.same_as
            .iter()
            .map(String::as_str)
            .chain([self.id.as_str()])
            .min()
            .unwrap_or_default()
    }
}

/// Every ID the DSP's collections hold, of the features it has on, as they say: each kind of
/// data's spellings read in the kinds' order, the first kept first (the README's table).
pub(crate) fn found(kinds: &[&'static People], store: &Store, dsp: &str) -> Result<Found> {
    let contacts_only = contacts_only(kinds);
    let mut found: Found = BTreeMap::new();
    for kind in kinds {
        let source = kind.source;
        for named in (kind.read)(store, dsp)? {
            let id = named.id.trim();
            if id.is_empty() {
                continue;
            }
            let entry = found
                .entry((source, id.to_owned()))
                .or_insert_with(|| Identity {
                    contacts_only: contacts_only.contains(&source),
                    ..Identity::new(source, id)
                });
            entry.named(&named.name);
            entry.department = entry.department.take().or(named.department);
            entry.position = entry.position.take().or(named.position);
            entry.seen(named.first_seen, named.last_seen);
            entry.listed |= named.listed;
            // A photo counts from a kind that serves its photos.
            if kind.photos.is_some() {
                entry.photo = entry.photo.take().or(named.photo);
            }
            entry
                .same_as
                .extend(named.same_as.into_iter().filter(|other| other != id));
            for email in &named.emails {
                entry.contacts.add(Contact::Email, email);
            }
            for phone in &named.phones {
                entry.contacts.add(Contact::Phone, phone);
            }
            for role in named.roles {
                if !entry.roles.contains(&role) {
                    entry.roles.push(role);
                }
            }
        }
    }
    found.retain(|_, identity| !identity.names.is_empty());
    Ok(found)
}

/// How much a source has of one ID, and the last day it has.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seen {
    pub count: usize,
    pub last: Option<String>,
}
/// Everything each ID appears in. Only current publications count, as every page reads.
pub(crate) fn activity(
    kinds: &[&'static People],
    store: &Store,
    dsp: &str,
) -> Result<BTreeMap<Key, BTreeMap<Kind, Seen>>> {
    let mut out: BTreeMap<Key, BTreeMap<Kind, Seen>> = BTreeMap::new();
    for kind in kinds {
        let Some((_, records)) = kind.records else {
            continue;
        };
        for row in records(store, dsp)? {
            out.entry((kind.source, row.id)).or_default().insert(
                kind.kind(),
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
    pub collected: BTreeMap<Source, BTreeSet<String>>,
}
impl Days {
    pub fn of(&self, key: &Key) -> Option<&BTreeSet<String>> {
        self.worked.get(key)
    }
}
/// When each source shows people at work, from every kind of data that says (`.workdays`).
pub(crate) fn days(kinds: &[&'static People], store: &Store, dsp: &str) -> Result<Days> {
    let mut out = Days::default();
    for kind in kinds {
        let Some(workdays) = kind.workdays else {
            continue;
        };
        let source = kind.source;
        let Workdays { worked, collected } = workdays(store, dsp)?;
        out.collected.entry(source).or_default().extend(collected);
        for (id, day) in worked {
            out.worked.entry((source, id)).or_default().insert(day);
        }
    }
    out.collected.retain(|_, days| !days.is_empty());
    Ok(out)
}

/// The departments each source's settings count as drivers (`.driver_departments`). Everyone
/// only such sources know outside them is office staff; without the setting, nobody is.
pub(crate) type DriverDepartments = BTreeMap<Source, BTreeSet<String>>;
pub(crate) fn driver_departments(
    kinds: &[&'static People],
    store: &Store,
    dsp: &str,
) -> Result<DriverDepartments> {
    let mut out = DriverDepartments::new();
    for kind in kinds {
        if let Some(departments) = kind.driver_departments
            && let Some(list) = departments(store, dsp)?
        {
            out.insert(kind.source, list);
        }
    }
    Ok(out)
}
