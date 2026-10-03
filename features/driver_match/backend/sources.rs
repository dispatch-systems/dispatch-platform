//! What each source knows about the people in it: the IDs, the names it writes and the
//! days it saw them, as each feature whose data names people says through the people slot.
//! Read afresh each time; Driver Match stores only what was decided.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{people::Workdays, registry},
    mcp::api::types::{DriverData, DriverSource},
};
use std::collections::{BTreeMap, BTreeSet};

/// An ID as one source knows it.
pub(crate) type Key = (DriverSource, String);

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
    /// On Paycom's current roster as an active employee.
    pub listed: bool,
}
impl Identity {
    fn new(source: DriverSource, id: &str) -> Self {
        Self {
            source,
            id: id.to_owned(),
            names: vec![],
            department: None,
            position: None,
            first_seen: None,
            last_seen: None,
            listed: false,
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
}

/// Every ID the DSP's collections hold: Paycom's employees, then every driver Amazon's
/// sources name, as the features that keep them say. Each kind of data's names are read in
/// its place: routes first among Amazon's, as they spell names most plainly; the scorecard
/// often adds a middle name and DVIC sometimes only a last initial.
pub(crate) fn identities(store: &Store, dsp: &str) -> Result<Vec<Identity>> {
    let mut found: BTreeMap<Key, Identity> = BTreeMap::new();
    for people in registry().people() {
        let source = people.source();
        for named in people.named(store, dsp)? {
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
        }
    }
    Ok(found
        .into_values()
        .filter(|identity| !identity.names.is_empty())
        .collect())
}

/// The name one ID's sources write now, as `identities` would read it first: Paycom's
/// newest roster, or Amazon's routes, then meal breaks, the scorecard and DVIC.
pub(crate) fn name_of(store: &Store, dsp: &str, (source, id): &Key) -> Result<Option<String>> {
    for people in registry().people() {
        if people.source() == *source
            && let Some(name) = people.name(store, dsp, id)?
        {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// How much a source has of one ID, and the last day it has.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seen {
    pub count: usize,
    pub last: Option<String>,
}
/// Everything each ID appears in. Only current publications count, as every page reads.
pub(crate) fn activity(
    store: &Store,
    dsp: &str,
) -> Result<BTreeMap<Key, BTreeMap<DriverData, Seen>>> {
    let mut out: BTreeMap<Key, BTreeMap<DriverData, Seen>> = BTreeMap::new();
    for people in registry().people() {
        for row in people.appearances(store, dsp)? {
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

/// The days each ID worked, and the days its source was collected for at all: a day
/// outside them says nothing either way.
#[derive(Debug, Default)]
pub(crate) struct Days {
    pub worked: BTreeMap<Key, BTreeSet<String>>,
    pub paycom: BTreeSet<String>,
    pub amazon: BTreeSet<String>,
}
impl Days {
    pub fn of(&self, key: &Key) -> Option<&BTreeSet<String>> {
        self.worked.get(key)
    }
}
/// When Paycom shows hours and when Amazon shows a route, a meal break or an inspection.
pub(crate) fn days(store: &Store, dsp: &str) -> Result<Days> {
    let mut out = Days::default();
    for people in registry().people() {
        let source = people.source();
        let Workdays { worked, collected } = people.workdays(store, dsp)?;
        if source == DriverSource::Paycom {
            out.paycom.extend(collected);
        } else {
            out.amazon.extend(collected);
        }
        for (id, day) in worked {
            out.worked.entry((source, id)).or_default().insert(day);
        }
    }
    Ok(out)
}

/// The departments each source's settings count as drivers: Paycom's, from the DSP's
/// Timecard settings. Everyone only that source knows outside them is office staff;
/// without the setting, nobody is.
pub(crate) type DriverDepartments = BTreeMap<DriverSource, BTreeSet<String>>;
pub(crate) fn driver_departments(store: &Store, dsp: &str) -> Result<DriverDepartments> {
    let mut out = DriverDepartments::new();
    for people in registry().people() {
        if let Some(list) = people.driver_departments(store, dsp)? {
            out.insert(people.source(), list);
        }
    }
    Ok(out)
}
