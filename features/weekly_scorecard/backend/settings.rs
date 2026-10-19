//! Weekly Scorecard's settings, on its own page: which weeks a scheduled run rechecks, and its
//! schedules; and the policy a DSP chose before Settings kept it, moved in once.
use super::{STORAGE, WeeklyScorecardStore};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, schedules, section},
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Which completed weeks a scheduled run rechecks, apart from when its schedules run: the
/// latest `lookback_weeks`, posted ones too, as disputes change them, each unless checked
/// within `refresh_hours`.
pub const COLLECTION: Section = section(
    "weekly_scorecard.collection",
    "Collection",
    "weekly_scorecard.manage",
    &[
        number("lookback_weeks", "Look back", 1, 8, Some(1)).unit("weeks"),
        number("refresh_hours", "Refresh after", 1, 168, Some(20)).unit("hours"),
        schedules(
            "schedules",
            "Schedules",
            "/api/dsp/weekly-scorecard/schedules",
            "weekly_scorecard",
            ("Weekly scorecard", "07:00"),
        ),
    ],
);

/// `COLLECTION`'s values, as a scheduled run reads them.
#[derive(Deserialize)]
pub struct Policy {
    pub lookback_weeks: u32,
    pub refresh_hours: u32,
}

/// The policy as the release before kept it, in its own database's settings.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    lookback_weeks: u32,
    refresh_hours: u32,
}

/// Moves the policy a DSP chose before Settings kept it, its database's `collection_policy`,
/// into `COLLECTION` as the DSP's first values, at startup, after the retired database's
/// import. Once the section holds some, saved or moved, nothing moves again; the old value
/// stays as it was, read only here. One that doesn't parse or fit isn't moved, so the defaults
/// hold, as for a value the section can't hold.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    // A DSP whose database isn't made yet chose no policy.
    if store.dsp(id)?.setting(STORAGE.marker, Value::Null)? != json!(1) {
        return Ok(());
    }
    let kept = store
        .weekly_scorecard_db(id)?
        .setting("collection_policy", Value::Null)?;
    let Ok(kept) = serde_json::from_value::<Kept>(kept) else {
        return Ok(());
    };
    let values =
        json!({"lookback_weeks": kept.lookback_weeks, "refresh_hours": kept.refresh_hours});
    match dispatch_core::settings::adopt(store, id, &COLLECTION, &values) {
        Err(error) if error.code == "invalid_input" => Ok(()),
        moved => moved.map(drop),
    }
}
