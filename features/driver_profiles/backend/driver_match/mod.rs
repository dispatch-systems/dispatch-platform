//! Driver Match: one code for each person the DSP's collections know, whatever each
//! source calls them. Collected rows keep their sources' IDs; `person_ids` leads every ID
//! to its person. New IDs get a person after every collection, joining someone only on a
//! certain match; anything less waits for a decision in Settings. It reads only the
//! features the DSP has on, and does nothing while it is off itself.
mod contacts;
pub(crate) mod maintenance;
mod matching;
mod overview;
mod review;
mod sources;

use crate::api::types::{
    DriverActivity, DriverCounts, DriverDays, DriverDetails, DriverEvent, DriverEventKind,
    DriverLink, DriverMatch,
};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, FromRow, Row, Store, iso},
    ensure,
    foundation::{crypto, names},
    manifest::people::{self, DriverData, DriverSource, Link},
    server::cache::DataDomain,
};
use matching::{Member, Saved};
use overview::overview;
use rusqlite::params;
use serde_json::{Value, json};
use sources::{Identity, Key};
use std::collections::{BTreeMap, BTreeSet};

/// Driver codes, the decisions about them and the IDs they lead to: core's own domain, as
/// every feature showing people reads them.
pub const DOMAIN: DataDomain = DataDomain::DRIVERS;
/// Its part of Driver Profiles: it matches only while this is on.
pub(crate) const PART: &str = "driver_match";
/// When collected data was last checked for new IDs.
const CHECKED: &str = "driver_match.checked_at";
/// Links saved on the meal-break page before Driver Match, which reads them as decisions and
/// writes its own back for the previous release.
pub const LINKS: &str = "employees.provider_links";
/// Letters and digits that cannot be mistaken for one another, without U, so a code
/// rarely spells a word.
const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 6;
/// How long someone no source lists can go unseen before they count as having left: days
/// before the newest day anything was collected for.
const FORMER_AFTER_DAYS: i64 = 21;

pub fn valid_code(code: &str) -> bool {
    code.len() == CODE_LENGTH && code.bytes().all(|b| ALPHABET.contains(&b))
}

/// What one pass reads: every ID the collections hold and the links saved on the meal-break
/// page. Reading takes the shared lock; `assign_drivers` then writes in one short step.
pub struct DriverSources {
    identities: Vec<sources::Identity>,
    saved: Saved,
}

/// A row of `person_ids`: an ID, its person and how it came to them. Nothing a source
/// says about the person is kept here; that stays in the source's own database. Its source
/// is kept as written, as a feature no longer in the build may have named it.
pub(crate) struct IdRow {
    source: String,
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
impl IdRow {
    /// Its source, while a feature in this build declares it.
    fn source(&self) -> Option<DriverSource> {
        DriverSource::parse(&self.source)
    }
    fn key(&self) -> Option<Key> {
        Some((self.source()?, self.external_id.clone()))
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
    /// The photo of `code`'s person, from Amazon's roster: its kind and its bytes.
    fn driver_photo(&self, dsp: &str, code: &str) -> Result<Option<(&'static str, Vec<u8>)>>;
    fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch>;
    fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch>;
    fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch>;
    /// Confirms one of `code`'s IDs is theirs, as a match only suggested it.
    fn confirm_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch>;
}
impl DriverMatchStore for Store {
    /// Gives every new ID in the DSP's collections a person. Answers how many IDs were new.
    fn match_drivers(&self, dsp: &str) -> Result<usize> {
        self.assign_drivers(dsp, self.driver_sources(dsp)?)
    }
    /// Reads what a pass needs, without writing: run it under the shared lock.
    fn driver_sources(&self, dsp: &str) -> Result<DriverSources> {
        let people = sources::people(self, dsp)?;
        Ok(DriverSources {
            identities: sources::identities(&people, self, dsp)?,
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
            let known: BTreeSet<(String, String)> = rows
                .iter()
                .map(|r| (r.source.clone(), r.external_id.clone()))
                .collect();
            let described: BTreeMap<Key, &Identity> = identities
                .iter()
                .map(|x| ((x.source, x.id.clone()), x))
                .collect();
            let incoming: Vec<Identity> = identities
                .iter()
                .filter(|x| !known.contains(&(x.source.as_str().to_owned(), x.id.clone())))
                .cloned()
                .collect();
            // An ID no source holds any more has nothing to match by.
            let mut people: BTreeMap<String, Vec<Member>> = BTreeMap::new();
            for row in &rows {
                let Some(key) = row.key() else { continue };
                let member = match described.get(&key) {
                    Some(x) => Member::of(x),
                    None => Member {
                        source: key.0,
                        id: key.1,
                        names: vec![],
                        emails: BTreeSet::new(),
                        phones: BTreeSet::new(),
                    },
                };
                people.entry(row.code.clone()).or_default().push(member);
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
        let ids: Vec<(&IdRow, Key)> = overview
            .rows
            .iter()
            .filter(|r| r.code == code)
            .filter_map(|r| Some((r, r.key()?)))
            .collect();
        let activity = DriverData::all()
            .filter_map(|data| {
                let count: usize = ids
                    .iter()
                    .filter_map(|(_, key)| overview.activity.get(key)?.get(&data))
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
        let dates: Vec<String> = (0..14)
            .rev()
            .map(|back| (today - chrono::Duration::days(back)).to_string())
            .collect();
        // Each source that knows them and was collected says, for each day it covers,
        // whether they worked.
        let mut known: Vec<DriverSource> = ids.iter().map(|(_, key)| key.0).collect();
        known.sort();
        known.dedup();
        let days = known
            .into_iter()
            .filter_map(|source| {
                let covered = overview.days.collected.get(&source)?;
                let worked: BTreeSet<&String> = ids
                    .iter()
                    .filter(|(_, key)| key.0 == source)
                    .filter_map(|(_, key)| overview.days.of(key))
                    .flatten()
                    .collect();
                Some(DriverDays {
                    source,
                    worked: dates
                        .iter()
                        .map(|date| {
                            let here = worked.contains(date);
                            (here || covered.contains(date)).then_some(here)
                        })
                        .collect(),
                })
            })
            .collect();
        let db = self.dsp(dsp)?;
        let mut history = vec![];
        for (id, key) in &ids {
            history.push(DriverEvent {
                at: id.linked_at.clone(),
                kind: if id.linked_by == DriverLink::New {
                    DriverEventKind::Added
                } else {
                    DriverEventKind::Linked
                },
                source: Some(key.0),
                link: Some(id.linked_by),
                name: overview.found.get(key).map(|x| names::display(x.name())),
                code: None,
                actor: actor_name(self, dsp, id.actor_id.as_deref())?,
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
                actor: actor_name(self, dsp, by.as_deref())?,
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
                actor: actor_name(self, dsp, by.as_deref())?,
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
                actor: actor_name(self, dsp, by.as_deref())?,
            });
        }
        history.sort_by(|a, b| b.at.cmp(&a.at));
        Ok(DriverDetails {
            driver,
            activity,
            dates,
            days,
            history,
            merged_codes,
        })
    }

    fn driver_photo(&self, dsp: &str, code: &str) -> Result<Option<(&'static str, Vec<u8>)>> {
        ensure(valid_code(code), "not_found", 404)?;
        let db = self.dsp(dsp)?;
        let code = surviving(&db, code)?;
        let amazon: Vec<String> = db
            .query_as::<(String,)>(
                "SELECT external_id FROM person_ids WHERE code=? AND source=? \
                 ORDER BY linked_at,external_id",
                params![code, matching::AMAZON],
            )?
            .into_iter()
            .map(|(id,)| id)
            .collect();
        drop(db);
        crate::backend::storage::DriverProfilesStore::photo(self, dsp, &amazon)
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

    fn confirm_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch> {
        ensure(valid_code(code), "invalid_input", 400)?;
        let dsp = c.dsp.id.as_str();
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            current(&db, code)?;
            ensure(
                db.exec(
                    "UPDATE person_ids SET linked_by='person',linked_at=?1,actor_id=?2 \
                     WHERE code=?3 AND source=?4 AND external_id=?5 AND linked_by<>'new'",
                    params![iso(), c.actor(), code, source, id],
                )? == 1,
                "driver_changed",
                409,
            )?;
            mirror_links(&db)
        })?;
        drop(db);
        self.audit_ref(
            Some(c.actor()),
            Some(dsp),
            "driver_match.confirmed",
            code,
            Some(code),
            &[],
            Some(("driver", code)),
        )?;
        self.driver_match(dsp)
    }
}

/// Who each ID is, for the features that show people: Timecard's meal breaks name each
/// driver's Paycom employee by it.
pub(crate) struct Directory;
impl people::Directory for Directory {
    fn links(
        &self,
        store: &Store,
        dsp: &str,
        from: DriverSource,
        to: DriverSource,
    ) -> Result<BTreeMap<String, Link>> {
        if !store.feature_enabled(dsp, PART)? {
            return Ok(BTreeMap::new());
        }
        let rows: Vec<IdRow> = store.dsp(dsp)?.query_as(
            "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.external_id",
            [],
        )?;
        let mut people: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
        for row in &rows {
            people.entry(&row.code).or_default().push(row);
        }
        let mut out = BTreeMap::new();
        for (code, ids) in people {
            let link = Link {
                code: code.to_owned(),
                ids: ids
                    .iter()
                    .filter(|i| i.source == to.as_str())
                    .map(|i| i.external_id.clone())
                    .collect(),
                confirmed: ids
                    .iter()
                    .any(|i| matches!(i.linked_by, DriverLink::Person | DriverLink::Saved)),
            };
            for id in ids.iter().filter(|i| i.source == from.as_str()) {
                out.insert(id.external_id.clone(), link.clone());
            }
        }
        Ok(out)
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
/// one, else another source's. A merged code answers for the person who kept its IDs.
fn driver_name(store: &Store, dsp: &str, code: &str) -> Result<Option<String>> {
    let ids = {
        let db = store.dsp(dsp)?;
        let code = surviving(&db, code)?;
        db.query_as::<(String, String)>(
            "SELECT source,external_id FROM person_ids WHERE code=? \
             ORDER BY source<>'paycom',linked_at,external_id",
            [&code],
        )?
    };
    for (source, id) in ids {
        let Some(source) = DriverSource::parse(&source) else {
            continue;
        };
        if let Some(name) = sources::name_of(store, dsp, &(source, id))? {
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
fn actor_name(store: &Store, dsp: &str, id: Option<&str>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    store.actor_name(dsp, id)
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
