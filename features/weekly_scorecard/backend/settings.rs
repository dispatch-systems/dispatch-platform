//! Weekly Scorecard's settings, with its collection on the Collectors page: which weeks a run
//! rechecks; and the policy a DSP chose before Settings kept it, moved in once.
use super::{STORAGE, WeeklyScorecardStore};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Section, number, section},
    settings,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Which completed weeks a run of its collection rechecks, apart from when its rules run it:
/// the latest `lookback_weeks`, posted ones too, as disputes change them, each unless checked
/// within `refresh_hours`.
pub const COLLECTION: Section = section(
    "weekly_scorecard.collection",
    "Collection",
    "weekly_scorecard.manage",
    &[
        number("lookback_weeks", "Look back", 1, 8, Some(1)).unit("weeks"),
        number("refresh_hours", "Refresh after", 1, 168, Some(20)).unit("hours"),
    ],
)
.collection("weekly_scorecard");

/// `COLLECTION`'s values, as a run reads them.
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
/// import (`settings::carry_over`). The old value is read as the DSP first opens since,
/// whatever it is, and not again; it stays as it was, read only here. One that doesn't parse
/// isn't moved, so the defaults hold, and a value its field doesn't hold now reads as the
/// default, the other moving still.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    settings::carry_over(store, id, &COLLECTION, || {
        // A DSP whose database isn't made yet chose no policy.
        if store.dsp(id)?.setting(STORAGE.marker, Value::Null)? != json!(1) {
            return Ok(None);
        }
        let kept = store
            .weekly_scorecard_db(id)?
            .setting("collection_policy", Value::Null)?;
        Ok(serde_json::from_value::<Kept>(kept).ok().map(|kept| {
            json!({"lookback_weeks": kept.lookback_weeks, "refresh_hours": kept.refresh_hours})
        }))
    });
    Ok(())
}
