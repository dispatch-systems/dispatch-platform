//! Short pre-trip inspections from Cortex's rolling supplementary workbooks.
//! Publication weeks are ISO Monday–Sunday; inspection dates come from the rows.
#[path = "hidden.rs"]
pub mod hidden;
#[path = "keeper.rs"]
pub mod keeper;
#[path = "storage.rs"]
mod storage;

use crate::{
    Error, Result,
    collectors::{
        AddedStorage,
        cortex::dvic::{Capture, Collection, JOB_KIND, MAX_WEEKS, Request, hash, report_week},
    },
    db::{Db, Kind},
    ensure, validate, weeks,
};
use serde_json::{Value, json};

pub static STORAGE: AddedStorage = AddedStorage {
    id: "dvic",
    kind: Kind::Dvic,
    marker: "storage.dvic",
    source: "dvic-v1",
    verify,
};
fn verify(db: &Db) -> Result<()> {
    for table in [
        "dvic_reports",
        "dvic_revisions",
        "dvic_inspections",
        "dvic_weeks",
        "dvic_runs",
        "dvic_hidden_drivers",
    ] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

pub fn weeks_ending(week: &str, count: usize) -> Result<Vec<String>> {
    ensure(
        (1..=MAX_WEEKS).contains(&count),
        "invalid_dvic_request",
        400,
    )?;
    weeks::weeks_before(week, count - 1)
}
