//! Live route data: today's routes as the last whole live read left them, kept in place read by
//! read in a database of its own beside historical route data (`routelive`); emptied as a new day
//! starts, and saved into historical route data as one of the day's reads. A feature reading routes
//! live keeps its reads here.
mod people_impact;

use people_impact::people_impact;

use super::{
    ADAPTER_VERSION, COLLECTION, DOMAIN, RoutesStore,
    publishing::{
        ASSIGNED_COLUMNS, DriverWrite, Writes, person_type, route_id, shared, shared_row, tasks,
        write_itinerary, write_raw, write_routes, write_shared,
    },
    saving,
    scope::{day_allowed, day_request, routes_day, routes_today},
    shaping::{PreparedItinerary, PreparedResponse, Shaper, prepare_lists, text},
    staging::blocking,
    storage::stored_capture,
    work_phones::{PHONES_SINCE, Stored},
};
use dispatch_core::{
    Error, Result, State,
    db::{Db, Store, at, s},
    ensure,
    foundation::crypto,
    manifest::{
        Counts,
        events::{self, PeopleChanged},
    },
    server::cache::PeopleImpact,
};
use dispatch_cortex::{
    self as cortex,
    routes::{Capture, Mode, Request},
};
use dispatch_routes_contract::RouteDataChanged;
use rusqlite::params;
use serde_json::Value;
use std::sync::Arc;

/// What keeping a live read counted, for its record: the itineraries it listed, and the rows it
/// wrote or deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveKept {
    pub listed: u64,
    pub changed: u64,
}
impl LiveKept {
    /// Each count under its name, as a live read's record keeps them.
    pub fn counts(&self) -> Counts {
        vec![("listed", self.listed), ("changed", self.changed)]
    }
}

/// What keeping a live read did, told of once it's committed.
#[derive(Debug)]
struct Kept {
    /// What it counted, for its record.
    counts: LiveKept,
    /// Whether it named a driver live route data didn't.
    named: bool,
    /// Whether it changed what the day's readers read: the day's own rows, its routes and
    /// itineraries with what they hold, or the day itself, one live route data didn't hold, or held
    /// as shaped before itineraries kept their phones, whose phones they read from then on, though
    /// it shows none. Not the drivers and addresses the days share, which no reader of the day reads,
    /// as a driver's phone gone from their record while their itinerary keeps it; nor the responses
    /// kept whole beside them, which a read changes each time Cortex answers its list
    /// (`backendTime`).
    data_changed: bool,
}

/// A whole live read, checked and shaped, ready to keep: its lists with their responses
/// compressed as they're stored, and each itinerary's rows.
struct Shaped {
    lists: Capture,
    responses: Vec<PreparedResponse>,
    itineraries: Vec<PreparedItinerary>,
}
impl Shaped {
    /// `capture` checked whole against `request`, in the scope its days are read in, then shaped.
    fn new(mut capture: Capture, request: &Request) -> Result<Self> {
        capture.validate(request)?;
        // A list without its itineraries names none only by mistake.
        ensure(
            capture.summaries["itinerarySummaries"].is_array(),
            "routes_capture_invalid",
            502,
        )?;
        request
            .scope_request()
            .validate_scope(&capture.scope)
            .map_err(|_| Error::new("routes_scope_mismatch", 502))?;
        let shaper = Shaper::new(&capture);
        // Each itinerary's response goes as its rows are made, so only one is held twice.
        let itineraries = std::mem::take(&mut capture.itineraries)
            .into_iter()
            .map(|captured| shaper.shape(&captured))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            responses: prepare_lists(&capture)?,
            lists: capture,
            itineraries,
        })
    }
}

/// What a live read asks Cortex for: a snapshot of today where the DSP is, at its station, as a
/// job asks for a day and as `keep_live` checks the read. Refuses a DSP without a station
/// (`routes_station_required`).
pub fn live_request(store: &Store, dsp: &str) -> Result<Request> {
    let today = routes_today(store, dsp)?.to_string();
    day_request(store, dsp, &today, Mode::Snapshot)
}

/// The key a live read is saved into historical route data under: its day and when it was
/// collected, the same however often it's saved.
fn key(day: &str, collected: i64) -> String {
    format!("live:{day}:{collected}")
}

/// Keeps `capture`, a whole read of today where the DSP is, as live route data's day: checked
/// whole against what a snapshot of that day at the DSP's station asks, in the scope its days are
/// read in, then kept in place in one transaction, so a reader sees the last read or this one
/// whole. A row new to it is inserted, one that changed is updated, one it no longer holds is
/// deleted and one unchanged is left unwritten; an itinerary it shows no work phone for keeps the
/// one the day's last read stored for it, unless it shows that phone on another driver's itinerary
/// (`work_phones`); and the drivers its route list assigns are named. A day other than the one live
/// route data holds, or another scope, starts it afresh. A read collected no later than the one it
/// holds changes nothing. It tells Driver Match of a driver live route data didn't name
/// (`people.changed`), then every feature that hears it of the day's route data changed, when it
/// changed what the day's readers read (`routes.data_changed`).
///
/// It refuses, changing nothing, a day other than today where the DSP is
/// (`routes_day_not_available`); a DSP without a station (`routes_station_required`); a read that
/// isn't whole, or a list naming no itinerary once the day has had one, which is a lost read
/// (`routes_capture_invalid`, `routes_scope_mismatch`, `routes_source_too_large`); a read taken
/// under another `revision` of the DSP's Cortex connection than it has as the read is written,
/// as once its credentials were replaced or it was disabled (`connection_changed`); and keeping
/// while Routes is off for the DSP (`routes_off`).
pub async fn keep_live(
    state: &Arc<State>,
    dsp: &str,
    capture: Capture,
    revision: i64,
) -> Result<LiveKept> {
    let d = dsp.to_owned();
    let on = move |db: &Store| ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409);
    let read_day = capture.scope.date.clone();
    // Like a job's work, a read has no one to try it again.
    State::patiently(async {
        let (d, day, check) = (dsp.to_owned(), read_day.clone(), on.clone());
        let request = state
            .read(move |db| {
                check(db)?;
                day_request(db, &d, &day, Mode::Snapshot)
            })
            .await?;
        // Checked and shaped on a blocking thread, without the platform lock.
        let shaped = tokio::task::spawn_blocking(move || Shaped::new(capture, &request))
            .await
            .map_err(blocking("routes.keep_failed"))??;
        let kept = state
            .run_scoped_prepared(
                dsp,
                DOMAIN,
                move |db, dsp| {
                    on(db)?;
                    let current = db.connection_revision(dsp, cortex::PROVIDER)? == Some(revision);
                    ensure(current, "connection_changed", 409)?;
                    day_allowed(
                        Mode::Snapshot,
                        &shaped.lists.scope.date,
                        routes_today(db, dsp)?,
                    )?;
                    // Additional proof failures never turn an otherwise valid poll into an error.
                    let impact = people_impact(db, dsp, &shaped).unwrap_or(PeopleImpact::MayChange);
                    Ok((shaped, impact))
                },
                |db, dsp, mut shaped| keep(db, dsp, &mut shaped),
            )
            .await?;
        if kept.named {
            events::publish(state, dsp, PeopleChanged).await;
        }
        if kept.data_changed {
            events::publish(state, dsp, RouteDataChanged { day: read_day }).await;
        }
        Ok(kept.counts)
    })
    .await
}

/// The live store's one current publication. Inactive rows never identify its read; more than
/// one active scope is ambiguous even though the database's uniqueness rule is per scope.
fn held_publication(db: &Db) -> Result<Option<Value>> {
    let rows = db.all(
        "SELECT id,job_id,day,mode,station,service_area_id,provider,timezone,started_at,\
         collected_at,adapter_version FROM route_publications WHERE active=1 LIMIT 2",
        [],
    )?;
    ensure(rows.len() <= 1, "routes_capture_invalid", 502)?;
    Ok(rows.into_iter().next())
}

/// Keeps a shaped read in the DSP's live route data in place, in one transaction, answering what
/// it did: each of its itineraries that shows no work phone keeps the one the day's last read
/// stored for it (`Stored`). Refuses a read of a day that isn't today as it's written, however long
/// reading and shaping it took.
fn keep(store: &Store, dsp: &str, read: &mut Shaped) -> Result<Kept> {
    let (lists, scope) = (&read.lists, &read.lists.scope);
    let day = scope.date.as_str();
    day_allowed(Mode::Snapshot, day, routes_today(store, dsp)?)?;
    let db = &*store.routes_live_db(dsp)?;
    let listed: Vec<String> = read.itineraries.iter().map(|i| i.id.clone()).collect();
    db.transaction(|| {
        let held = held_publication(db)?;
        let same = held.as_ref().filter(|row| {
            (s(row, "day"), s(row, "station"), s(row, "service_area_id"), s(row, "provider"))
                == (
                    day,
                    scope.station.as_str(),
                    scope.service_area_id.as_str(),
                    scope.provider.as_str(),
                )
        });
        let (publication, anew) = match same {
            Some(row) => {
                if s(row, "collected_at") >= at(lists.finished_at).as_str() {
                    let counts = LiveKept {
                        listed: listed.len() as u64,
                        ..LiveKept::default()
                    };
                    return Ok(Kept {
                        counts,
                        named: false,
                        data_changed: false,
                    });
                }
                ensure(
                    !listed.is_empty()
                        || db.count(
                            "SELECT count(*) FROM itineraries WHERE publication_id=?",
                            [s(row, "id")],
                        )? == 0,
                    "routes_capture_invalid",
                    502,
                )?;
                let unextracted = row["adapter_version"]
                    .as_i64()
                    .is_none_or(|version| version < PHONES_SINCE);
                (s(row, "id").to_owned(), unextracted)
            }
            None => {
                // Starting afresh clears the live store even when only inactive rows remain:
                // their shared names must not carry into a new live day.
                if held.is_some()
                    || db.one("SELECT 1 FROM route_publications LIMIT 1", [])?.is_some()
                {
                    empty(db)?;
                }
                let publication = crypto::id("routes")?;
                db.exec(
                    "INSERT INTO route_publications(id,job_id,day,station,service_area_id,provider,\
                     timezone,mode,started_at,collected_at,active,route_count,itinerary_count,\
                     stop_count,task_count,adapter_version) VALUES (?,?,?,?,?,?,?,?,?,?,1,0,0,0,0,?)",
                    params![
                        publication,
                        key(day, lists.finished_at),
                        day,
                        scope.station,
                        scope.service_area_id,
                        scope.provider,
                        scope.timezone,
                        lists.mode.as_str(),
                        at(lists.started_at),
                        at(lists.finished_at),
                        ADAPTER_VERSION
                    ],
                )?;
                (publication, true)
            }
        };
        Stored::of(db, &publication)?.keep_in(&mut read.itineraries);
        let drivers = db.count("SELECT count(*) FROM drivers", [])?;
        let routes = lists.route_summaries["rmsRouteSummaries"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        // The rows the day's readers read, counted apart from the rows the days share and the
        // responses kept whole beside them.
        let mut read_rows = write_routes(db, Writes::InPlace, &publication, day, routes)?
            + prune_routes(db, &publication, routes)?;
        let mut shared = write_assigned(
            db,
            day,
            &scope.provider,
            &lists.route_summaries,
            &read.itineraries,
        )?;
        let mut raw = 0;
        for response in &read.responses {
            raw += write_raw(
                db,
                Writes::InPlace,
                &publication,
                response.name,
                response.bytes,
                &response.body,
            )?;
        }
        for itinerary in &read.itineraries {
            let name = format!("itinerary:{}", itinerary.id);
            read_rows += write_itinerary(db, Writes::InPlace, &publication, day, itinerary)?;
            shared += write_shared(db, Writes::InPlace, &publication, day, itinerary)?;
            raw += write_raw(
                db,
                Writes::InPlace,
                &publication,
                &name,
                itinerary.raw_bytes,
                &itinerary.raw,
            )?;
            read_rows += prune_itinerary(db, &publication, itinerary)?;
        }
        // An itinerary no longer listed goes with its response.
        read_rows += prune_itineraries(db, &publication, &listed)?;
        // Every itinerary listed, each once.
        ensure(
            db.count(
                "SELECT count(*) FROM itineraries WHERE publication_id=?",
                [&publication],
            )? == listed.len() as i64,
            "routes_capture_invalid",
            502,
        )?;
        db.exec(
            "UPDATE route_publications SET job_id=?,timezone=?,started_at=?,collected_at=?,\
             route_count=?,itinerary_count=?,stop_count=?,task_count=?,adapter_version=? WHERE id=?",
            params![
                key(day, lists.finished_at),
                scope.timezone,
                at(lists.started_at),
                at(lists.finished_at),
                routes.len() as i64,
                listed.len() as i64,
                read.itineraries.iter().map(|i| i.stops.len()).sum::<usize>() as i64,
                read.itineraries.iter().map(|i| i.tasks.len()).sum::<usize>() as i64,
                // Every row is this read's now, shaped as it was.
                ADAPTER_VERSION,
                publication
            ],
        )?;
        let counts = LiveKept {
            listed: listed.len() as u64,
            changed: (read_rows + shared + raw) as u64,
        };
        Ok(Kept {
            counts,
            named: db.count("SELECT count(*) FROM drivers", [])? > drivers,
            data_changed: anew || read_rows > 0,
        })
    })
}
/// Deletes the planned routes live route data's day holds that `routes` no longer lists. Answers
/// the rows deleted.
fn prune_routes(db: &Db, publication: &str, routes: &[Value]) -> Result<usize> {
    let kept: Vec<String> = routes.iter().map(route_id).collect();
    db.exec(
        "DELETE FROM routes WHERE publication_id=?1 AND route_id NOT IN (SELECT value FROM json_each(?2))",
        params![publication, serde_json::to_string(&kept)?],
    )
}
/// The drivers the provider's routes on the day's route list assign who have none of
/// `itineraries`, the read's, written over the rows of the drivers live route data names where
/// they changed, inside the caller's transaction: a driver assigned to a route is known before
/// their itinerary is read, which names them from then on. Their work phone is left as it was:
/// before a driver signs in, the list shows the last phone they had. Answers the rows written.
fn write_assigned(
    db: &Db,
    day: &str,
    provider: &str,
    route_summaries: &Value,
    itineraries: &[PreparedItinerary],
) -> Result<usize> {
    let mut upsert =
        db.0.prepare_cached(&shared("drivers", "transporter_id", ASSIGNED_COLUMNS))?;
    let mut written = 0;
    for driver in assigned_drivers(provider, route_summaries, itineraries) {
        written += upsert.execute(shared_row(Writes::InPlace, "", day, driver.sent()))?;
    }
    Ok(written)
}

/// In the same order as the route-list writer, including duplicate transporter records.
/// The set avoids scanning every itinerary for every assigned driver.
fn assigned_drivers(
    provider: &str,
    route_summaries: &Value,
    itineraries: &[PreparedItinerary],
) -> Vec<DriverWrite> {
    let present: std::collections::BTreeSet<&str> = itineraries
        .iter()
        .map(|i| i.transporter_id.as_str())
        .collect();
    let routes = route_summaries["rmsRouteSummaries"].as_array();
    let assigned: std::collections::BTreeSet<String> = routes
        .into_iter()
        .flatten()
        .filter(|route| text(&route["companyId"]).as_deref() == Some(provider))
        .flat_map(|route| route["transporters"].as_array().into_iter().flatten())
        .filter_map(|transporter| text(&transporter["transporterId"]))
        .filter(|id| !present.contains(id.as_str()))
        .collect();
    let mut drivers = Vec::new();
    for person in route_summaries["transporters"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(id) = text(&person["transporterId"]).filter(|id| assigned.contains(id)) else {
            continue;
        };
        drivers.push(DriverWrite::new(
            id,
            person,
            vec![text(&person["initials"]).into(), person_type(person).into()],
        ));
    }
    drivers
}
/// Deletes what live route data's day holds of an itinerary that its read no longer does: stops
/// and tasks gone from it, and breaks and unknown stops past their count. Answers the rows
/// deleted.
fn prune_itinerary(db: &Db, publication: &str, itinerary: &PreparedItinerary) -> Result<usize> {
    let stops: Vec<&str> = itinerary.stops.iter().map(|s| s.id.as_str()).collect();
    let tasks: Vec<&str> = tasks(itinerary).map(|(t, _)| t.id.as_str()).collect();
    let planned = itinerary.breaks.iter().filter(|b| b.planned).count();
    let taken = itinerary.breaks.len() - planned;
    let id = &itinerary.id;
    Ok(db.exec(
        "DELETE FROM stops WHERE publication_id=?1 AND itinerary_id=?2 \
         AND stop_id NOT IN (SELECT value FROM json_each(?3))",
        params![publication, id, serde_json::to_string(&stops)?],
    )? + db.exec(
        "DELETE FROM tasks WHERE publication_id=?1 AND itinerary_id=?2 \
         AND task_id NOT IN (SELECT value FROM json_each(?3))",
        params![publication, id, serde_json::to_string(&tasks)?],
    )? + db.exec(
        "DELETE FROM breaks WHERE publication_id=?1 AND itinerary_id=?2 \
         AND ordinal>=CASE planned WHEN 1 THEN ?4 ELSE ?3 END",
        params![publication, id, taken as i64, planned as i64],
    )? + db.exec(
        "DELETE FROM unknown_stops WHERE publication_id=?1 AND itinerary_id=?2 AND ordinal>=?3",
        params![publication, id, itinerary.unknown_stops.len() as i64],
    )?)
}
/// Deletes the itineraries live route data's day holds that its read no longer lists, with every
/// row and response of theirs. Answers the rows deleted.
fn prune_itineraries(db: &Db, publication: &str, listed: &[String]) -> Result<usize> {
    let listed = serde_json::to_string(listed)?;
    let mut deleted = 0;
    for table in [
        "itineraries",
        "stops",
        "tasks",
        "driver_days",
        "breaks",
        "unknown_stops",
    ] {
        deleted += db.exec(
            &format!(
                "DELETE FROM {table} WHERE publication_id=?1 \
                 AND itinerary_id NOT IN (SELECT value FROM json_each(?2))"
            ),
            params![publication, listed],
        )?;
    }
    deleted += db.exec(
        "DELETE FROM route_raw WHERE publication_id=?1 AND name LIKE 'itinerary:%' \
         AND substr(name,11) NOT IN (SELECT value FROM json_each(?2))",
        params![publication, listed],
    )?;
    Ok(deleted)
}
/// Empties live route data, inside the caller's transaction: every read, driver and address.
fn empty(db: &Db) -> Result<()> {
    for table in [
        "tasks",
        "stops",
        "route_raw",
        "breaks",
        "unknown_stops",
        "driver_days",
        "itineraries",
        "routes",
        "route_publications",
        "addresses",
        "drivers",
    ] {
        db.exec(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Empties live route data, as a new day starts or a saved one stops, and gives back the
/// file's space, its log's with it; then tells every feature that hears it that the route data of
/// the day it held changed (`routes.data_changed`), each time, so a call made again after one that
/// couldn't give the space back tells them too, of today where the DSP is, as it holds no day by
/// then.
/// Refuses while Routes is off for the DSP (`routes_off`).
pub async fn clear_live(state: &Arc<State>, dsp: &str) -> Result<()> {
    let d = dsp.to_owned();
    let day = State::patiently(state.run_scoped(dsp, DOMAIN, move |db| {
        ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409)?;
        let live = db.routes_live_db(&d)?;
        let held = live.transaction(|| {
            let held = held_publication(&live)?;
            empty(&live)?;
            Ok(held.map(|row| s(&row, "day").to_owned()))
        })?;
        live.0
            .execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        match held {
            Some(day) => Ok(day),
            None => Ok(routes_today(db, &d)?.to_string()),
        }
    }))
    .await?;
    events::publish(state, dsp, RouteDataChanged { day }).await;
    Ok(())
}

/// Saves the last whole read live route data holds into historical route data as one of its
/// day's reads, through `save_day`: its responses as they were read, each itinerary's work phone as
/// live route data keeps it, kept ones too, under a key of the read's own, so saving it again, as
/// after a restart, saves nothing twice. It is a final read only if it was read once its day had
/// ended where the DSP is, a snapshot otherwise, as the platform has a day's reads. Live route data
/// keeps it. Answers whether it is the day's read now, or none when live route data holds no
/// day; features hear of a day saved as `save_day` tells them. It refuses as `save_day` does,
/// and while Routes is off for the DSP (`routes_off`).
pub async fn save_live_day(state: &Arc<State>, dsp: &str) -> Result<Option<bool>> {
    let d = dsp.to_owned();
    let held = state
        .read(move |db| {
            ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409)?;
            let live = db.routes_live_db(&d)?;
            let Some(row) = held_publication(&live)? else {
                return Ok(None);
            };
            let mut capture = stored_capture(&live, &row)?;
            let read_on = routes_day(db, &d, capture.finished_at)?;
            capture.mode = match day_allowed(Mode::Final, &capture.scope.date, read_on) {
                Ok(()) => Mode::Final,
                Err(_) => Mode::Snapshot,
            };
            let stored = Stored::of(&live, s(&row, "id"))?;
            Ok(Some((s(&row, "job_id").to_owned(), capture, stored)))
        })
        .await?;
    let Some((key, capture, stored)) = held else {
        return Ok(None);
    };
    saving::save_keeping(state, dsp, &key, capture, stored)
        .await
        .map(Some)
}

/// Saves the last whole read live route data holds into historical route data
/// (`save_live_day`), then empties live route data (`clear_live`): once the day is saved, as the
/// day's read or one collected no later than it has, or once it holds no day, as after an
/// earlier call emptied it but couldn't give its file's space back. Answers as `save_live_day`
/// does; features hear of the save and of the emptying, each once it's done. A save refused
/// leaves live route data as it was.
pub async fn save_and_clear_live(state: &Arc<State>, dsp: &str) -> Result<Option<bool>> {
    let saved = save_live_day(state, dsp).await?;
    clear_live(state, dsp).await?;
    Ok(saved)
}

#[cfg(test)]
#[path = "../tests/backend/live.rs"]
mod tests;
