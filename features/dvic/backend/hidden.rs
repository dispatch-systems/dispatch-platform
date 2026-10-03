//! Drivers kept out of a DSP's DVIC data. Nothing in the dashboard or the API reads or
//! changes the list: the operator manages it with `dvic-hide`, `dvic-unhide` and
//! `dvic-hidden`. Publication drops a hidden driver's rows before anything is written, and
//! hiding a driver removes what was stored before, so the DSP's DVIC database never holds
//! them. Showing a driver again lets later reports in; it restores nothing.
use crate::{Result, collectors::cortex::dvic::Inspection, db::Db, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The hidden drivers' transporter IDs.
pub fn hidden(db: &Db) -> Result<HashSet<String>> {
    Ok(db
        .all("SELECT transporter_id FROM dvic_hidden_drivers", [])?
        .iter()
        .filter_map(|row| row["transporter_id"].as_str().map(str::to_owned))
        .collect())
}

/// The rows of `rows` whose drivers are not hidden.
pub fn kept<'a>(rows: &'a [Inspection], hidden: &HashSet<String>) -> Vec<&'a Inspection> {
    rows.iter()
        .filter(|row| !hidden.contains(&row.transporter_id))
        .collect()
}

/// Amazon's transporter IDs are short uppercase letters and digits.
fn driver_id(driver: &str) -> Result<()> {
    ensure(
        (1..=64).contains(&driver.len())
            && driver
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "invalid_driver_id",
        400,
    )
}

/// Records `driver` as hidden, then deletes their inspections and removes their rows from
/// every stored report copy, recounting the reports those copies are current for.
pub fn hide(db: &Db, driver: &str, note: &str, at: &str) -> Result<Value> {
    driver_id(driver)?;
    let note = note.trim();
    ensure(
        (1..=200).contains(&note.chars().count()),
        "invalid_note",
        400,
    )?;
    db.exec("BEGIN IMMEDIATE", [])?;
    let result = (|| {
        db.exec(
            "INSERT INTO dvic_hidden_drivers(transporter_id,note,hidden_at) VALUES (?,?,?) \
             ON CONFLICT(transporter_id) DO UPDATE SET note=excluded.note",
            [driver, note, at],
        )?;
        let inspections = db.exec(
            "DELETE FROM dvic_inspections WHERE transporter_id=?",
            [driver],
        )?;
        let only = HashSet::from([driver.to_owned()]);
        let mut copies = 0;
        for revision in db.all("SELECT id,rows FROM dvic_revisions", [])? {
            let id = revision["id"].as_str().unwrap_or_default();
            let all: Vec<Inspection> =
                serde_json::from_str(revision["rows"].as_str().unwrap_or("[]"))?;
            let rows = kept(&all, &only);
            if rows.len() == all.len() {
                continue;
            }
            copies += 1;
            db.exec(
                "UPDATE dvic_revisions SET rows=? WHERE id=?",
                [serde_json::to_string(&rows)?, id.to_owned()],
            )?;
            let shorts = rows
                .iter()
                .map(|row| row.is_short())
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .filter(|short| *short)
                .count();
            db.exec(
                "UPDATE dvic_reports SET row_count=?,short_count=?,min_date=?,max_date=? WHERE revision_id=?",
                rusqlite::params![
                    rows.len() as i64,
                    shorts as i64,
                    rows.iter().map(|row| row.start_date.as_str()).min(),
                    rows.iter().map(|row| row.start_date.as_str()).max(),
                    id,
                ],
            )?;
        }
        Ok(json!({"driver":driver,"inspectionsDeleted":inspections,"reportCopiesCleaned":copies}))
    })();
    db.exec(if result.is_ok() { "COMMIT" } else { "ROLLBACK" }, [])?;
    result
}

/// Takes `driver` off the list. Their later reports are stored again; nothing returns.
pub fn unhide(db: &Db, driver: &str) -> Result<Value> {
    driver_id(driver)?;
    let removed = db.exec(
        "DELETE FROM dvic_hidden_drivers WHERE transporter_id=?",
        [driver],
    )?;
    ensure(removed == 1, "driver_not_hidden", 404)?;
    Ok(json!({"driver":driver,"hidden":false}))
}

/// Every hidden driver with why and since when.
pub fn list(db: &Db) -> Result<Value> {
    Ok(json!(db.all(
        "SELECT transporter_id AS driver,note,hidden_at AS hiddenAt FROM dvic_hidden_drivers \
         ORDER BY hidden_at,transporter_id",
        [],
    )?))
}
