//! Driver Match: one code for each person the DSP's collections know, whatever each
//! source calls them. Collected rows keep their sources' IDs; `person_ids` leads every ID
//! to its person. New IDs get a person after every collection, joining someone only on a
//! certain match; anything less waits for a decision in Settings.
mod matching;
pub(crate) mod names;
mod review;
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
    ensure, workforce,
};
use matching::{Member, Saved};
use rusqlite::params;
use serde_json::{Value, json};
use sources::{Days, Key, Seen};
use std::collections::{BTreeMap, BTreeSet};

/// When collected data was last checked for new IDs.
const CHECKED: &str = "driver_match.checked_at";
/// Letters and digits that cannot be mistaken for one another, without U, so a code
/// rarely spells a word.
const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 6;
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

/// The Paycom departments a DSP counts as drivers, from its Timecard settings. Everyone
/// only Paycom knows outside them is office staff; without the setting, nobody is.
type DriverDepartments = Option<BTreeSet<String>>;
fn office(department: Option<&str>, drivers: &DriverDepartments) -> bool {
    drivers
        .as_ref()
        .is_some_and(|list| !list.contains(department.unwrap_or("")))
}

/// What one pass reads: every ID the collections hold and the links saved on the meal-break
/// page. Reading takes the shared lock; `assign_drivers` then writes in one short step.
pub struct DriverSources {
    identities: Vec<sources::Identity>,
    saved: Saved,
}

/// A row of `person_ids`.
struct IdRow {
    source: DriverSource,
    external_id: String,
    code: String,
    name: String,
    department: Option<String>,
    position: Option<String>,
    first_seen: Option<String>,
    last_seen: Option<String>,
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
            name: row.get("name")?,
            department: row.get("department")?,
            position: row.get("position")?,
            first_seen: row.get("first_seen")?,
            last_seen: row.get("last_seen")?,
            linked_by: row.get("linked_by")?,
            linked_at: row.get("linked_at")?,
            actor_id: row.get("actor_id")?,
        })
    }
}
impl IdRow {
    fn key(&self) -> Key {
        (self.source, self.external_id.clone())
    }
    fn public(&self) -> DriverId {
        DriverId {
            source: self.source,
            id: self.external_id.clone(),
            name: self.name.clone(),
            department: self.department.clone(),
            position: self.position.clone(),
            first_seen: self.first_seen.clone(),
            last_seen: self.last_seen.clone(),
            linked_by: self.linked_by,
            linked_at: self.linked_at.clone(),
        }
    }
}

/// Everything the tab and the details panel read, gathered once.
struct Overview {
    result: DriverMatch,
    rows: Vec<IdRow>,
    activity: BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    days: Days,
}

/// A person as the tab lists them, before review marks the ones in a pair.
fn driver(
    code: &str,
    ids: &[&IdRow],
    activity: &BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    departments: &DriverDepartments,
) -> Driver {
    let paycom: Vec<&&IdRow> = ids
        .iter()
        .filter(|i| i.source == DriverSource::Paycom)
        .collect();
    let amazon = ids.iter().any(|i| i.source == DriverSource::Amazon);
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
    } else if amazon {
        DriverStatus::AmazonOnly
    } else if paycom
        .iter()
        .all(|i| office(i.department.as_deref(), departments))
    {
        DriverStatus::Office
    } else {
        DriverStatus::PaycomOnly
    };
    let seen: Vec<&BTreeMap<DriverData, Seen>> =
        ids.iter().filter_map(|i| activity.get(&i.key())).collect();
    let last_seen = seen
        .iter()
        .flat_map(|data| data.values().filter_map(|s| s.last.clone()))
        .chain(ids.iter().filter_map(|i| i.last_seen.clone()))
        .max();
    // Paycom's name is the legal one; Amazon's stands in when Paycom has none.
    let name = paycom
        .first()
        .map(|i| names::display(&i.name))
        .or_else(|| ids.first().map(|i| names::display(&i.name)))
        .unwrap_or_default();
    Driver {
        code: code.to_owned(),
        name,
        status,
        ids: ids.iter().map(|i| i.public()).collect(),
        appears: DATA
            .into_iter()
            .filter(|d| seen.iter().any(|data| data.contains_key(d)))
            .collect(),
        last_seen,
    }
}

impl Store {
    /// Gives every new ID in the DSP's collections a person, and keeps each known ID's
    /// name, department and days current. Answers how many IDs were new.
    pub fn match_drivers(&self, dsp: &str) -> Result<usize> {
        self.assign_drivers(dsp, self.driver_sources(dsp)?)
    }
    /// Reads what a pass needs, without writing: run it under the shared lock.
    pub fn driver_sources(&self, dsp: &str) -> Result<DriverSources> {
        Ok(DriverSources {
            identities: sources::identities(self, dsp)?,
            saved: self.saved_links(dsp)?,
        })
    }
    /// Writes what a pass found: people for new IDs, and known IDs' details kept current.
    /// Who already holds what is read again here, so a decision made since the read stands.
    pub fn assign_drivers(&self, dsp: &str, found: DriverSources) -> Result<usize> {
        let DriverSources { identities, saved } = found;
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let rows: Vec<IdRow> = db.query_as("SELECT * FROM person_ids", [])?;
            let known: BTreeMap<Key, &IdRow> = rows.iter().map(|r| (r.key(), r)).collect();
            let mut incoming = vec![];
            for x in &identities {
                let Some(row) = known.get(&(x.source, x.id.clone())) else {
                    incoming.push(x.clone());
                    continue;
                };
                if row.name != x.name()
                    || row.department != x.department
                    || row.position != x.position
                    || row.first_seen != x.first_seen
                    || row.last_seen != x.last_seen
                {
                    db.exec(
                        "UPDATE person_ids SET name=?,department=?,position=?,first_seen=?,\
                         last_seen=? WHERE source=? AND external_id=?",
                        params![
                            x.name(),
                            x.department,
                            x.position,
                            x.first_seen,
                            x.last_seen,
                            x.source,
                            x.id
                        ],
                    )?;
                }
            }
            let names: BTreeMap<Key, &Vec<String>> = identities
                .iter()
                .map(|x| ((x.source, x.id.clone()), &x.names))
                .collect();
            let mut people: BTreeMap<String, Vec<Member>> = BTreeMap::new();
            for row in &rows {
                people.entry(row.code.clone()).or_default().push(Member {
                    source: row.source,
                    id: row.external_id.clone(),
                    names: names
                        .get(&row.key())
                        .map_or_else(|| vec![row.name.clone()], |n| (*n).clone()),
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
                    let code = self.new_person(&db, dsp, &now, None, None)?;
                    fresh.insert(person.clone(), code.clone());
                    code
                };
                let x = &incoming[*index];
                db.exec(
                    "INSERT INTO person_ids(source,external_id,code,name,department,position,\
                     first_seen,last_seen,linked_by,linked_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
                    params![
                        x.source,
                        x.id,
                        code,
                        x.name(),
                        x.department,
                        x.position,
                        x.first_seen,
                        x.last_seen,
                        link,
                        now
                    ],
                )?;
            }
            db.set(CHECKED, &json!(now))?;
            Ok(decided.len())
        })
    }

    /// The links saved on the meal-break page before Driver Match, read as decisions.
    fn saved_links(&self, dsp: &str) -> Result<Saved> {
        let value = self.dsp(dsp)?.setting(crate::meals::LINKS, Value::Null)?;
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
        &self,
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
                && self.platform.exec(
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

    fn overview(&self, dsp: &str) -> Result<Overview> {
        let identities = sources::identities(self, dsp)?;
        let activity = sources::activity(self, dsp)?;
        let days = sources::days(self, dsp)?;
        let saved = self.saved_links(dsp)?;
        let departments: DriverDepartments =
            self.preference_values(dsp)?["values"]["driver_departments"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|d| d.as_str().map(str::to_owned))
                        .collect()
                });
        let db = self.dsp(dsp)?;
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
                    driver(code, ids, &activity, &departments),
                )
            })
            .collect();
        // Every name an ID's sources write, so a middle name or a different spelling in
        // one of them still counts.
        let spellings: BTreeMap<Key, &Vec<String>> = identities
            .iter()
            .map(|x| ((x.source, x.id.clone()), &x.names))
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
        let side = |status: DriverStatus| -> Vec<review::Side<'_>> {
            held.iter()
                .filter(|(code, _)| drivers[**code].status == status && !kept.contains(**code))
                .map(|(code, ids)| review::Side {
                    code,
                    names: ids
                        .iter()
                        .flat_map(|i| {
                            spellings.get(&i.key()).map_or_else(
                                || vec![i.name.as_str()],
                                |n| n.iter().map(String::as_str).collect(),
                            )
                        })
                        .collect(),
                })
                .collect()
        };
        let suggestions = review::suggest(
            &side(DriverStatus::PaycomOnly),
            &side(DriverStatus::AmazonOnly),
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
                .then_with(|| workforce::compare(&drivers[&a.3].name, &drivers[&b.3].name))
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
        list.sort_by(|a, b| workforce::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
        let count = |statuses: &[DriverStatus]| {
            list.iter().filter(|d| statuses.contains(&d.status)).count()
        };
        let office = count(&[DriverStatus::Office]);
        let counts = DriverCounts {
            all: list.len(),
            drivers: list.len() - office,
            matched: count(&[
                DriverStatus::Matched,
                DriverStatus::Variant,
                DriverStatus::Confirmed,
            ]),
            review: review.len(),
            paycom_only: count(&[DriverStatus::PaycomOnly]),
            amazon_only: count(&[DriverStatus::AmazonOnly]),
            office,
        };
        Ok(Overview {
            result: DriverMatch {
                checked_at,
                counts,
                review,
                drivers: list,
            },
            rows,
            activity,
            days,
        })
    }

    /// The Driver Match tab: the pairs to decide, then every person with a code.
    pub fn driver_match(&self, dsp: &str) -> Result<DriverMatch> {
        Ok(self.overview(dsp)?.result)
    }

    /// The code a merged person's IDs went to, followed to the end.
    fn surviving(&self, db: &Db, code: &str) -> Result<String> {
        let mut code = code.to_owned();
        for _ in 0..32 {
            let row = db
                .query_as::<(Option<String>,)>(
                    "SELECT merged_into FROM people WHERE code=?",
                    [&code],
                )?
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

    /// Who did something, as the DSP sees them: a platform owner is always Platform support.
    fn actor_name(&self, id: Option<&str>) -> Result<Option<String>> {
        let Some(id) = id else { return Ok(None) };
        Ok(self
            .platform
            .query_as::<(String, bool)>(
                "SELECT first_name||' '||last_name,platform_owner FROM users WHERE id=?",
                [id],
            )?
            .into_iter()
            .next()
            .map(|(name, owner)| {
                if owner {
                    "Platform support".to_owned()
                } else {
                    name
                }
            }))
    }

    /// One person in full: what they appear in, their last fourteen days and every
    /// decision about them. A merged person's code opens whoever kept their IDs.
    pub fn driver_details(&self, dsp: &str, code: &str) -> Result<DriverDetails> {
        ensure(valid_code(code), "not_found", 404)?;
        let code = self.surviving(&*self.dsp(dsp)?, code)?;
        let overview = self.overview(dsp)?;
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
                name: Some(names::display(&id.name)),
                code: None,
                actor: self.actor_name(id.actor_id.as_deref())?,
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
                actor: self.actor_name(by.as_deref())?,
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
                actor: self.actor_name(by.as_deref())?,
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
                actor: self.actor_name(by.as_deref())?,
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
    fn display_name(db: &Db, code: &str) -> Result<String> {
        let rows = db.query_as::<(String, String)>(
            "SELECT source,name FROM person_ids WHERE code=? ORDER BY source='amazon',linked_at",
            [code],
        )?;
        Ok(rows
            .first()
            .map(|(_, name)| names::display(name))
            .unwrap_or_default())
    }

    /// Makes two people one: `code`'s IDs move to `into`, and `code` leads to `into` from
    /// now on. A decision that the two were different people no longer stands.
    pub fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch> {
        ensure(
            valid_code(code) && valid_code(into) && code != into,
            "invalid_input",
            400,
        )?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        let name = db.transaction(|| {
            Self::current(&db, code)?;
            Self::current(&db, into)?;
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
            Self::display_name(&db, into)
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.merged",
            &format!("{code} into {into}"),
            Some(&name),
            &[("code", Some(code.to_owned()), Some(into.to_owned()))],
            Some(("driver", into)),
        )?;
        self.driver_match(dsp)
    }

    /// Moves one ID off to a new person, when a link was wrong. The two are then kept
    /// apart, so they are not suggested as one again.
    pub fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch> {
        ensure(valid_code(code), "invalid_input", 400)?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        let (fresh, name) = db.transaction(|| {
            Self::current(&db, code)?;
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
            let fresh = self.new_person(&db, dsp, &now, Some(code), Some(c.actor()))?;
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
            Ok((fresh.clone(), Self::display_name(&db, &fresh)?))
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.split",
            &format!("{code} to {fresh}"),
            Some(&name),
            &[("code", Some(code.to_owned()), Some(fresh.clone()))],
            Some(("driver", &fresh)),
        )?;
        self.driver_match(dsp)
    }

    /// Records that two people are different, so they are never suggested as one again.
    pub fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch> {
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
        let names = db.transaction(|| {
            Self::current(&db, code)?;
            Self::current(&db, other)?;
            db.exec(
                "INSERT OR IGNORE INTO people_apart(first,second,decided_at,actor_id) VALUES (?,?,?,?)",
                params![first, second, iso(), c.actor()],
            )?;
            Ok(format!(
                "{} and {}",
                Self::display_name(&db, code)?,
                Self::display_name(&db, other)?
            ))
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.kept_apart",
            &format!("{code} and {other}"),
            Some(&names),
            &[],
            Some(("driver", code)),
        )?;
        self.driver_match(dsp)
    }
}
