//! Routes' settings: how long the DSP keeps its route data, on Settings' Data page, and the
//! window a DSP chose before Settings kept it, moved in once.
use super::{MAX_RETENTION_DAYS, MIN_RETENTION_DAYS, RoutesStore, STORAGE, routes_today};
use chrono::Duration;
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, section},
};
use serde::Deserialize;
use serde_json::{Value, json};

/// How many days of route data the DSP keeps, counted back from today where it is; none keeps
/// every day. Hourly, the days past it are deleted, so a window that passes a stored day asks
/// first.
pub const RETENTION: Section = section(
    "routes.retention",
    "Route data",
    "routes.manage",
    &[number(
        "days",
        "Keep route data for",
        MIN_RETENTION_DAYS,
        MAX_RETENTION_DAYS,
        None,
    )
    .unit("days")
    .presets(&[
        (30, "30 days"),
        (90, "90 days"),
        (180, "6 months"),
        (365, "1 year"),
        (730, "2 years"),
        (1825, "5 years"),
    ])
    .none("Every day")],
)
.on("data")
.keywords(&["retention"])
.warn(deletes);

/// `RETENTION`'s values, as Routes reads them.
#[derive(Deserialize)]
pub struct Retention {
    pub days: Option<i64>,
}

/// The words to confirm a window with when it passes a day the DSP's route data holds: the
/// first day it keeps, as expiry counts it. Every day kept, or none stored before it, asks
/// nothing. It reads only `dsp`'s route data.
fn deletes(db: &Store, dsp: &str, _: &Value, after: &Value) -> Result<Option<String>> {
    let Some(days) = after["days"].as_i64() else {
        return Ok(None);
    };
    let start = routes_today(db, dsp)? - Duration::days(days);
    let passed = db.routes_db(dsp)?.one(
        "SELECT 1 FROM route_publications WHERE active=1 AND day<? LIMIT 1",
        [start.to_string()],
    )?;
    Ok(passed.map(|_| {
        format!(
            "Route data from before {} will be deleted within the hour. This can’t be undone.",
            start.format("%b %-d, %Y")
        )
    }))
}

/// Moves the window a DSP chose before Settings kept it, its route data's `route_retention`
/// row, into `RETENTION` as the DSP's first value, at startup. Once the section holds one,
/// saved or moved, nothing moves again; the table stays as it was, read only here.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    // A DSP whose route data isn't made yet chose no window.
    if store.dsp(id)?.setting(STORAGE.marker, Value::Null)? != json!(1) {
        return Ok(());
    }
    let db = store.routes_db(id)?;
    // Route data from before the table existed chose none either.
    let kept = db.one(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='route_retention'",
        [],
    )?;
    let days = match kept {
        Some(_) => db
            .one("SELECT days FROM route_retention WHERE id=1", [])?
            .and_then(|row| row["days"].as_i64()),
        None => None,
    };
    if let Some(days) = days {
        dispatch_core::settings::adopt(store, id, &RETENTION, &json!({ "days": days }))?;
    }
    Ok(())
}
