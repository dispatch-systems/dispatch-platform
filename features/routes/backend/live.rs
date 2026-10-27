//! Live route data: today's routes as the last whole live read left them, kept in place read by
//! read in a database of its own beside historical route data (`routelive`), with who holds
//! which work phone (`phones`); emptied as a new day starts, and saved into historical route data
//! as one of the day's reads. A feature reading routes live keeps its reads here.
use super::*;
use dispatch_core::{
    State,
    foundation::crypto,
    manifest::{
        Counts,
        events::{self, PeopleChanged},
    },
};
use dispatch_routes_contract::WorkPhonesChanged;
use std::sync::Arc;

/// What keeping a live read counted, for its record: the itineraries it listed, the rows it
/// wrote or deleted, the work phones held after it, and the holds it claimed, moved or ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveKept {
    pub listed: u64,
    pub changed: u64,
    pub holding: u64,
    pub phone_events: u64,
}
impl LiveKept {
    /// Each count under its name, as a live read's record keeps them.
    pub fn counts(&self) -> Counts {
        vec![
            ("listed", self.listed),
            ("changed", self.changed),
            ("holding", self.holding),
            ("phoneEvents", self.phone_events),
        ]
    }
}

/// A whole live read, checked and shaped, ready to keep: its lists with their responses
/// compressed as they're stored, each itinerary's rows, and the work phones it shows.
struct Shaped {
    lists: Capture,
    responses: Vec<(&'static str, usize, Vec<u8>)>,
    itineraries: Vec<PreparedItinerary>,
    shown: phones::Shown,
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
        let listed = listed(&capture.summaries, &capture.scope);
        let shaper = Shaper::new(&capture);
        // Each itinerary's response goes as its rows are made, so only one is held twice.
        let itineraries = std::mem::take(&mut capture.itineraries)
            .into_iter()
            .map(|captured| shaper.shape(&captured))
            .collect::<Result<Vec<_>>>()?;
        let shown = phones::Shown::read(
            &capture.summaries,
            &listed,
            itineraries
                .iter()
                .flat_map(|i| i.work_phones.iter().cloned()),
        );
        let mut responses = Vec::new();
        for (name, list) in [
            ("summaries", &capture.summaries),
            ("route_summaries", &capture.route_summaries),
        ] {
            let body = serde_json::to_vec(list)?;
            responses.push((name, body.len(), gzip(&body)?));
        }
        Ok(Self {
            lists: capture,
            responses,
            itineraries,
            shown,
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
/// deleted and one unchanged is left unwritten; the drivers its route list assigns are named; and
/// who holds which work phone is settled from it. A day other than the one live route data holds,
/// or another scope, starts it afresh. A read collected no later than the one it holds changes
/// nothing. It tells Driver Match of a driver live route data didn't name (`people.changed`), then
/// every feature that hears it of a hold claimed, moved, handed over or withdrawn on the read's day
/// (`routes.work_phones_changed`).
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
        let d = dsp.to_owned();
        let (kept, named) = state
            .run_scoped(dsp, DOMAIN, move |db| {
                on(db)?;
                let current = db.connection_revision(&d, cortex::PROVIDER)? == Some(revision);
                ensure(current, "connection_changed", 409)?;
                keep(db, &d, &shaped)
            })
            .await?;
        if named {
            events::publish(state, dsp, PeopleChanged).await;
        }
        if kept.phone_events > 0 {
            events::publish(state, dsp, WorkPhonesChanged { day: read_day }).await;
        }
        Ok(kept)
    })
    .await
}
/// Keeps a shaped read in the DSP's live route data in place, in one transaction, answering what
/// it counted and whether it named a driver live route data didn't. Refuses a read of a day that
/// isn't today as it's written, however long reading and shaping it took.
fn keep(store: &Store, dsp: &str, read: &Shaped) -> Result<(LiveKept, bool)> {
    let (lists, scope) = (&read.lists, &read.lists.scope);
    let day = scope.date.as_str();
    day_allowed(Mode::Snapshot, day, routes_today(store, dsp)?)?;
    let db = &*store.routes_live_db(dsp)?;
    let listed: Vec<String> = read.itineraries.iter().map(|i| i.id.clone()).collect();
    db.transaction(|| {
        let held = db.one(
            "SELECT id,day,station,service_area_id,provider,collected_at,\
             (SELECT count(*) FROM itineraries) itineraries FROM route_publications",
            [],
        )?;
        let same = held.as_ref().filter(|row| {
            (s(row, "day"), s(row, "station"), s(row, "service_area_id"), s(row, "provider"))
                == (
                    day,
                    scope.station.as_str(),
                    scope.service_area_id.as_str(),
                    scope.provider.as_str(),
                )
        });
        let publication = match same {
            Some(row) => {
                if s(row, "collected_at") >= at(lists.finished_at).as_str() {
                    let holding = db.count(
                        "SELECT count(*) FROM work_phones WHERE day=? AND until IS NULL",
                        [day],
                    )?;
                    let kept = LiveKept {
                        listed: listed.len() as u64,
                        holding: holding as u64,
                        ..LiveKept::default()
                    };
                    return Ok((kept, false));
                }
                ensure(
                    !listed.is_empty() || row["itineraries"] == 0,
                    "routes_capture_invalid",
                    502,
                )?;
                s(row, "id").to_owned()
            }
            None => {
                if held.is_some() {
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
                publication
            }
        };
        let drivers = db.count("SELECT count(*) FROM drivers", [])?;
        let routes = lists.route_summaries["rmsRouteSummaries"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut changed = write_routes(db, Writes::InPlace, &publication, day, routes)?
            + prune_routes(db, &publication, routes)?
            + write_assigned(
                db,
                day,
                &scope.provider,
                &lists.route_summaries,
                &read.itineraries,
            )?;
        for (name, bytes, body) in &read.responses {
            changed += write_raw(db, Writes::InPlace, &publication, name, *bytes, body)?;
        }
        for itinerary in &read.itineraries {
            let name = format!("itinerary:{}", itinerary.id);
            changed += write_itinerary(db, Writes::InPlace, &publication, day, itinerary)?
                + write_raw(
                    db,
                    Writes::InPlace,
                    &publication,
                    &name,
                    itinerary.raw_bytes,
                    &itinerary.raw,
                )?
                + prune_itinerary(db, &publication, itinerary)?;
        }
        changed += prune_itineraries(db, &publication, &listed)?;
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
        let settled = phones::settle(db, day, lists.finished_at, &read.shown)?;
        let kept = LiveKept {
            listed: listed.len() as u64,
            changed: changed as u64,
            holding: settled.holding,
            phone_events: settled.events,
        };
        Ok((kept, db.count("SELECT count(*) FROM drivers", [])? > drivers))
    })
}
/// Empties live route data, inside the caller's transaction: every read, driver, address and
/// work phone.
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
        "work_phones",
        "work_phones_shown",
    ] {
        db.exec(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Empties live route data, as a new day starts or a saved one stops, and gives back the
/// file's space, its log's with it; then tells every feature that hears it that its holds on the
/// work phones of the day it held are gone (`routes.work_phones_changed`), each time, so a call
/// made again after one that couldn't give the space back tells them too, of today where the DSP
/// is, as it holds no day by then. Refuses while Routes is off for the DSP (`routes_off`).
pub async fn clear_live(state: &Arc<State>, dsp: &str) -> Result<()> {
    let d = dsp.to_owned();
    let day = State::patiently(state.run_scoped(dsp, DOMAIN, move |db| {
        ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409)?;
        let live = db.routes_live_db(&d)?;
        let held = live.transaction(|| {
            let held = live.one("SELECT day FROM route_publications", [])?;
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
    events::publish(state, dsp, WorkPhonesChanged { day }).await;
    Ok(())
}

/// Saves the last whole read live route data holds into historical route data as one of its
/// day's reads, with every hold of the day on the company's work phones, through `save_day`: its
/// responses as they were read, under a key of the read's own, so saving it again, as after a
/// restart, saves nothing twice. It is a final read only if it was read once its day had ended
/// where the DSP is, a snapshot otherwise, as the platform has a day's reads. Live route data
/// keeps it. Answers whether it is the day's read now, or none when live route data holds no
/// day; features hear of a day saved as `save_day` tells them. It refuses as `save_day` does,
/// and while Routes is off for the DSP (`routes_off`).
pub async fn save_live_day(state: &Arc<State>, dsp: &str) -> Result<Option<bool>> {
    let d = dsp.to_owned();
    let held = state
        .read(move |db| {
            ensure(db.feature_enabled(&d, COLLECTION)?, "routes_off", 409)?;
            let live = db.routes_live_db(&d)?;
            let Some(row) = live.one(
                "SELECT id,job_id,day,mode,station,service_area_id,provider,timezone,started_at,\
                 collected_at FROM route_publications",
                [],
            )?
            else {
                return Ok(None);
            };
            let mut capture = stored_capture(&live, &row)?;
            let read_on = routes_day(db, &d, capture.finished_at)?;
            capture.mode = match day_allowed(Mode::Final, &capture.scope.date, read_on) {
                Ok(()) => Mode::Final,
                Err(_) => Mode::Snapshot,
            };
            let phones = phones::holds(&live, &capture.scope.date)?;
            Ok(Some((s(&row, "job_id").to_owned(), capture, phones)))
        })
        .await?;
    let Some((key, capture, phones)) = held else {
        return Ok(None);
    };
    save_day(state, dsp, &key, capture, phones).await.map(Some)
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
