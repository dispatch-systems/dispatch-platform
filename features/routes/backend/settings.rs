//! Routes' settings: its schedules, on its own page; how long the DSP keeps its route data, on
//! Settings' Data page; and the window a DSP chose before Settings kept it, moved in once.
use super::{
    COLLECTION, MAX_RETENTION_DAYS, MIN_RETENTION_DAYS, RoutesStore, STORAGE, TODAY, first_kept,
    maintenance::MAINTENANCE,
};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, schedules, section},
    settings,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// The schedules of its `routes` collection, which core's schedule routes keep: each reads
/// yesterday, as `routes`, or today so far, as `routes_today`.
pub const SCHEDULES: Section = section(
    "routes.collection",
    "Collection",
    "routes.manage",
    &[schedules(
        "schedules",
        "Schedules",
        "/api/dsp/routes/schedules",
        COLLECTION,
        ("Daily routes", "04:00"),
    )
    .collects("Day", &[(COLLECTION, "Yesterday"), (TODAY, "Today so far")])],
);

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

/// The longest after a save until expiry runs, in milliseconds: its hour, and the minute
/// upkeep may wait to be offered.
const EXPIRES_WITHIN: i64 = MAINTENANCE.every.as_millis() as i64 + 60 * 1000;

/// The words to confirm a window with when it passes a day the DSP's route data holds. It
/// reads only `dsp`'s route data.
fn deletes(db: &Store, dsp: &str, _: &Value, after: &Value) -> Result<Option<String>> {
    deleted(db, dsp, after, dispatch_core::db::now())
}
/// The words to confirm the window `after` with, saved at `at` (milliseconds since the epoch),
/// when it passes a day the route data holds: the first day it keeps as the expiry that runs
/// within the hour after `at` counts it, a day later when that hour passes midnight where the
/// DSP is, so the expiry deletes no day the words don't name. Every day kept, or none stored
/// before it, asks nothing.
fn deleted(db: &Store, dsp: &str, after: &Value, at: i64) -> Result<Option<String>> {
    let Some(days) = after["days"].as_i64() else {
        return Ok(None);
    };
    let start = first_kept(db, dsp, days, at + EXPIRES_WITHIN)?;
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
/// row, into `RETENTION` as the DSP's first value, at startup (`settings::carry_over`). The
/// table is read as the DSP first opens since, whatever it holds, and not again; it stays as it
/// was, read only here.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    settings::carry_over(store, id, &RETENTION, || {
        // A DSP whose route data isn't made yet chose no window.
        if store.dsp(id)?.setting(STORAGE.marker, Value::Null)? != json!(1) {
            return Ok(None);
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
        Ok(days.map(|days| json!({ "days": days })))
    });
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
