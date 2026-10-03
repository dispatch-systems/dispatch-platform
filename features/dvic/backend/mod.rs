//! Short pre-trip inspections from Cortex's rolling supplementary workbooks.
//! Publication weeks are ISO Monday–Sunday; inspection dates come from the rows.
#[path = "cli.rs"]
pub mod cli;
#[path = "hidden.rs"]
pub mod hidden;
#[path = "keeper.rs"]
pub mod keeper;
#[path = "people.rs"]
pub mod people;
#[path = "storage.rs"]
mod storage;

pub use storage::DvicStore;

use crate::{
    Error, Result,
    collectors::{
        AddedStorage,
        cortex::dvic::{
            Capture, Collection, JOB_KIND, KnownReport, MAX_WEEKS, Request, hash, report_week,
        },
    },
    db::{Db, Kind, migrations::add_column},
    ensure,
    read_cache::DataDomain,
    validate, weeks,
};
use serde_json::{Value, json};

/// The inspections it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("dvic");
/// The DVIC database beside `cortex.sqlite`.
pub const DATABASE: Kind = Kind::new("dvic", 1);
pub static STORAGE: AddedStorage = AddedStorage {
    id: "dvic",
    kind: DATABASE,
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

/// Migration 3: whether what each report, inspection, week and run holds had its station
/// scope verified.
pub(crate) fn add_verified_scope(db: &Db) -> Result<()> {
    for table in [
        "dvic_reports",
        "dvic_inspections",
        "dvic_weeks",
        "dvic_runs",
    ] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
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
