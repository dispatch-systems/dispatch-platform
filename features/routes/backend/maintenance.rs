//! Routes' upkeep: retiring route data past each DSP's retention window, deleting what no
//! reader sees any more, telling of the days whose route data changed that nothing told of, and
//! extracting the work phones of saved days kept before Routes kept them.
use super::{
    COLLECTION, RoutesStore,
    old_days::{self, Next},
    saving,
    scope::retention_start,
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use dispatch_cortex::routes::JOB_KIND;
use rusqlite::params;
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// Retires route data past each DSP's retention window, hourly, and deletes what no
/// reader sees any more in small steps, so the platform lock is never held for long; tells of
/// each day noted changed and not told of yet (`changed`); and extracts a few saved days' work
/// phones from their stored responses (`phone_backfill`).
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: clean,
};

fn failed(event: &str, error: &Error) {
    observability::event("error", event, json!({"error":error.code}));
}

fn clean(state: Arc<State>, expire: bool) -> Upkeep {
    Box::pin(async move {
        let dsps = state.read(|db| db.served_ids()).await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => return failed("routes_cleanup_failed", &error),
        };
        for dsp in dsps {
            // One disabled since the pass began waits, its days still to tell of, until it's
            // enabled; one disabled while its turn runs changes nothing after (`run_serving`).
            // Sweeping only lets go of what no reader sees.
            let id = dsp.clone();
            match state.read(move |db| db.serves_dsp(&id)).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    failed("routes_cleanup_failed", &error);
                    continue;
                }
            }
            super::changed::tell(&state, &dsp).await;
            if let Err(error) = super::phone_backfill::extract(&state, &dsp).await {
                failed("routes_phones_failed", &error);
            }
            if expire {
                let id = dsp.clone();
                if let Err(error) = state
                    .run_serving(dsp.clone(), super::DOMAIN, move |db| db.expire_routes(&id))
                    .await
                {
                    failed("routes_expiry_failed", &error);
                }
            }
            // Hourly, an old day's rows go once each itinerary's are proven to read the same
            // from its stored response: a look at a time, each write a short one of its own.
            // A day is some seventy steps; the rest waits for the next hour.
            for _ in 0..if expire { 400 } else { 0 } {
                let id = dsp.clone();
                match state.read(move |db| old_days::look(db, &id)).await {
                    Ok(Next::Done) => break,
                    Ok(Next::Look) => {}
                    Ok(Next::Write) => {
                        let id = dsp.clone();
                        let wrote = state
                            .run_serving(dsp.clone(), super::DOMAIN, move |db| {
                                old_days::write(db, &id)
                            })
                            .await;
                        if let Err(error) = wrote {
                            failed("routes_old_days_failed", &error);
                            break;
                        }
                    }
                    Err(error) => {
                        failed("routes_old_days_failed", &error);
                        break;
                    }
                }
            }
            // A day of rows is a few dozen steps; the rest waits for the next minute.
            let mut selected = None;
            for _ in 0..200 {
                let id = dsp.clone();
                match state
                    .run_bookkeeping(move |db| {
                        let more = sweep_routes_step(db, &id, &mut selected)?;
                        Ok((more, selected))
                    })
                    .await
                {
                    Ok((true, next)) => selected = next,
                    Ok((false, _)) => break,
                    Err(error) => {
                        failed("routes_cleanup_failed", &error);
                        break;
                    }
                }
            }
        }
    })
}

/// Retires the days a DSP's retention window has passed: they become inactive, and
/// `sweep_routes` deletes them in small steps. Addresses and drivers last seen before
/// the window go with them. Nothing is retired while the routes feature is off, so switching
/// it off never deletes. Answers how many days were retired.
pub(super) fn expire_routes(store: &Store, id: &str) -> Result<usize> {
    if !store.feature_enabled(id, COLLECTION)? {
        return Ok(0);
    }
    let Some(start) = retention_start(store, id)? else {
        return Ok(0);
    };
    let start = start.to_string();
    let db = store.routes_db(id)?;
    let retired = db.transaction(|| {
        let retired = db.exec(
            "UPDATE route_publications SET active=0 WHERE active=1 AND day<?",
            [&start],
        )?;
        db.exec("DELETE FROM addresses WHERE last_seen_day<?", [&start])?;
        db.exec("DELETE FROM drivers WHERE last_seen_day<?", [&start])?;
        Ok(retired)
    })?;
    if retired > 0 {
        store.audit(None, Some(id), "routes.data_expired", &start)?;
    }
    Ok(retired)
}

/// Reuse the chosen publication during a scheduler batch. Every step checks its
/// activity and job again, because another transition can run between deletions. A
/// publication a job or a save is still staging is kept.
pub(crate) fn sweep_routes_step(
    store: &Store,
    id: &str,
    selected: &mut Option<String>,
) -> Result<bool> {
    const BATCH: i64 = 2000;
    let db = store.routes_db(id)?;
    let mut running = store.active_job_ids(id, JOB_KIND)?;
    running.extend(saving::running(id));
    let retained = match selected.as_deref() {
        Some(publication) => db
            .one_as::<(String, String)>(
                "SELECT id,job_id FROM route_publications WHERE id=? AND active=0",
                [publication],
            )?
            .filter(|(_, job)| !running.contains(job))
            .map(|(publication, _)| publication),
        None => None,
    };
    let publication = match retained {
        Some(publication) => Some(publication),
        None => db
            .one_as::<(String,)>(
                "SELECT id FROM route_publications WHERE active=0 AND \
             job_id NOT IN (SELECT value FROM json_each(?)) ORDER BY collected_at,id LIMIT 1",
                [serde_json::to_string(&running)?],
            )?
            .map(|(publication,)| publication),
    };
    let Some(publication) = publication else {
        *selected = None;
        return Ok(false);
    };
    *selected = Some(publication.clone());
    let remaining = db.transaction(|| {
        for table in [
            "package_days",
            "tasks",
            "stops",
            "route_raw",
            "breaks",
            "unknown_stops",
            "staged_addresses",
            "staged_drivers",
            "driver_days",
            "itineraries",
            "routes",
        ] {
            let removed = db.exec(
                &format!(
                    "DELETE FROM {table} WHERE rowid IN \
                     (SELECT rowid FROM {table} WHERE publication_id=? LIMIT ?)"
                ),
                params![publication, BATCH],
            )?;
            if removed > 0 {
                return Ok(true);
            }
        }
        db.exec("DELETE FROM route_publications WHERE id=?", [&publication])?;
        Ok(false)
    })?;
    if !remaining {
        *selected = None;
    }
    Ok(true)
}
