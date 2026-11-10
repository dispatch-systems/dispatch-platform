//! The proof that keeping a live read leaves the DSP's people cache as it is: read-only, before
//! the read is kept, it holds only where every driver name and seen-day upsert and the day's
//! distinct drivers would be unchanged. Whatever it doesn't recognise may change them.
use super::{Shaped, assigned_drivers};
use crate::backend::{
    ADAPTER_VERSION, RoutesStore,
    publishing::{DriverWrite, widened_seen},
    shaping::identifier,
};
use dispatch_core::{
    Result,
    db::{Store, at, s},
    server::cache::PeopleImpact,
};
use dispatch_cortex::routes::Mode;
use std::collections::{BTreeMap, BTreeSet};

/// Typed held row, including exactly the seen expressions the drivers' upsert uses.
struct HeldDriver {
    id: String,
    first: Option<String>,
    last: Option<String>,
    first_seen: String,
    last_seen: String,
    same_seen: bool,
}
impl dispatch_core::db::FromRow for HeldDriver {
    fn from_row(row: &dispatch_core::db::Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("transporter_id")?,
            first: row.get("first_name")?,
            last: row.get("last_name")?,
            first_seen: row.get("first_seen_day")?,
            last_seen: row.get("last_seen_day")?,
            same_seen: row.get("same_seen")?,
        })
    }
}

/// A sufficient no-change proof for the full live people contribution, before any writes.
/// Every unrecognized/reset/older branch remains conservative. The writer uses the same
/// driver extraction; its only later preparation carries work phones, outside this projection.
pub(super) fn people_impact(store: &Store, dsp: &str, read: &Shaped) -> Result<PeopleImpact> {
    let uncertain = PeopleImpact::MayChange;
    let db = store.routes_live_db(dsp)?;
    let scope = &read.lists.scope;
    let day = scope.date.as_str();
    let held = db.all(
        "SELECT id,day,station,service_area_id,provider,timezone,mode,active,collected_at,adapter_version \
         FROM route_publications LIMIT 2",
        [],
    )?;
    let [held] = held.as_slice() else {
        return Ok(uncertain);
    };
    if held["active"].as_i64() != Some(1)
        || held["adapter_version"].as_i64() != Some(ADAPTER_VERSION)
        || !identifier(s(held, "id"), 512)
        || held["mode"].as_str() != Some(Mode::Snapshot.as_str())
        || held["timezone"].as_str() != Some(scope.timezone.as_str())
        || held["collected_at"]
            .as_str()
            .is_none_or(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).is_err())
        || (
            s(held, "day"),
            s(held, "station"),
            s(held, "service_area_id"),
            s(held, "provider"),
        ) != (
            day,
            scope.station.as_str(),
            scope.service_area_id.as_str(),
            scope.provider.as_str(),
        )
        || held["collected_at"]
            .as_str()
            .is_none_or(|at_held| at_held >= at(read.lists.finished_at).as_str())
    {
        return Ok(uncertain);
    }
    // A valid current scope has text dates and nullable text names. Typed reads reject
    // unknown storage rather than letting JSON's string convenience erase NULL/type differences.
    let [first_seen, last_seen] = widened_seen("drivers", "?1", "?1");
    let drivers = db.query_as::<HeldDriver>(
        &format!(
            "SELECT transporter_id,first_name,last_name,first_seen_day,last_seen_day, \
            (first_seen_day,last_seen_day) IS ({first_seen},{last_seen}) same_seen FROM drivers"
        ),
        [day],
    )?;
    let drivers: BTreeMap<_, _> = drivers
        .into_iter()
        .map(|driver| (driver.id.clone(), driver))
        .collect();
    let planned = assigned_drivers(
        &scope.provider,
        &read.lists.route_summaries,
        &read.itineraries,
    )
    .into_iter()
    .chain(read.itineraries.iter().filter_map(DriverWrite::itinerary));
    for driver in planned {
        let Some(held) = drivers.get(&driver.id) else {
            return Ok(uncertain);
        };
        // shared_update overwrites nullable names and widens seen with SQLite min/max.
        // Check every write against the held row, so a temporary rename restored by a
        // duplicate is conservative too. Unknown date spellings never certify no change.
        let canonical = |date: &str| {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .is_ok_and(|parsed| parsed.to_string() == date)
        };
        if !identifier(&driver.id, 512)
            || driver.first != held.first
            || driver.last != held.last
            || !canonical(&held.first_seen)
            || !canonical(&held.last_seen)
            || !held.same_seen
        {
            return Ok(uncertain);
        }
    }
    let mut ids = BTreeSet::new();
    let mut proposed = BTreeSet::new();
    for itinerary in &read.itineraries {
        // write_itinerary binds these actual dynamic columns. Do not infer membership
        // from the separate task/shared-driver ID if their representations ever diverge.
        let mut bound = itinerary
            .columns
            .iter()
            .filter(|(column, _)| column == "transporter_id");
        let id = bound.next().and_then(|(_, value)| value.as_str());
        if !ids.insert(itinerary.id.as_str())
            || !identifier(&itinerary.id, 512)
            || id != Some(itinerary.transporter_id.as_str())
            || bound.next().is_some()
            || id.is_none_or(|id| !identifier(id, 512))
            || itinerary
                .columns
                .iter()
                .any(|(c, _)| ["day", "publication_id", "itinerary_id"].contains(&c.as_str()))
        {
            return Ok(uncertain);
        }
        proposed.insert((itinerary.transporter_id.clone(), day.to_owned()));
    }
    let active: BTreeSet<_> = db
        .query_as::<(String, String)>(
            "SELECT DISTINCT i.transporter_id,i.day FROM itineraries i \
         JOIN route_publications p ON p.id=i.publication_id AND p.active=1",
            [],
        )?
        .into_iter()
        .collect();
    Ok(if active == proposed {
        PeopleImpact::Unchanged
    } else {
        uncertain
    })
}
