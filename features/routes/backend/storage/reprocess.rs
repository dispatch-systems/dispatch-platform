//! Whole-day rebuilding from stored responses, preserving kept phones and changed-day notes.
use super::{
    RoutesStore,
    reads::{PUBLICATION_SELECT, publication},
    stored_capture,
};
use crate::{
    api::types::RouteReprocess,
    backend::{
        ADAPTER_VERSION, changed,
        publishing::{Writes, share, write_itinerary, write_routes, write_shared},
        shaping::prepare,
        work_phones,
    },
};
use dispatch_core::{
    Error, Result,
    db::{Store, s},
};
use rusqlite::params;

/// Rebuilds every active publication's rows, or one day's, from the responses it
/// stored: what a release that reads more of them needs, without collecting again. An
/// itinerary its responses show no work phone for keeps the one its rows rebuilt stored
/// (`work_phones`). Each day rebuilt is noted, to tell of (`changed`).
pub(super) fn reprocess_routes(
    store: &Store,
    id: &str,
    day: Option<&str>,
) -> Result<RouteReprocess> {
    if let Some(day) = day {
        dispatch_core::foundation::validate::date(day)?;
    }
    let db = store.routes_db(id)?;
    let publications = db.all(
        "SELECT id,day,mode,station,service_area_id,provider,timezone,started_at,collected_at \
         FROM route_publications WHERE active=1 AND (?1 IS NULL OR day=?1) ORDER BY day",
        [day],
    )?;
    let mut rebuilt = Vec::new();
    for row in &publications {
        let publication = s(row, "id").to_owned();
        let capture = stored_capture(&db, row)?;
        let mut prepared = prepare(&capture)?;
        let routes = capture.route_summaries["rmsRouteSummaries"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        db.transaction(|| {
            work_phones::Stored::of(&db, &publication)?.keep_in(&mut prepared);
            // Rebuild shared staging from this capture too: keys omitted by the current
            // shaper must not replay an earlier staged identity over a later day's. Keep
            // this reset and the final share inside the same day transaction.
            for table in [
                "staged_addresses",
                "staged_drivers",
                "routes",
                "itineraries",
                "stops",
                "tasks",
                "driver_days",
                "breaks",
                "unknown_stops",
            ] {
                db.exec(
                    &format!("DELETE FROM {table} WHERE publication_id=?"),
                    [&publication],
                )?;
            }
            let day = s(row, "day");
            write_routes(&db, Writes::Staged, &publication, day, routes)?;
            for itinerary in &prepared {
                write_itinerary(&db, Writes::Staged, &publication, day, itinerary)?;
                write_shared(&db, Writes::Staged, &publication, day, itinerary)?;
            }
            share(&db, &publication, day)?;
            db.exec(
                "UPDATE route_publications SET route_count=?,itinerary_count=?,stop_count=?,task_count=?,\
                 adapter_version=? WHERE id=?",
                params![
                    routes.len() as i64,
                    prepared.len() as i64,
                    prepared.iter().map(|i| i.stops.len()).sum::<usize>() as i64,
                    prepared.iter().map(|i| i.tasks.len()).sum::<usize>() as i64,
                    ADAPTER_VERSION,
                    publication
                ],
            )?;
            changed::note(&db, day)?;
            Ok(())
        })?;
        let fresh = db
            .one(&format!("{PUBLICATION_SELECT} WHERE id=?"), [&publication])?
            .ok_or_else(|| Error::new("not_found", 404))?;
        rebuilt.push(self::publication(&fresh));
    }
    Ok(RouteReprocess {
        publications: rebuilt,
    })
}
