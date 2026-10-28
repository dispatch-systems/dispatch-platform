//! Each itinerary's work phone, kept in its row in both route databases: the company phone its
//! driver's records show with it, by its last ten digits, the day's list's, else the itinerary's
//! own response's; or, when a read of its day shows none, the one the day's route data last stored
//! for it (`Stored`). A saved day kept before Routes kept them has them extracted from its stored
//! responses by the same rule (`phone_backfill`). A day's read answers each driver's latest
//! (`DayPhones`), and the days kept whether a number was ever one.
use super::*;
use dispatch_routes_contract::{DayPhones, DriverPhone};
use std::collections::{BTreeMap, BTreeSet};

/// The first adapter version whose itineraries keep their work phone. A publication shaped before
/// keeps none in its rows until they're extracted from its stored responses (`phone_backfill`).
pub(super) const PHONES_SINCE: i64 = 3;

/// A phone number as its last ten digits, which is how two records are compared.
pub(super) fn digits(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 10).then(|| digits[digits.len() - 10..].to_owned())
}

impl Shaper {
    /// The work phone `driver`'s records show with their itinerary, by its last ten digits: the
    /// day's list's record of them, else `own`'s, the drivers the itinerary's response names; none
    /// when neither shows one.
    pub(super) fn work_phone(&self, driver: &str, own: &Value) -> Option<String> {
        let shown = |records: &[Value]| {
            let record = records
                .iter()
                .find(|r| r["transporterId"] == json!(driver))?;
            digits(record["workPhoneNumber"].as_str()?)
        };
        shown(&self.people).or_else(|| shown(own.as_array().map_or(&[], Vec::as_slice)))
    }
}

impl PreparedItinerary {
    /// The work phone its read shows (`Shaper::work_phone`), or the one it keeps.
    fn work_phone(&self) -> Option<&str> {
        let (_, phone) = self.columns.iter().find(|(c, _)| c == "work_phone")?;
        phone.as_str()
    }
    /// Gives it `phone`, the one it keeps.
    fn keep_work_phone(&mut self, phone: &str) {
        if let Some((_, kept)) = self.columns.iter_mut().find(|(c, _)| c == "work_phone") {
            *kept = json!(phone);
        }
    }
}

/// Each itinerary of a publication, with its driver and the work phone it keeps, if any.
const ITINERARIES: &str = "SELECT itinerary_id,transporter_id,work_phone FROM itineraries \
    WHERE publication_id=?";

/// The drivers some itineraries give each phone to, by its ten digits.
struct Drivers<'a>(BTreeMap<&'a str, BTreeSet<&'a str>>);
impl<'a> Drivers<'a> {
    /// The drivers of `phones`, each a driver and the phone they're given, if any.
    fn of(phones: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> Self {
        let mut drivers = BTreeMap::<&str, BTreeSet<&str>>::new();
        for (driver, phone) in phones {
            if let Some(phone) = phone {
                drivers.entry(phone).or_default().insert(driver);
            }
        }
        Self(drivers)
    }
    /// Whether they give `phone` to a driver other than `driver`.
    fn other_than(&self, driver: &str, phone: &str) -> bool {
        (self.0.get(phone)).is_some_and(|drivers| drivers.iter().any(|d| *d != driver))
    }
}

/// What a day's route data last stored of its itineraries' work phones, by itinerary ID: each
/// one's driver, and its phone or none. An itinerary that a later read of the same day shows no
/// phone for keeps that phone, as when Cortex ends its driver's session; one stored with none keeps
/// none, as once another driver showed its phone. Only the day's own, never an earlier day's:
/// phones pass from driver to driver day by day. Across a driver's itineraries no keeping is
/// needed: one whose later itinerary shows no phone has the latest that shows one as their latest
/// of the day already (`of_day`).
#[derive(Debug, Default)]
pub(super) struct Stored(BTreeMap<String, (String, Option<String>)>);
impl Stored {
    /// What `publication`'s itineraries keep in `routes`, live or historical route data.
    pub(super) fn of(routes: &Db, publication: &str) -> Result<Self> {
        let rows = routes.all(ITINERARIES, [publication])?;
        let stored = rows.iter().map(|row| {
            let phone = row["work_phone"].as_str().map(str::to_owned);
            let driver = s(row, "transporter_id").to_owned();
            (s(row, "itinerary_id").to_owned(), (driver, phone))
        });
        Ok(Self(stored.collect()))
    }
    /// Each itinerary they store, its ID, driver and phone, if any.
    fn itineraries(&self) -> Vec<(&str, &str, Option<&str>)> {
        (self.0.iter())
            .map(|(id, (driver, phone))| (id.as_str(), driver.as_str(), phone.as_deref()))
            .collect()
    }
    /// These, stored more recently, and `older`'s for each itinerary these don't store: one these
    /// store with no phone keeps none, and a phone these give a driver isn't another's by `older`.
    pub(super) fn or(mut self, older: Self) -> Self {
        let older: Vec<_> = {
            let drivers = Drivers::of(self.0.values().map(|(d, p)| (d.as_str(), p.as_deref())));
            (older.0.into_iter())
                .filter(|(itinerary, _)| !self.0.contains_key(itinerary))
                .map(|(itinerary, (driver, phone))| {
                    let phone = phone.filter(|phone| !drivers.other_than(&driver, phone));
                    (itinerary, (driver, phone))
                })
                .collect()
        };
        self.0.extend(older);
        self
    }
    /// The phone each itinerary of a read that shows none keeps, by its ID: the one stored for it,
    /// unless the read shows that phone on another driver's itinerary, whose it is now. `read` is
    /// each of the read's itineraries, its ID, driver and the phone it shows.
    fn kept(&self, read: &[(&str, &str, Option<&str>)]) -> BTreeMap<String, String> {
        let shown = Drivers::of(read.iter().map(|(_, driver, phone)| (*driver, *phone)));
        (read.iter())
            .filter(|(_, _, phone)| phone.is_none())
            .filter_map(|(itinerary, driver, _)| {
                let phone = self.0.get(*itinerary)?.1.as_deref()?;
                (!shown.other_than(driver, phone))
                    .then(|| ((*itinerary).to_owned(), phone.to_owned()))
            })
            .collect()
    }
    /// `itineraries`, a read of the day shaped, each that shows no phone given the one it keeps.
    pub(super) fn keep_in(&self, itineraries: &mut [PreparedItinerary]) {
        let read: Vec<_> = (itineraries.iter())
            .map(|i| (i.id.as_str(), i.transporter_id.as_str(), i.work_phone()))
            .collect();
        let kept = self.kept(&read);
        for itinerary in itineraries {
            if let Some(phone) = kept.get(&itinerary.id) {
                itinerary.keep_work_phone(phone);
            }
        }
    }
    /// `publication`'s itineraries in `routes`, a read of the day staged, each that shows no phone
    /// given the one it keeps, inside the caller's transaction.
    pub(super) fn keep_staged(&self, routes: &Db, publication: &str) -> Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        let read = Self::of(routes, publication)?;
        for (itinerary, phone) in self.kept(&read.itineraries()) {
            routes.exec(
                "UPDATE itineraries SET work_phone=? WHERE publication_id=? AND itinerary_id=?",
                params![phone, publication, itinerary],
            )?;
        }
        Ok(())
    }
}

/// The oldest adapter version among a day's active publications at a station: none when it has
/// none.
const DAY_VERSION: &str = "SELECT min(adapter_version) version FROM route_publications \
    WHERE station=?1 AND day=?2 AND active=1";
/// Each itinerary of a day's read at a station that shows a work phone, with when it began: when it
/// started, or else was planned to depart.
const DAY_PHONES: &str = "SELECT i.transporter_id,i.itinerary_id,i.work_phone,\
    coalesce(i.started_at,i.planned_departure_at) began FROM itineraries i \
    JOIN route_publications p ON p.id=i.publication_id AND p.active=1 AND p.station=?1 \
    WHERE i.day=?2 AND i.work_phone IS NOT NULL";

/// Each driver's latest work phone on `day` at `station`, as `routes`, live or historical route
/// data, keeps its read: the phone on their itinerary that began last, of those showing one, and of
/// two that began together, the one whose ID sorts last.
pub(super) fn of_day(routes: &Db, station: &str, day: &str) -> Result<DayPhones> {
    // One read, so a recollection can't land between the day's version and its phones.
    routes.transaction(|| {
        let version = routes.one(DAY_VERSION, params![station, day])?;
        match version.and_then(|row| row["version"].as_i64()) {
            None => return Ok(DayPhones::Uncollected),
            Some(version) if version < PHONES_SINCE => return Ok(DayPhones::Unextracted),
            Some(_) => {}
        }
        // Each driver's itinerary that began last, by when it began and its ID, with its phone.
        let mut latest = BTreeMap::<String, ((Option<i64>, String), String)>::new();
        for row in routes.all(DAY_PHONES, params![station, day])? {
            let began = (row["began"].as_i64(), s(&row, "itinerary_id").to_owned());
            let driver = s(&row, "transporter_id");
            if latest.get(driver).is_none_or(|(kept, _)| began > *kept) {
                latest.insert(driver.to_owned(), (began, s(&row, "work_phone").to_owned()));
            }
        }
        let mut drivers = BTreeMap::<&str, usize>::new();
        for (_, phone) in latest.values() {
            *drivers.entry(phone.as_str()).or_default() += 1;
        }
        let phones = latest.iter().map(|(driver, (_, phone))| DriverPhone {
            driver_id: driver.clone(),
            phone: phone.clone(),
            shared: drivers[phone.as_str()] > 1,
        });
        Ok(DayPhones::Read(phones.collect()))
    })
}

/// An itinerary of a day kept whose work phone is the one given, found by the phone
/// (`itineraries_work_phone` in historical route data, which keeps every day's).
pub(super) const EVER: &str = "SELECT 1 FROM itineraries i \
    JOIN route_publications p ON p.id=i.publication_id AND p.active=1 WHERE i.work_phone=? LIMIT 1";
/// Whether `phone`, its ten digits, is an itinerary's work phone on a day `routes` keeps.
pub(super) fn ever(routes: &Db, phone: &str) -> Result<bool> {
    Ok(routes.one(EVER, [phone])?.is_some())
}

#[cfg(test)]
#[path = "../tests/backend/work_phones.rs"]
mod tests;
