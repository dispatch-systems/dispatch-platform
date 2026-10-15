//! What the tab and the details panel read: everyone with a code, as the sources the DSP has
//! on describe them, what each does for the DSP, how surely their IDs are one person's, and
//! the pairs that may be one person, gathered once.
use super::matching::{AMAZON, PAYCOM, Saved};
use super::review;
use super::sources::{self, Days, DriverDepartments, Identity, Key, Seen};
use super::{CHECKED, FORMER_AFTER_DAYS, IdRow, saved_links};
use crate::api::types::{
    Driver, DriverCounts, DriverEvidence, DriverId, DriverLink, DriverMatch, DriverPair,
    DriverRole, DriverStrength,
};
use crate::backend::storage::DriverProfilesStore;
use dispatch_core::{
    Result,
    db::Store,
    foundation::names,
    manifest::people::{DriverData, DriverSource},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Every ID the collections hold now, as they describe it.
pub(crate) type Found = BTreeMap<Key, Identity>;

/// Everything the tab and the details panel read, gathered once.
pub(crate) struct Overview {
    pub result: DriverMatch,
    /// Every ID of the sources on, with its person.
    pub rows: Vec<IdRow>,
    pub found: Found,
    pub activity: BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    pub days: Days,
}

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
            emails: x.map(|x| x.written.0.clone()).unwrap_or_default(),
            phones: x.map(|x| x.written.1.clone()).unwrap_or_default(),
            roles: x.map(|x| x.roles.clone()).unwrap_or_default(),
            listed: x.is_some_and(|x| x.listed),
            first_seen: x.and_then(|x| x.first_seen.clone()),
            last_seen: x.and_then(|x| x.last_seen.clone()),
            linked_by: self.linked_by,
            linked_at: self.linked_at.clone(),
        }
    }
}

/// What a person is drawn from.
struct Context<'a> {
    found: &'a Found,
    activity: &'a BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    departments: &'a DriverDepartments,
    photos: &'a BTreeMap<String, String>,
    /// The first day that still counts as recent.
    recent: &'a str,
}

/// A person as the tab lists them, before review marks the ones in a pair.
fn driver(code: &str, ids: &[(&IdRow, DriverSource)], context: &Context) -> Driver {
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
    // Someone no source lists, and nothing has collected lately, has left. Nobody can match
    // them, so they wait on no one.
    let gone = !described.iter().any(|x| x.listed)
        && last_seen
            .as_deref()
            .is_none_or(|last| last < context.recent);
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
    let in_office =
        !drives && !counted.is_empty() && counted.iter().all(|x| office(x, context.departments));
    let roles: BTreeSet<&str> = described
        .iter()
        .flat_map(|x| x.roles.iter().map(String::as_str))
        .collect();
    let role = if gone {
        DriverRole::Former
    } else if in_office {
        DriverRole::Office
    } else if roles.contains("Helper") && !roles.contains("Driver") {
        DriverRole::Helper
    } else {
        DriverRole::Driver
    };
    let link = ids
        .iter()
        .map(|(row, _)| row.linked_by)
        .filter(|link| *link != DriverLink::New)
        .min_by_key(|link| sureness(*link))
        .unwrap_or(DriverLink::New);
    // Paycom's name is the legal one; the next source's stands in when Paycom has none, and
    // an ID no source holds any more when none does.
    let name = ids
        .iter()
        .filter_map(|(row, source)| {
            let x = context.found.get(&(*source, row.external_id.clone()))?;
            Some((*source, names::display(x.name())))
        })
        .min_by_key(|(source, _)| (source.as_str() != PAYCOM, *source))
        .map(|(_, name)| name)
        .or_else(|| ids.first().map(|(row, _)| row.external_id.clone()))
        .unwrap_or_default();
    let photo = ids
        .iter()
        .filter(|(_, source)| source.as_str() == AMAZON)
        .find_map(|(row, _)| context.photos.get(&row.external_id).cloned());
    Driver {
        code: code.to_owned(),
        name,
        role,
        link,
        review: false,
        ids: ids
            .iter()
            .map(|(row, source)| row.public(*source, context.found))
            .collect(),
        appears: DriverData::all()
            .filter(|d| seen.iter().any(|data| data.contains_key(d)))
            .collect(),
        last_seen,
        photo,
    }
}

/// The days a person worked, and the days the sources that know them were collected for.
fn worked(
    ids: &[(&IdRow, DriverSource)],
    days: &Days,
) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let mut worked = BTreeSet::new();
    let mut collected = BTreeSet::new();
    let mut any = false;
    for (row, source) in ids {
        if let Some(set) = days.of(&(*source, row.external_id.clone())) {
            worked.extend(set.iter().cloned());
            any = true;
        }
        if let Some(set) = days.collected.get(source) {
            collected.extend(set.iter().cloned());
        }
    }
    any.then_some((worked, collected))
}

pub(crate) fn overview(store: &Store, dsp: &str) -> Result<Overview> {
    let people = sources::people(store, dsp)?;
    let active = sources::active(&people);
    let found: Found = sources::identities(&people, store, dsp)?
        .into_iter()
        .map(|x| ((x.source, x.id.clone()), x))
        .collect();
    let activity = sources::activity(&people, store, dsp)?;
    let days = sources::days(&people, store, dsp)?;
    let saved = saved_links(store, dsp)?;
    let departments = sources::driver_departments(&people, store, dsp)?;
    let photos = store.photo_versions(dsp)?;
    // Counted from what was collected, not from today, so a DSP whose collections pause
    // does not see everyone leave.
    let recent = found
        .values()
        .filter_map(|x| x.last_seen.as_deref())
        .chain(
            activity
                .values()
                .flat_map(|data| data.values().filter_map(|s| s.last.as_deref())),
        )
        .filter_map(|day| chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .max()
        .map(|newest| (newest - chrono::Duration::days(FORMER_AFTER_DAYS)).to_string())
        .unwrap_or_default();
    let db = store.dsp(dsp)?;
    // IDs of a source no feature on names sit idle: kept, and shown nowhere.
    let rows: Vec<IdRow> = db
        .query_as::<IdRow>(
            "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.source,i.external_id",
            [],
        )?
        .into_iter()
        .filter(|row| row.source().is_some_and(|source| active.contains(&source)))
        .collect();
    let apart: BTreeSet<(String, String)> = db
        .query_as::<(String, String)>("SELECT first,second FROM people_apart", [])?
        .into_iter()
        .collect();
    let checked_at = db
        .setting(CHECKED, Value::Null)?
        .as_str()
        .map(str::to_owned);
    drop(db);
    // Each person's IDs by their sources' order, then as they came.
    let mut held: BTreeMap<&str, Vec<(&IdRow, DriverSource)>> = BTreeMap::new();
    for row in &rows {
        if let Some(source) = row.source() {
            held.entry(&row.code).or_default().push((row, source));
        }
    }
    for ids in held.values_mut() {
        ids.sort_by_key(|(row, source)| (*source, row.linked_at.clone()));
    }
    let context = Context {
        found: &found,
        activity: &activity,
        departments: &departments,
        photos: &photos,
        recent: &recent,
    };
    let mut drivers: BTreeMap<String, Driver> = held
        .iter()
        .map(|(code, ids)| ((*code).to_owned(), driver(code, ids, &context)))
        .collect();
    let pairs = pairs(&held, &drivers, &found, &days, &saved, &apart);
    for (_, _, person, candidate, _) in &pairs {
        for code in [person, candidate] {
            drivers.get_mut(code).unwrap().review = true;
        }
    }
    let review: Vec<DriverPair> = pairs
        .into_iter()
        .map(|(strength, _, person, candidate, evidence)| DriverPair {
            person: drivers[&person].clone(),
            candidate: drivers[&candidate].clone(),
            strength,
            evidence,
        })
        .collect();
    let mut list: Vec<Driver> = drivers.into_values().collect();
    list.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
    let count = |role: DriverRole| list.iter().filter(|d| d.role == role).count();
    let working = |d: &&Driver| matches!(d.role, DriverRole::Driver | DriverRole::Helper);
    let counts = DriverCounts {
        people: list.len(),
        review: review.len(),
        unconfirmed: list
            .iter()
            .filter(|d| d.link == DriverLink::Variant)
            .count(),
        missing: list
            .iter()
            .filter(working)
            .filter(|d| active.iter().any(|s| d.ids.iter().all(|i| i.source != *s)))
            .count(),
        drivers: count(DriverRole::Driver),
        helpers: count(DriverRole::Helper),
        office: count(DriverRole::Office),
        former: count(DriverRole::Former),
    };
    Ok(Overview {
        result: DriverMatch {
            checked_at,
            sources: active,
            counts,
            review,
            drivers: list,
        },
        rows,
        found,
        activity,
        days,
    })
}

type Pair = (DriverStrength, usize, String, String, Vec<DriverEvidence>);
/// The pairs that may be one person, the strongest first, and no more than three choices for
/// any one person. Each pair's `person` holds fewer IDs, or a source later in the order:
/// joining them keeps the `candidate`'s code, so a code follows the employment record.
fn pairs(
    held: &BTreeMap<&str, Vec<(&IdRow, DriverSource)>>,
    drivers: &BTreeMap<String, Driver>,
    found: &Found,
    days: &Days,
    saved: &Saved,
    apart: &BTreeSet<(String, String)>,
) -> Vec<Pair> {
    // Someone kept apart from Paycom on the meal-break page; that decision stands.
    let kept = |ids: &[(&IdRow, DriverSource)]| {
        ids.iter().any(|(row, source)| {
            source.as_str() == AMAZON && saved.separate.contains(&row.external_id)
        })
    };
    let sides: Vec<review::Side> = held
        .iter()
        .filter(|(_, ids)| !kept(ids))
        .map(|(code, ids)| {
            let described: Vec<&Identity> = ids
                .iter()
                .filter_map(|(row, source)| found.get(&(*source, row.external_id.clone())))
                .collect();
            review::Side {
                code,
                sources: ids.iter().map(|(_, source)| *source).collect(),
                // Every name an ID's sources write, so a middle name or a different spelling
                // in one of them still counts.
                names: described
                    .iter()
                    .flat_map(|x| x.names.iter().map(String::as_str))
                    .collect(),
                emails: described
                    .iter()
                    .flat_map(|x| x.emails.iter().map(String::as_str))
                    .collect(),
                phones: described
                    .iter()
                    .flat_map(|x| x.phones.iter().map(String::as_str))
                    .collect(),
            }
        })
        .collect();
    let mut pairs: Vec<Pair> = review::suggest(&sides, apart)
        .into_iter()
        .map(|mut suggestion| {
            let order = |code: &str| {
                let ids = &held[code];
                let latest = ids.iter().map(|(_, source)| *source).max();
                (std::cmp::Reverse(ids.len()), latest)
            };
            let (person, candidate) = if order(&suggestion.first) > order(&suggestion.second) {
                (suggestion.first, suggestion.second)
            } else {
                (suggestion.second, suggestion.first)
            };
            if let (Some((drove, drove_days)), Some((clocked, clocked_days))) = (
                worked(&held[person.as_str()], days),
                worked(&held[candidate.as_str()], days),
            ) && let Some(days) =
                review::worked_days(Some(&clocked), Some(&drove), &clocked_days, &drove_days)
            {
                suggestion.evidence.push(days);
            }
            let strength = review::strength(&suggestion.evidence);
            (
                strength,
                suggestion.evidence.len(),
                person,
                candidate,
                suggestion.evidence,
            )
        })
        .collect();
    pairs.sort_by(|a, b| {
        (a.0 != DriverStrength::Strong)
            .cmp(&(b.0 != DriverStrength::Strong))
            .then(b.1.cmp(&a.1))
            .then_with(|| names::compare(&drivers[&a.2].name, &drivers[&b.2].name))
    });
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    pairs.retain(|pair| {
        let count = shown.entry(pair.2.clone()).or_default();
        *count += 1;
        *count <= 3
    });
    pairs
}
