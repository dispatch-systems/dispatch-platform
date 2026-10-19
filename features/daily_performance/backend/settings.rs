//! Daily Performance's settings, on its own page: which dates a scheduled run rechecks, and its
//! schedules; and the policy a DSP chose before Settings kept it, moved in once.
use super::storage::{DailyPerformanceStore, STORAGE};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, schedules, section},
    settings,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Which dates a scheduled run rechecks, apart from when its schedules run: the
/// `lookback_days` before today, each unless collected within `refresh_hours`.
pub const COLLECTION: Section = section(
    "daily_performance.collection",
    "Collection",
    "daily_performance.manage",
    &[
        number("lookback_days", "Look back", 1, 30, Some(7)).unit("days"),
        number("refresh_hours", "Refresh after", 1, 168, Some(20)).unit("hours"),
        schedules(
            "schedules",
            "Schedules",
            "/api/dsp/daily-performance/schedules",
            "daily_performance",
            ("Daily performance", "07:00"),
        ),
    ],
);

/// `COLLECTION`'s values, as a scheduled run reads them.
#[derive(Deserialize)]
pub struct Policy {
    pub lookback_days: u32,
    pub refresh_hours: u32,
}

/// The policy as the release before kept it, in its own database's settings.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    lookback_days: u32,
    refresh_hours: u32,
}

/// Moves the policy a DSP chose before Settings kept it, its database's `collection_policy`,
/// into `COLLECTION` as the DSP's first values, at startup (`settings::carry_over`). The old
/// value is read as the DSP first opens since, whatever it is, and not again; it stays as it
/// was, read only here. One that doesn't parse or fit isn't moved, so the defaults hold, as
/// for a value the section can't hold.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    settings::carry_over(store, id, &COLLECTION, || {
        // A DSP whose database isn't made yet chose no policy.
        if store.dsp(id)?.setting(STORAGE.marker, Value::Null)? != json!(1) {
            return Ok(None);
        }
        let kept = store
            .daily_performance_db(id)?
            .setting("collection_policy", Value::Null)?;
        Ok(serde_json::from_value::<Kept>(kept).ok().map(|kept| {
            json!({"lookback_days": kept.lookback_days, "refresh_hours": kept.refresh_hours})
        }))
    });
    Ok(())
}
