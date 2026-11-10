//! Compact directory data beside the immutable payroll history. A publisher refreshes it in
//! the same transaction. Writes by an older release, after a rollback, mark it dirty; readers
//! then use the payroll history until a publisher or upkeep rebuilds it.
use super::people;
use dispatch_core::manifest::people::Named;
use dispatch_core::{
    Result, State,
    db::{Db, Store, n, s},
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use dispatch_paycom as paycom;
use rusqlite::params;
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(crate) fn install(db: &Db) -> Result<()> {
    db.0.execute_batch(include_str!(
        "../../migrations/paycom/0001_people_summaries.sql"
    ))?;
    rebuild(db)
}
pub(super) fn valid(db: &Db) -> Result<bool> {
    Ok(db.one_as::<(i64,)>("SELECT dirty FROM paycom_people_state WHERE id=1", [])? == Some((0,)))
}
/// Called before the publisher changes history, while holding its write transaction.
pub(super) fn prepare(db: &Db) -> Result<()> {
    if !valid(db)? {
        rebuild(db)?;
    }
    Ok(())
}
/// Unions only this publication's workdays, including a fingerprint-only collection-time update.
pub(super) fn published(db: &Db, publication: &str) -> Result<()> {
    db.exec(
        "INSERT OR IGNORE INTO paycom_people_workdays \
         SELECT employee_code,date FROM timecards WHERE publication_id=? AND hours>0",
        [publication],
    )?;
    db.exec(
        "INSERT OR IGNORE INTO paycom_people_collected_days \
         SELECT t.date FROM timecards t JOIN publications p ON p.id=t.publication_id \
         WHERE p.id=? AND t.date<substr(p.collected_at,1,10)",
        [publication],
    )?;
    refresh(db)
}
/// The roster's exact historical naming rules, with worked dates read from the compact union.
fn refresh(db: &Db) -> Result<()> {
    let mut found = people::roster(db)?;
    for row in db.all(
        "SELECT employee_code code,min(date) first,max(date) last \
         FROM paycom_people_workdays GROUP BY employee_code",
        [],
    )? {
        if let Some(entry) = found.get_mut(s(&row, "code")) {
            entry.first_seen = Some(s(&row, "first").to_owned());
            entry.last_seen = Some(s(&row, "last").to_owned());
        }
    }
    db.exec("DELETE FROM paycom_people", [])?;
    for entry in found.into_values() {
        db.exec(
            "INSERT INTO paycom_people \
             (code,name,department,position,first_seen,last_seen,listed) VALUES (?,?,?,?,?,?,?)",
            params![
                entry.id,
                entry.name,
                entry.department,
                entry.position,
                entry.first_seen,
                entry.last_seen,
                entry.listed
            ],
        )?;
    }
    db.exec("UPDATE paycom_people_state SET dirty=0 WHERE id=1", [])?;
    Ok(())
}
fn rebuild(db: &Db) -> Result<()> {
    db.exec("DELETE FROM paycom_people_workdays", [])?;
    db.exec(
        "INSERT INTO paycom_people_workdays \
         SELECT DISTINCT employee_code,date FROM timecards WHERE hours>0",
        [],
    )?;
    db.exec("DELETE FROM paycom_people_collected_days", [])?;
    db.exec(
        "INSERT INTO paycom_people_collected_days \
         SELECT DISTINCT t.date FROM timecards t JOIN publications p ON p.id=t.publication_id \
         WHERE t.date<substr(p.collected_at,1,10)",
        [],
    )?;
    refresh(db)
}
pub(super) fn employees(db: &Db) -> Result<Vec<Named>> {
    Ok(db
        .all("SELECT * FROM paycom_people ORDER BY code", [])?
        .into_iter()
        .map(|row| {
            let optional = |key: &str| row[key].as_str().map(str::to_owned);
            Named {
                name: s(&row, "name").to_owned(),
                department: optional("department"),
                position: optional("position"),
                first_seen: optional("first_seen"),
                last_seen: optional("last_seen"),
                listed: n(&row, "listed") == 1,
                ..Named::new(s(&row, "code"))
            }
        })
        .collect())
}
/// Rebuilds what an older release's writes marked dirty, without adding work to a directory
/// read.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: maintain,
};
fn maintain(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let result = async {
            let dsps = state.read(|db| db.served_ids()).await?;
            for dsp in dsps {
                let at = dsp.clone();
                let dirty = state
                    .read(move |db| {
                        if !db.feature_enabled(&at, "timecard")? {
                            return Ok(false);
                        }
                        let paycom = db.collector(&at, paycom::PROVIDER)?;
                        Ok(!valid(&paycom)?)
                    })
                    .await?;
                if dirty {
                    let at = dsp.clone();
                    state
                        .run_serving(dsp, super::DOMAIN, move |db: &Store| {
                            let db = db.collector(&at, paycom::PROVIDER)?;
                            db.transaction(|| prepare(&db))
                        })
                        .await?;
                }
            }
            Ok::<_, dispatch_core::Error>(())
        }
        .await;
        if let Err(error) = result {
            observability::event(
                "error",
                "timecard.people_summary_failed",
                json!({"error":error.code}),
            );
        }
    })
}
