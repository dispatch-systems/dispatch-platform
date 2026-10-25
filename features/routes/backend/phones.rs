//! Who holds which of the company's work phones, settled from each whole live read: what each
//! driver's record shows, in the day's list and in their itinerary, made into holds from a claim
//! until it ends. A phone passes from driver to driver, so a hold is never a sign that two
//! drivers are one person. Its state is live route data's, `work_phones` and
//! `work_phones_shown`, settled in the transaction that keeps the read it comes from, so a
//! restart neither repeats a claim nor loses one.
use super::*;
use dispatch_cortex::routes::token;
use std::collections::{BTreeMap, BTreeSet};

/// A driver the day's list lists, with the work phone it shows them with and what says whether
/// they're still out: their session ended, and when they last did anything.
pub(super) struct Listed {
    pub(super) driver: String,
    pub(super) phone: Option<String>,
    pub(super) ended: bool,
    pub(super) last_event: i64,
}
/// The work phones a read shows, by their ten digits: the list's for each driver it lists, once
/// each; every driver's it names, those only assigned to a route yet too; and each itinerary's
/// own drivers'.
pub(super) struct Shown {
    pub(super) listed: Vec<Listed>,
    pub(super) named: BTreeMap<String, String>,
    pub(super) itineraries: BTreeMap<String, String>,
}
/// What settling a read's work phones did: how many holds it claimed, moved, handed over,
/// ended or withdrew, and how many are held after it.
pub(super) struct Settled {
    pub(super) events: u64,
    pub(super) holding: u64,
}

/// A phone number as its last ten digits, which is how two records are compared.
pub(super) fn digits(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 10).then(|| digits[digits.len() - 10..].to_owned())
}
/// The phones a list of drivers shows, by the IDs that can name a hold.
pub(super) fn phones_of(transporters: &Value) -> impl Iterator<Item = (String, Option<String>)> {
    let transporters = transporters.as_array().into_iter().flatten();
    transporters.filter_map(|t| {
        let id = t["transporterId"].as_str().filter(|id| token(id, 64))?;
        Some((
            id.to_owned(),
            t["workPhoneNumber"].as_str().and_then(digits),
        ))
    })
}

impl Shown {
    /// What a read shows: in `summaries`, the day's list, of each driver it names and of the
    /// drivers of the itineraries `listed` names; and in `itineraries`, each itinerary's own.
    pub(super) fn read(
        summaries: &Value,
        listed: &[(String, String)],
        itineraries: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let phones: BTreeMap<String, Option<String>> =
            phones_of(&summaries["transporters"]).collect();
        let drivers: BTreeSet<&str> = listed.iter().map(|(_, driver)| driver.as_str()).collect();
        let mut once: BTreeMap<&str, Listed> = BTreeMap::new();
        for summary in summaries["itinerarySummaries"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(driver) = summary["transporterId"]
                .as_str()
                .filter(|d| drivers.contains(d))
            else {
                continue;
            };
            let entry = Listed {
                driver: driver.to_owned(),
                phone: phones.get(driver).cloned().flatten(),
                ended: summary["driverSessionEnded"].as_bool().unwrap_or(false),
                last_event: summary["lastDriverEventTime"]
                    .as_f64()
                    .map_or(0, |t| t as i64),
            };
            // A driver on two routes is listed by the one still out, else the one they last did
            // anything on.
            match once.get(driver) {
                Some(kept)
                    if (!kept.ended, kept.last_event) >= (!entry.ended, entry.last_event) => {}
                _ => {
                    once.insert(driver, entry);
                }
            }
        }
        Self {
            listed: once.into_values().collect(),
            named: (phones.into_iter())
                .filter_map(|(driver, phone)| Some((driver, phone?)))
                .collect(),
            itineraries: itineraries.into_iter().collect(),
        }
    }
    /// What a listed driver's record shows now: the list's phone, else their itinerary's.
    fn showing<'a>(&'a self, listed: &'a Listed) -> Option<&'a str> {
        let itinerary = self.itineraries.get(&listed.driver).map(String::as_str);
        listed.phone.as_deref().or(itinerary)
    }
}

/// What live route data holds of a driver's work phones: the first phone their record showed
/// today, the one it showed at the last read that listed them, the one it showed when they last
/// claimed one, and the phone they hold now.
struct Driver {
    first: Option<String>,
    last: Option<String>,
    claimed: Option<String>,
    holds: Option<String>,
}
fn driver(db: &Db, day: &str, id: &str) -> Result<Driver> {
    let shown = db.one(
        "SELECT first_shown,last_shown,claimed FROM work_phones_shown WHERE transporter_id=?",
        [id],
    )?;
    let holds = db.one(
        "SELECT phone FROM work_phones WHERE day=? AND transporter_id=? AND until IS NULL",
        params![day, id],
    )?;
    let field = |row: &Option<Value>, key: &str| {
        row.as_ref()
            .and_then(|row| row[key].as_str().map(str::to_owned))
    };
    Ok(Driver {
        first: field(&shown, "first_shown"),
        last: field(&shown, "last_shown"),
        claimed: field(&shown, "claimed"),
        holds: field(&holds, "phone"),
    })
}
/// Ends the hold `driver` has at `at`, if they have one, forgetting what their record showed
/// when they claimed it, so the same phone shown again is claimed again. Answers whether they
/// had one.
fn end(db: &Db, day: &str, at: i64, driver: &str) -> Result<bool> {
    let held = db.exec(
        "UPDATE work_phones SET until=? WHERE day=? AND transporter_id=? AND until IS NULL",
        params![at, day, driver],
    )? > 0;
    if held {
        db.exec(
            "UPDATE work_phones_shown SET claimed=NULL WHERE transporter_id=?",
            [driver],
        )?;
    }
    Ok(held)
}

/// Settles who holds which work phone on `day` after a whole read at `at`, inside the caller's
/// transaction. A driver claims a phone once signed in, when what their record shows changes to
/// it, never by showing it again: until they sign in it shows the last phone they had. Signed in
/// is a driver event today, or a record changed from the first phone it showed today, read for
/// every driver the list names, dispatched or not; a claim made before that is withdrawn, never
/// having been a hold. A driver holding a phone switches to another once two reads in a row show
/// it. A claim of a phone someone else holds is a handover and ends theirs, and their record
/// still showing it never takes it back. A session Cortex has ended ends its driver's hold,
/// whatever their record shows then, and forgets what it showed, so the same phone shown once the
/// session is back is claimed again; a driver gone from the day's list ends theirs the same way,
/// and a list naming nobody ends none. Claims are settled oldest activity first, so the most
/// recently active driver holds a phone two records show. Every hold is kept, from its claim
/// until it ends.
pub(super) fn settle(db: &Db, day: &str, at: i64, shown: &Shown) -> Result<Settled> {
    let mut events = 0;
    // The first phone each driver's record shows today: the list's before their itinerary's.
    let mut first = db.0.prepare_cached(
        "INSERT INTO work_phones_shown(transporter_id,first_shown) VALUES (?,?) \
         ON CONFLICT(transporter_id) DO UPDATE SET first_shown=coalesce(first_shown,excluded.first_shown)",
    )?;
    for (driver, phone) in shown.named.iter().chain(&shown.itineraries) {
        first.execute(params![driver, phone])?;
    }
    let mut observed: Vec<(&Listed, &str)> = shown
        .listed
        .iter()
        .filter_map(|listed| Some((listed, shown.showing(listed)?)))
        .collect();
    observed.sort_by_key(|(listed, _)| listed.last_event);
    let mut signed_in = BTreeSet::new();
    for (listed, phone) in &observed {
        let first = driver(db, day, &listed.driver)?.first;
        if listed.last_event > 0 || first.as_deref() != Some(*phone) {
            signed_in.insert(listed.driver.as_str());
        }
    }
    for (listed, _) in &observed {
        let id = listed.driver.as_str();
        if !signed_in.contains(id) && driver(db, day, id)?.holds.is_some() {
            db.exec(
                "DELETE FROM work_phones WHERE day=? AND transporter_id=? AND until IS NULL",
                params![day, id],
            )?;
            db.exec(
                "UPDATE work_phones_shown SET claimed=NULL WHERE transporter_id=?",
                [id],
            )?;
            events += 1;
        }
    }
    for (listed, phone) in &observed {
        let id = listed.driver.as_str();
        // A finished shift's record only shows the last phone it had.
        if listed.ended || !signed_in.contains(id) {
            continue;
        }
        let now = driver(db, day, id)?;
        if now.claimed.as_deref() == Some(*phone) {
            continue;
        }
        // A record that flips between two phones every read moves no phone.
        let switching = now.holds.as_deref().is_some_and(|held| held != *phone);
        if switching && now.last.as_deref() != Some(*phone) {
            continue;
        }
        let handed = db.exec(
            "UPDATE work_phones SET until=?1 WHERE day=?2 AND phone=?3 AND transporter_id<>?4 \
             AND until IS NULL",
            params![at, day, phone, id],
        )?;
        db.exec(
            "UPDATE work_phones SET until=? WHERE day=? AND transporter_id=? AND until IS NULL",
            params![at, day, id],
        )?;
        db.exec(
            "INSERT INTO work_phones(day,transporter_id,phone,since) VALUES (?,?,?,?)",
            params![day, id, phone, at],
        )?;
        db.exec(
            "UPDATE work_phones_shown SET claimed=? WHERE transporter_id=?",
            params![phone, id],
        )?;
        events += 1 + handed as u64;
    }
    for listed in &shown.listed {
        if listed.ended && end(db, day, at, &listed.driver)? {
            events += 1;
        }
    }
    if !shown.listed.is_empty() {
        let listed: BTreeSet<&str> = shown.listed.iter().map(|l| l.driver.as_str()).collect();
        let holding = db.all(
            "SELECT transporter_id FROM work_phones WHERE day=? AND until IS NULL",
            [day],
        )?;
        for row in holding {
            let id = s(&row, "transporter_id");
            if !listed.contains(id) && end(db, day, at, id)? {
                events += 1;
            }
        }
    }
    // What this read showed, which the next one's switches are confirmed against.
    let mut last = db.0.prepare_cached(
        "INSERT INTO work_phones_shown(transporter_id,last_shown) VALUES (?,?) \
         ON CONFLICT(transporter_id) DO UPDATE SET last_shown=excluded.last_shown",
    )?;
    for listed in &shown.listed {
        last.execute(params![listed.driver, shown.showing(listed)])?;
    }
    Ok(Settled {
        events,
        holding: db.count(
            "SELECT count(*) FROM work_phones WHERE day=? AND until IS NULL",
            [day],
        )? as u64,
    })
}

#[cfg(test)]
#[path = "../tests/backend/phones.rs"]
mod tests;
