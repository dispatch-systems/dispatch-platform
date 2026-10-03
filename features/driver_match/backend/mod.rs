//! Driver Match: one code for each person the DSP's collections know, whatever each
//! source calls them. Collected rows keep their sources' IDs; `person_ids` leads every ID
//! to its person. New IDs get a person after every collection, joining someone only on a
//! certain match; anything less waits for a decision in Settings.
#[path = "maintenance.rs"]
pub mod maintenance;
#[path = "matching.rs"]
mod matching;
#[path = "review.rs"]
mod review;
#[path = "sources.rs"]
mod sources;

use crate::{
    Error, Result,
    accounts::Context,
    contracts::{
        Driver, DriverActivity, DriverCounts, DriverData, DriverDay, DriverDetails, DriverEvent,
        DriverEventKind, DriverId, DriverLink, DriverMatch, DriverPair, DriverSource, DriverStatus,
        DriverStrength,
    },
    crypto,
    db::{Db, FromRow, Row, Store, iso},
    ensure, names,
    read_cache::DataDomain,
};
use matching::{Member, Saved};
use rusqlite::params;
use serde_json::{Value, json};
use sources::{Days, DriverDepartments, Identity, Key, Seen};
use std::collections::{BTreeMap, BTreeSet};

/// Driver codes, the decisions about them and the IDs they lead to.
pub const DOMAIN: DataDomain = DataDomain::new("drivers");
/// When collected data was last checked for new IDs.
const CHECKED: &str = "driver_match.checked_at";
/// Links saved on the meal-break page before Driver Match, which reads them as decisions and
/// writes its own back for the previous release.
pub(crate) const LINKS: &str = "employees.provider_links";
/// Letters and digits that cannot be mistaken for one another, without U, so a code
/// rarely spells a word.
const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 6;
/// How long someone only one source knows can go unseen, off Paycom's roster, before
/// they count as having left: days before the newest day anything was collected for.
const FORMER_AFTER_DAYS: i64 = 21;
/// The data a person can appear in, in the order the tab shows it.
const DATA: [DriverData; 5] = [
    DriverData::Timecards,
    DriverData::Routes,
    DriverData::MealBreaks,
    DriverData::Dvic,
    DriverData::Scorecard,
];

pub fn valid_code(code: &str) -> bool {
    code.len() == CODE_LENGTH && code.bytes().all(|b| ALPHABET.contains(&b))
}

/// Whether someone works in the office: outside the departments their source's settings
/// count as drivers.
fn office(x: &Identity, drivers: &DriverDepartments) -> bool {
    drivers
        .get(&x.source)
        .is_some_and(|list| !list.contains(x.department.as_deref().unwrap_or("")))
}

/// What one pass reads: every ID the collections hold and the links saved on the meal-break
/// page. Reading takes the shared lock; `assign_drivers` then writes in one short step.
pub struct DriverSources {
    identities: Vec<sources::Identity>,
    saved: Saved,
}

/// A row of `person_ids`: an ID, its person and how it came to them. Nothing a source
/// says about the person is kept here; that stays in the source's own database.
struct IdRow {
    source: DriverSource,
    external_id: String,
    code: String,
    linked_by: DriverLink,
    linked_at: String,
    actor_id: Option<String>,
}
impl FromRow for IdRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            source: row.get("source")?,
            external_id: row.get("external_id")?,
            code: row.get("code")?,
            linked_by: row.get("linked_by")?,
            linked_at: row.get("linked_at")?,
            actor_id: row.get("actor_id")?,
        })
    }
}
/// Every ID the collections hold now, as they describe it.
type Found = BTreeMap<Key, Identity>;
impl IdRow {
    fn key(&self) -> Key {
        (self.source, self.external_id.clone())
    }
    /// The ID as its sources describe it now; one no source holds any more shows bare.
    fn public(&self, found: &Found) -> DriverId {
        let x = found.get(&self.key());
        DriverId {
            source: self.source,
            id: self.external_id.clone(),
            name: x.map(|x| x.name().to_owned()).unwrap_or_default(),
            department: x.and_then(|x| x.department.clone()),
            position: x.and_then(|x| x.position.clone()),
            first_seen: x.and_then(|x| x.first_seen.clone()),
            last_seen: x.and_then(|x| x.last_seen.clone()),
            linked_by: self.linked_by,
            linked_at: self.linked_at.clone(),
        }
    }
}

/// Everything the tab and the details panel read, gathered once.
struct Overview {
    result: DriverMatch,
    rows: Vec<IdRow>,
    found: Found,
    activity: BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    days: Days,
}

/// A person as the tab lists them, before review marks the ones in a pair. `recent` is
/// the first day that still counts as recent.
fn driver(
    code: &str,
    ids: &[&IdRow],
    activity: &BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    departments: &DriverDepartments,
    found: &Found,
    recent: &str,
) -> Driver {
    let paycom: Vec<&&IdRow> = ids
        .iter()
        .filter(|i| i.source == DriverSource::Paycom)
        .collect();
    let amazon = ids.iter().any(|i| i.source == DriverSource::Amazon);
    let seen: Vec<&BTreeMap<DriverData, Seen>> =
        ids.iter().filter_map(|i| activity.get(&i.key())).collect();
    let last_seen = seen
        .iter()
        .flat_map(|data| data.values().filter_map(|s| s.last.clone()))
        .chain(
            ids.iter()
                .filter_map(|i| found.get(&i.key())?.last_seen.clone()),
        )
        .max();
    // Someone only one source knows who has left: off Paycom's roster, and in nothing
    // collected lately. Nobody can match them, so they wait on no one.
    let gone = !ids
        .iter()
        .any(|i| found.get(&i.key()).is_some_and(|x| x.listed))
        && last_seen.as_deref().is_none_or(|last| last < recent);
    let status = if !paycom.is_empty() && amazon {
        if ids
            .iter()
            .any(|i| matches!(i.linked_by, DriverLink::Person | DriverLink::Saved))
        {
            DriverStatus::Confirmed
        } else if ids.iter().any(|i| i.linked_by == DriverLink::Variant) {
            DriverStatus::Variant
        } else {
            DriverStatus::Matched
        }
    } else if gone {
        DriverStatus::Former
    } else if amazon {
        DriverStatus::AmazonOnly
    } else if paycom
        .iter()
        .all(|i| found.get(&i.key()).is_some_and(|x| office(x, departments)))
    {
        DriverStatus::Office
    } else {
        DriverStatus::PaycomOnly
    };
    // Paycom's name is the legal one; Amazon's stands in when Paycom has none, and an ID
    // no source holds any more when neither does.
    let written = |source: DriverSource| {
        ids.iter()
            .filter(|i| i.source == source)
            .find_map(|i| found.get(&i.key()))
            .map(|x| names::display(x.name()))
    };
    let name = written(DriverSource::Paycom)
        .or_else(|| written(DriverSource::Amazon))
        .or_else(|| ids.first().map(|i| i.external_id.clone()))
        .unwrap_or_default();
    Driver {
        code: code.to_owned(),
        name,
        status,
        ids: ids.iter().map(|i| i.public(found)).collect(),
        appears: DATA
            .into_iter()
            .filter(|d| seen.iter().any(|data| data.contains_key(d)))
            .collect(),
        last_seen,
    }
}

/// What Driver Match reads and writes for a DSP: its codes, the decisions about them and the
/// IDs they lead to.
pub trait DriverMatchStore {
    fn match_drivers(&self, dsp: &str) -> Result<usize>;
    fn driver_sources(&self, dsp: &str) -> Result<DriverSources>;
    fn assign_drivers(&self, dsp: &str, found: DriverSources) -> Result<usize>;
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch>;
    fn driver_counts(&self, dsp: &str) -> Result<DriverCounts>;
    fn driver_details(&self, dsp: &str, code: &str) -> Result<DriverDetails>;
    fn driver_links(&self, dsp: &str) -> Result<BTreeMap<String, (Vec<String>, bool)>>;
    fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch>;
    fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch>;
    fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch>;
}
impl DriverMatchStore for Store {
    /// Gives every new ID in the DSP's collections a person. Answers how many IDs were new.
    fn match_drivers(&self, dsp: &str) -> Result<usize> {
        self.assign_drivers(dsp, self.driver_sources(dsp)?)
    }
    /// Reads what a pass needs, without writing: run it under the shared lock.
    fn driver_sources(&self, dsp: &str) -> Result<DriverSources> {
        Ok(DriverSources {
            identities: sources::identities(self, dsp)?,
            saved: saved_links(self, dsp)?,
        })
    }
    /// Writes what a pass found: a person for every new ID. Who already holds what is read
    /// again here, so a decision made since the read stands.
    fn assign_drivers(&self, dsp: &str, found: DriverSources) -> Result<usize> {
        let DriverSources { identities, saved } = found;
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let rows: Vec<IdRow> = db.query_as("SELECT * FROM person_ids", [])?;
            let known: BTreeSet<Key> = rows.iter().map(IdRow::key).collect();
            let names: BTreeMap<Key, &Vec<String>> = identities
                .iter()
                .map(|x| ((x.source, x.id.clone()), &x.names))
                .collect();
            let incoming: Vec<Identity> = identities
                .iter()
                .filter(|x| !known.contains(&(x.source, x.id.clone())))
                .cloned()
                .collect();
            // An ID no source holds any more has no names to match by.
            let mut people: BTreeMap<String, Vec<Member>> = BTreeMap::new();
            for row in &rows {
                people.entry(row.code.clone()).or_default().push(Member {
                    source: row.source,
                    id: row.external_id.clone(),
                    names: names
                        .get(&row.key())
                        .map(|n| (*n).clone())
                        .unwrap_or_default(),
                });
            }
            let decided = matching::plan(people, &incoming, &saved);
            let now = iso();
            let mut fresh: BTreeMap<String, String> = BTreeMap::new();
            for (index, person, link) in &decided {
                let code = if !person.starts_with('+') {
                    person.clone()
                } else if let Some(code) = fresh.get(person) {
                    code.clone()
                } else {
                    let code = new_person(self, &db, dsp, &now, None, None)?;
                    fresh.insert(person.clone(), code.clone());
                    code
                };
                let x = &incoming[*index];
                db.exec(
                    "INSERT INTO person_ids(source,external_id,code,linked_by,linked_at) \
                     VALUES (?,?,?,?,?)",
                    params![x.source, x.id, code, link, now],
                )?;
            }
            db.set(CHECKED, &json!(now))?;
            Ok(decided.len())
        })
    }

    /// The Driver Match tab: the pairs to decide, then every person with a code.
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch> {
        Ok(overview(self, dsp)?.result)
    }

    /// The Settings tab badge uses the same decisions without transferring the roster.
    fn driver_counts(&self, dsp: &str) -> Result<DriverCounts> {
        Ok(overview(self, dsp)?.result.counts)
    }

    /// One person in full: what they appear in, their last fourteen days and every
    /// decision about them. A merged person's code opens whoever kept their IDs.
    fn driver_details(&self, dsp: &str, code: &str) -> Result<DriverDetails> {
        ensure(valid_code(code), "not_found", 404)?;
        let code = surviving(&*self.dsp(dsp)?, code)?;
        let overview = overview(self, dsp)?;
        let driver = overview
            .result
            .drivers
            .iter()
            .find(|d| d.code == code)
            .cloned()
            .ok_or_else(|| Error::new("not_found", 404))?;
        let ids: Vec<&IdRow> = overview.rows.iter().filter(|r| r.code == code).collect();
        let activity = DATA
            .into_iter()
            .filter_map(|data| {
                let count: usize = ids
                    .iter()
                    .filter_map(|i| overview.activity.get(&i.key())?.get(&data))
                    .map(|seen| seen.count)
                    .sum();
                (count > 0).then_some(DriverActivity { data, count })
            })
            .collect();
        let zone: chrono_tz::Tz = self
            .find_dsp(dsp)?
            .timezone
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let today = chrono::Utc::now().with_timezone(&zone).date_naive();
        let worked = |source: DriverSource| -> Option<BTreeSet<&String>> {
            ids.iter().any(|i| i.source == source).then(|| {
                ids.iter()
                    .filter(|i| i.source == source)
                    .filter_map(|i| overview.days.of(&i.key()))
                    .flatten()
                    .collect()
            })
        };
        let (clocked, drove) = (worked(DriverSource::Paycom), worked(DriverSource::Amazon));
        let days = (0..14)
            .rev()
            .map(|back| {
                let date = (today - chrono::Duration::days(back)).to_string();
                let on = |days: &Option<BTreeSet<&String>>, covered: &BTreeSet<String>| {
                    let days = days.as_ref()?;
                    let here = days.contains(&date);
                    (here || covered.contains(&date)).then_some(here)
                };
                DriverDay {
                    clocked_in: on(&clocked, &overview.days.paycom),
                    drove: on(&drove, &overview.days.amazon),
                    date,
                }
            })
            .collect();
        let db = self.dsp(dsp)?;
        let mut history = vec![];
        for id in &ids {
            history.push(DriverEvent {
                at: id.linked_at.clone(),
                kind: if id.linked_by == DriverLink::New {
                    DriverEventKind::Added
                } else {
                    DriverEventKind::Linked
                },
                source: Some(id.source),
                link: Some(id.linked_by),
                name: overview
                    .found
                    .get(&id.key())
                    .map(|x| names::display(x.name())),
                code: None,
                actor: actor_name(self, id.actor_id.as_deref())?,
            });
        }
        let named = |code: &str| {
            overview
                .result
                .drivers
                .iter()
                .find(|d| d.code == code)
                .map(|d| d.name.clone())
        };
        let mut merged_codes = vec![];
        for (merged, at, by) in db.query_as::<(String, Option<String>, Option<String>)>(
            "SELECT code,merged_at,merged_by FROM people WHERE merged_into=? ORDER BY merged_at",
            [&code],
        )? {
            history.push(DriverEvent {
                at: at.unwrap_or_default(),
                kind: DriverEventKind::Merged,
                source: None,
                link: None,
                name: None,
                code: Some(merged.clone()),
                actor: actor_name(self, by.as_deref())?,
            });
            merged_codes.push(merged);
        }
        for (from, at, by) in db.query_as::<(Option<String>, String, Option<String>)>(
            "SELECT split_from,created_at,created_by FROM people WHERE code=? AND split_from IS NOT NULL",
            [&code],
        )? {
            history.push(DriverEvent {
                at,
                kind: DriverEventKind::Split,
                source: None,
                link: None,
                name: from.as_deref().and_then(named),
                code: from,
                actor: actor_name(self, by.as_deref())?,
            });
        }
        for (first, second, at, by) in db.query_as::<(String, String, String, Option<String>)>(
            "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
            [&code],
        )? {
            let other = if first == code { second } else { first };
            history.push(DriverEvent {
                at,
                kind: DriverEventKind::Apart,
                source: None,
                link: None,
                name: named(&other),
                code: Some(other),
                actor: actor_name(self, by.as_deref())?,
            });
        }
        history.sort_by(|a, b| b.at.cmp(&a.at));
        Ok(DriverDetails {
            driver,
            activity,
            days,
            history,
            merged_codes,
        })
    }

    /// Every Amazon driver with a person: the Paycom codes that person holds, and whether
    /// someone confirmed them rather than a name. The meal-break comparison joins by this.
    fn driver_links(&self, dsp: &str) -> Result<BTreeMap<String, (Vec<String>, bool)>> {
        let rows: Vec<IdRow> = self.dsp(dsp)?.query_as(
            "SELECT * FROM person_ids ORDER BY linked_at,external_id",
            [],
        )?;
        let mut people: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
        for row in &rows {
            people.entry(&row.code).or_default().push(row);
        }
        let mut out = BTreeMap::new();
        for ids in people.values() {
            let paycom: Vec<String> = ids
                .iter()
                .filter(|i| i.source == DriverSource::Paycom)
                .map(|i| i.external_id.clone())
                .collect();
            let confirmed = ids
                .iter()
                .any(|i| matches!(i.linked_by, DriverLink::Person | DriverLink::Saved));
            for id in ids.iter().filter(|i| i.source == DriverSource::Amazon) {
                out.insert(id.external_id.clone(), (paycom.clone(), confirmed));
            }
        }
        Ok(out)
    }

    /// Makes two people one: `code`'s IDs move to `into`, and `code` leads to `into` from
    /// now on. A decision that the two were different people no longer stands.
    fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch> {
        ensure(
            valid_code(code) && valid_code(into) && code != into,
            "invalid_input",
            400,
        )?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            current(&db, code)?;
            current(&db, into)?;
            let now = iso();
            db.exec(
                "UPDATE person_ids SET code=?1,linked_by='person',linked_at=?2,actor_id=?3 \
                 WHERE code=?4",
                params![into, now, c.actor(), code],
            )?;
            db.exec(
                "UPDATE people SET merged_into=?1,merged_at=?2,merged_by=?3 WHERE code=?4",
                params![into, now, c.actor(), code],
            )?;
            // Codes merged into this one before lead straight to the survivor now.
            db.exec("UPDATE people SET merged_into=?1 WHERE merged_into=?2", params![into, code])?;
            let others = db.query_as::<(String, String, String, Option<String>)>(
                "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
                [code],
            )?;
            db.exec("DELETE FROM people_apart WHERE first=?1 OR second=?1", [code])?;
            for (first, second, at, by) in others {
                let other = if first == code { second } else { first };
                if other == into {
                    continue;
                }
                let (a, b) = if into < other.as_str() { (into, other.as_str()) } else { (other.as_str(), into) };
                db.exec(
                    "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
                    params![a, b, at, by],
                )?;
            }
            mirror_links(&db)
        })?;
        drop(db);
        // Codes, not names: what a collection says about someone stays in its database.
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.merged",
            &format!("{code} into {into}"),
            Some(&format!("{code} and {into}")),
            &[("code", Some(code.to_owned()), Some(into.to_owned()))],
            Some(("driver", into)),
        )?;
        self.driver_match(dsp)
    }

    /// Moves one ID off to a new person, when a link was wrong. The two are then kept
    /// apart, so they are not suggested as one again.
    fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch> {
        ensure(valid_code(code), "invalid_input", 400)?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        let fresh = db.transaction(|| {
            current(&db, code)?;
            ensure(
                db.count(
                    "SELECT count(*) FROM person_ids WHERE source=? AND external_id=? AND code=?",
                    params![source, id, code],
                )? == 1,
                "driver_changed",
                409,
            )?;
            ensure(
                db.count("SELECT count(*) FROM person_ids WHERE code=?", [code])? > 1,
                "driver_single_id",
                409,
            )?;
            let now = iso();
            let fresh = new_person(self, &db, dsp, &now, Some(code), Some(c.actor()))?;
            db.exec(
                "UPDATE person_ids SET code=?1,linked_by='person',linked_at=?2,actor_id=?3 \
                 WHERE source=?4 AND external_id=?5",
                params![fresh, now, c.actor(), source, id],
            )?;
            let (a, b) = if code < fresh.as_str() { (code, fresh.as_str()) } else { (fresh.as_str(), code) };
            db.exec(
                "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
                params![a, b, now, c.actor()],
            )?;
            mirror_links(&db)?;
            Ok(fresh)
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.split",
            &format!("{code} to {fresh}"),
            Some(&fresh),
            &[("code", Some(code.to_owned()), Some(fresh.clone()))],
            Some(("driver", &fresh)),
        )?;
        self.driver_match(dsp)
    }

    /// Records that two people are different, so they are never suggested as one again.
    fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch> {
        ensure(
            valid_code(code) && valid_code(other) && code != other,
            "invalid_input",
            400,
        )?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        let (first, second) = if code < other {
            (code, other)
        } else {
            (other, code)
        };
        db.transaction(|| {
            current(&db, code)?;
            current(&db, other)?;
            db.exec(
                "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
                params![first, second, iso(), c.actor()],
            )
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.kept_apart",
            &format!("{code} and {other}"),
            Some(&format!("{code} and {other}")),
            &[],
            Some(("driver", code)),
        )?;
        self.driver_match(dsp)
    }
}

/// The links saved on the meal-break page before Driver Match, read as decisions.
fn saved_links(store: &Store, dsp: &str) -> Result<Saved> {
    let value = store.dsp(dsp)?.setting(LINKS, Value::Null)?;
    let text = |v: &Value| v.as_str().map(str::to_owned);
    Ok(Saved {
        links: value["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| Some((text(&l["cortexId"])?, text(&l["paycomCode"])?)))
            .collect(),
        separate: value["separate"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect(),
    })
}

/// A person with a new code, unique on the platform: every DSP's codes are listed in
/// `driver_codes`, so a code never leads to two people anywhere.
fn new_person(
    store: &Store,
    db: &Db,
    dsp: &str,
    now: &str,
    split_from: Option<&str>,
    actor: Option<&str>,
) -> Result<String> {
    for _ in 0..16 {
        // 240 is the largest multiple of 30 a byte holds; larger bytes would favour
        // the alphabet's first letters, so they are skipped.
        let code: String = crypto::random::<24>()?
            .into_iter()
            .filter(|b| *b < 240)
            .take(CODE_LENGTH)
            .map(|b| ALPHABET[usize::from(b % 30)] as char)
            .collect();
        if code.len() == CODE_LENGTH
            && store.platform.exec(
                "INSERT OR IGNORE INTO driver_codes(code,dsp_id,created_at) VALUES (?,?,?)",
                params![code, dsp, now],
            )? == 1
        {
            db.exec(
                "INSERT INTO people(code,created_at,split_from,created_by) VALUES (?,?,?,?)",
                params![code, now, split_from, actor],
            )?;
            return Ok(code);
        }
    }
    Err(Error::new("driver_code_unavailable", 500))
}

fn overview(store: &Store, dsp: &str) -> Result<Overview> {
    let found: Found = sources::identities(store, dsp)?
        .into_iter()
        .map(|x| ((x.source, x.id.clone()), x))
        .collect();
    let activity = sources::activity(store, dsp)?;
    let days = sources::days(store, dsp)?;
    let saved = saved_links(store, dsp)?;
    let departments = sources::driver_departments(store, dsp)?;
    // Counted from what was collected, not from today, so a DSP whose collections
    // pause does not see everyone leave.
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
    let rows: Vec<IdRow> = db.query_as(
        "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
         WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.source,i.external_id",
        [],
    )?;
    let apart: BTreeSet<(String, String)> = db
        .query_as::<(String, String)>("SELECT first,second FROM people_apart", [])?
        .into_iter()
        .collect();
    let checked_at = db
        .setting(CHECKED, Value::Null)?
        .as_str()
        .map(str::to_owned);
    drop(db);
    let mut held: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
    for row in &rows {
        held.entry(&row.code).or_default().push(row);
    }
    let mut drivers: BTreeMap<String, Driver> = held
        .iter()
        .map(|(code, ids)| {
            (
                (*code).to_owned(),
                driver(code, ids, &activity, &departments, &found, &recent),
            )
        })
        .collect();
    // Someone kept these apart from Paycom on the meal-break page; that decision stands.
    let kept: BTreeSet<&str> = held
        .iter()
        .filter(|(_, ids)| {
            ids.iter().any(|i| {
                i.source == DriverSource::Amazon && saved.separate.contains(&i.external_id)
            })
        })
        .map(|(code, _)| *code)
        .collect();
    // Everyone only one source knows, those who have left included: their records
    // are worth joining all the same.
    let side = |source: DriverSource| -> Vec<review::Side<'_>> {
        let only = [
            DriverStatus::PaycomOnly,
            DriverStatus::AmazonOnly,
            DriverStatus::Former,
        ];
        held.iter()
            .filter(|(code, ids)| {
                only.contains(&drivers[**code].status)
                    && ids.iter().all(|i| i.source == source)
                    && !kept.contains(**code)
            })
            .map(|(code, ids)| review::Side {
                code,
                // Every name an ID's sources write, so a middle name or a different
                // spelling in one of them still counts.
                names: ids
                    .iter()
                    .filter_map(|i| found.get(&i.key()))
                    .flat_map(|x| x.names.iter().map(String::as_str))
                    .collect(),
            })
            .collect()
    };
    let suggestions = review::suggest(
        &side(DriverSource::Paycom),
        &side(DriverSource::Amazon),
        &apart,
    );
    let worked = |code: &str| -> Option<BTreeSet<String>> {
        let sets: Vec<&BTreeSet<String>> = held
            .get(code)?
            .iter()
            .filter_map(|i| days.of(&i.key()))
            .collect();
        (!sets.is_empty()).then(|| sets.into_iter().flatten().cloned().collect())
    };
    let mut pairs: Vec<(
        DriverStrength,
        usize,
        String,
        String,
        Vec<crate::contracts::DriverEvidence>,
    )> = vec![];
    for mut suggestion in suggestions {
        if let Some(days) = review::worked_days(
            worked(&suggestion.paycom).as_ref(),
            worked(&suggestion.amazon).as_ref(),
            &days.paycom,
            &days.amazon,
        ) {
            suggestion.evidence.push(days);
        }
        let strength = review::strength(&suggestion.evidence);
        pairs.push((
            strength,
            suggestion.evidence.len(),
            suggestion.paycom,
            suggestion.amazon,
            suggestion.evidence,
        ));
    }
    // The strongest first, and no more than three choices for any one Amazon driver.
    pairs.sort_by(|a, b| {
        (a.0 != DriverStrength::Strong)
            .cmp(&(b.0 != DriverStrength::Strong))
            .then(b.1.cmp(&a.1))
            .then_with(|| names::compare(&drivers[&a.3].name, &drivers[&b.3].name))
    });
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    pairs.retain(|pair| {
        let count = shown.entry(pair.3.clone()).or_default();
        *count += 1;
        *count <= 3
    });
    for (_, _, paycom, amazon, _) in &pairs {
        for code in [paycom, amazon] {
            drivers.get_mut(code).unwrap().status = DriverStatus::Review;
        }
    }
    let review: Vec<DriverPair> = pairs
        .into_iter()
        .map(|(strength, _, paycom, amazon, evidence)| DriverPair {
            paycom: drivers[&paycom].clone(),
            amazon: drivers[&amazon].clone(),
            strength,
            evidence,
        })
        .collect();
    let mut list: Vec<Driver> = drivers.into_values().collect();
    list.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
    let count =
        |statuses: &[DriverStatus]| list.iter().filter(|d| statuses.contains(&d.status)).count();
    let office = count(&[DriverStatus::Office]);
    let former = count(&[DriverStatus::Former]);
    let counts = DriverCounts {
        all: list.len(),
        drivers: list.len() - office - former,
        matched: count(&[
            DriverStatus::Matched,
            DriverStatus::Variant,
            DriverStatus::Confirmed,
        ]),
        review: review.len(),
        paycom_only: count(&[DriverStatus::PaycomOnly]),
        amazon_only: count(&[DriverStatus::AmazonOnly]),
        office,
        former,
    };
    Ok(Overview {
        result: DriverMatch {
            checked_at,
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

/// The code a merged person's IDs went to, followed to the end.
fn surviving(db: &Db, code: &str) -> Result<String> {
    let mut code = code.to_owned();
    for _ in 0..32 {
        let row = db
            .query_as::<(Option<String>,)>("SELECT merged_into FROM people WHERE code=?", [&code])?
            .into_iter()
            .next()
            .ok_or_else(|| Error::new("not_found", 404))?;
        match row.0 {
            Some(next) => code = next,
            None => return Ok(code),
        }
    }
    Err(Error::new("not_found", 404))
}

/// The name a code's person goes by now, as the tab shows it: Paycom's when they have
/// one, else Amazon's. A merged code answers for the person who kept its IDs.
fn driver_name(store: &Store, dsp: &str, code: &str) -> Result<Option<String>> {
    let ids = {
        let db = store.dsp(dsp)?;
        let code = surviving(&db, code)?;
        db.query_as::<(DriverSource, String)>(
            "SELECT source,external_id FROM person_ids WHERE code=? \
             ORDER BY source='amazon',linked_at,external_id",
            [&code],
        )?
    };
    for key in &ids {
        if let Some(name) = sources::name_of(store, dsp, key)? {
            return Ok(Some(names::display(&name)));
        }
    }
    Ok(None)
}
/// Puts today's names to the codes Driver Match's activity entries name. The entries
/// keep codes, as what a collection says about someone stays in its database; a code
/// that finds no name stays as it is.
pub(crate) fn name_driver_events(store: &Store, events: &mut [Value]) {
    let mut known: BTreeMap<(String, String), String> = BTreeMap::new();
    for event in events {
        let action = event["action"].as_str().unwrap_or_default();
        let (Some(dsp), Some(target)) = (event["dspId"].as_str(), event["target"].as_str()) else {
            continue;
        };
        let codes: Vec<&str> = target.split(" and ").collect();
        if !action.starts_with("driver_match.") || !codes.iter().all(|c| valid_code(c)) {
            continue;
        }
        let mut named: Vec<String> = vec![];
        for code in codes {
            let name = known
                .entry((dsp.to_owned(), code.to_owned()))
                .or_insert_with(|| {
                    driver_name(store, dsp, code)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| code.to_owned())
                })
                .clone();
            // Two codes merged into one person are that one person now.
            if action != "driver_match.merged" || !named.contains(&name) {
                named.push(name);
            }
        }
        event["target"] = json!(named.join(" and "));
    }
}

/// Who did something, as the DSP sees them.
fn actor_name(store: &Store, id: Option<&str>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    store.actor_name(id)
}

/// Writes the confirmed links back where the meal-break page kept them, so the previous
/// release still sees them after a rollback. Remove once that release can no longer be
/// rolled back to.
fn mirror_links(db: &Db) -> Result<()> {
    let before = db.setting(LINKS, json!({"revision":0,"links":[]}))?;
    let mut taken = BTreeSet::new();
    let mut links = vec![];
    for (amazon, paycom) in db.query_as::<(String, String)>(
        "SELECT a.external_id,p.external_id FROM person_ids a JOIN person_ids p \
         ON p.code=a.code AND p.source='paycom' WHERE a.source='amazon' AND EXISTS \
         (SELECT 1 FROM person_ids c WHERE c.code=a.code AND c.linked_by IN ('saved','person')) \
         ORDER BY a.linked_at,a.external_id,p.linked_at",
        [],
    )? {
        // The previous release lets each Paycom code be linked once.
        if taken.insert(paycom.clone()) {
            links.push(json!({"id":crypto::id("employee")?,"cortexId":amazon,"paycomCode":paycom}));
        }
    }
    // A link saved for a driver whose data is no longer stored has no person to speak
    // for it, so it stays as it was saved.
    let known: BTreeSet<String> = db
        .query_as::<(String,)>(
            "SELECT external_id FROM person_ids WHERE source='amazon'",
            [],
        )?
        .into_iter()
        .map(|(id,)| id)
        .collect();
    for link in before["links"].as_array().into_iter().flatten() {
        if let (Some(amazon), Some(paycom)) =
            (link["cortexId"].as_str(), link["paycomCode"].as_str())
            && !known.contains(amazon)
            && taken.insert(paycom.to_owned())
        {
            links.push(link.clone());
        }
    }
    db.set(
        LINKS,
        &json!({
            "revision": before["revision"].as_i64().unwrap_or(0) + 1,
            "links": links,
            "separate": before["separate"].clone(),
        }),
    )
}

/// Answers whether `code` is a person who still holds their IDs.
fn current(db: &Db, code: &str) -> Result<()> {
    ensure(
        db.count(
            "SELECT count(*) FROM people WHERE code=? AND merged_into IS NULL",
            [code],
        )? == 1,
        "driver_changed",
        409,
    )
}
