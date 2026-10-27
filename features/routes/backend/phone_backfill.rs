//! Filling in the itineraries' work phones of the saved days kept before Routes kept them, from
//! each day's stored responses, by the minute's upkeep, as shaping takes them
//! (`work_phones`).
use super::{work_phones::PHONES_SINCE, *};
use dispatch_core::{State, manifest::events};
use dispatch_routes_contract::RouteDataChanged;
use std::sync::Arc;

/// The most days of a DSP the minute's upkeep tries to extract work phones for, newest first, those
/// whose stored responses can't be read among them: each day's are read whole, so a year of days
/// takes a couple of hours.
const EXTRACTED_PER_PASS: i64 = 4;

/// Extracts the itineraries' work phones of up to `EXTRACTED_PER_PASS` of the DSP's saved days
/// kept before Routes kept them, newest first, from their stored responses, while Routes is on for
/// the DSP; then tells every feature that hears it of each day, once it's kept
/// (`routes.data_changed`). Only a day shaped by the version just before is: one older needs a
/// reprocess, which shapes it whole. A day whose stored responses can't be read is logged and left
/// as it was, one of the days tried.
pub(super) async fn extract(state: &Arc<State>, dsp: &str) -> Result<()> {
    let d = dsp.to_owned();
    let days = state
        .read(move |db| {
            if !db.feature_enabled(&d, COLLECTION)? {
                return Ok(Vec::new());
            }
            db.routes_db(&d)?.all(
                "SELECT id,day,mode,station,service_area_id,provider,timezone,started_at,\
                 collected_at FROM route_publications WHERE active=1 AND adapter_version=?1 \
                 ORDER BY day DESC LIMIT ?2",
                params![PHONES_SINCE - 1, EXTRACTED_PER_PASS],
            )
        })
        .await?;
    for row in days {
        let (d, stored) = (dsp.to_owned(), row.clone());
        let read = state.read(move |db| phones_kept(&*db.routes_db(&d)?, &stored));
        let phones = match read.await {
            Ok(phones) => phones,
            Err(error) => {
                observability::event(
                    "error",
                    "routes.phones_unextracted",
                    json!({"dspId":dsp,"day":s(&row, "day"),"error":error.code}),
                );
                continue;
            }
        };
        let (d, publication) = (dsp.to_owned(), s(&row, "id").to_owned());
        let kept = state
            .run_scoped(dsp, DOMAIN, move |db| {
                let routes = db.routes_db(&d)?;
                routes.transaction(|| {
                    // Still the day's read, shaped as it was read: one since, or a reprocess, wins.
                    let current = routes.exec(
                        "UPDATE route_publications SET adapter_version=?1 \
                         WHERE id=?2 AND active=1 AND adapter_version=?1-1",
                        params![PHONES_SINCE, publication],
                    )?;
                    if current == 0 {
                        return Ok(false);
                    }
                    for (itinerary, phone) in &phones {
                        routes.exec(
                            "UPDATE itineraries SET work_phone=? \
                             WHERE publication_id=? AND itinerary_id=?",
                            params![phone, publication, itinerary],
                        )?;
                    }
                    Ok(true)
                })
            })
            .await?;
        if kept {
            let day = s(&row, "day").to_owned();
            events::publish(state, dsp, RouteDataChanged { day }).await;
        }
    }
    Ok(())
}
/// Each itinerary's work phone in `publication`'s stored responses, by the itinerary, as shaping
/// takes it: only the drivers each response names are read of it.
fn phones_kept(routes: &Db, publication: &Value) -> Result<Vec<(String, Option<String>)>> {
    let capture = stored_capture(routes, publication)?;
    let shaper = Shaper::new(&capture);
    let invalid = |_| Error::new("routes_capture_invalid", 502);
    (capture.itineraries.iter())
        .map(|itinerary| {
            let own = wire::drivers(&itinerary.detail).map_err(invalid)?;
            let phone = shaper.work_phone(&itinerary.transporter_id, &own.transporters);
            Ok((itinerary.id.clone(), phone))
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/backend/phone_backfill.rs"]
mod tests;
