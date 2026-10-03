//! Single-employee Paycom requests and captures, separate from full roster snapshots.
use crate::{
    Result,
    collectors::paycom::{
        self,
        timecards::{EmployeeSync, PERIOD_DAYS},
        validation::validate_workforce,
    },
    db::{Store, s},
    ensure,
};
use chrono::Duration;
use rusqlite::params;
use serde_json::Value;

/// Apply the source link once, for both the employee and daily views.
pub(crate) fn synced_cards(data: &Value) -> Vec<Value> {
    data["timecards"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|card| {
            let mut card = card.clone();
            card["sourceUrl"] = data["sources"][0]["url"].clone();
            card
        })
        .collect()
}

pub(crate) fn publish_employee_timecard(
    store: &Store,
    id: &str,
    scope: &EmployeeSync,
    data: &Value,
) -> Result<()> {
    validate_workforce(data)?;
    let start = scope.period().start()?;
    let records = data["timecards"].as_array().unwrap();
    ensure(
        data["from"] == scope.from
            && data["to"] == scope.to
            && data["employees"].as_array().unwrap().len() == 1
            && data["employees"][0]["code"] == scope.employee_code
            && records.len() == PERIOD_DAYS as usize
            && (0..PERIOD_DAYS).all(|i| {
                records.iter().any(|card| {
                    card["employeeCode"] == scope.employee_code
                        && card["date"] == (start + Duration::days(i)).to_string()
                })
            }),
        "timecard_identity_mismatch",
        502,
    )?;
    // Kept outside full publications: a single-employee sync never becomes the roster.
    // Older releases safely ignore this additive table on rollback.
    store.collector(id, paycom::PROVIDER)?.exec(
        "INSERT INTO employee_timecard_syncs VALUES (?,?,?,?,?) \
         ON CONFLICT(employee_code,period_from,period_to) DO UPDATE SET \
         collected_at=excluded.collected_at,data=excluded.data \
         WHERE excluded.collected_at>=employee_timecard_syncs.collected_at",
        params![
            scope.employee_code,
            scope.from,
            scope.to,
            s(data, "collectedAt"),
            data.to_string()
        ],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/punches/sync.rs"]
mod tests;
