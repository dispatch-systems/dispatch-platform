//! Each itinerary's work phone, kept in its row in both route databases: the company phone its
//! driver's records show with it, by its last ten digits, the day's list's, else the itinerary's
//! own response's. A day's read answers each driver's latest (`DayPhones`), and the days kept
//! whether a number was ever one.
use super::*;
use dispatch_routes_contract::{DayPhones, DriverPhone};
use std::collections::BTreeMap;

/// The first adapter version whose itineraries keep their work phone. A publication shaped before
/// keeps none in its rows.
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
            let record = records.iter().find(|r| r["transporterId"] == json!(driver))?;
            digits(record["workPhoneNumber"].as_str()?)
        };
        shown(&self.people).or_else(|| shown(own.as_array().map_or(&[], Vec::as_slice)))
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
