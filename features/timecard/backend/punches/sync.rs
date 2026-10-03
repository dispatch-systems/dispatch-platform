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

impl Store {
    pub(crate) fn publish_employee_timecard(
        &self,
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
        self.collector(id, paycom::PROVIDER)?.exec(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{collectors::paycom::fixtures, config::Config, operations};
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_scoped_publication_rejects_other_employees_periods_and_incomplete_captures() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
        let mut config = Config::load()?;
        config.root = root.path().into();
        config.development = true;
        config.fixture = true;
        config.environment = "preview".into();
        let store = Store::initialize(config)?;
        let bootstrap = operations::bootstrap(
            &store,
            "sync@example.test",
            "Sync",
            "Owner",
            "test-password-long",
        )?;
        let id = s(&bootstrap["dsp"], "id");
        let full = fixtures::fixture_date("UTC", Some("2026-08-22".parse().unwrap()))?;
        store.publish(id, &full)?;
        let scope = EmployeeSync {
            employee_code: "E001".into(),
            from: "2026-08-09".into(),
            to: "2026-08-22".into(),
        };
        let capture = scope.fixture("UTC")?;
        store.publish_employee_timecard(id, &scope, &capture)?;
        let saved = store.employee_timecard(id, "E001", Some(&scope.period()))?;
        assert_eq!(saved.timecards.len(), 14);
        let mut wrong = capture.clone();
        wrong["employees"] = full["employees"].clone();
        assert!(store.publish_employee_timecard(id, &scope, &wrong).is_err());
        let mut wrong = capture.clone();
        wrong["from"] = json!("2026-08-08");
        assert!(store.publish_employee_timecard(id, &scope, &wrong).is_err());
        let mut wrong = capture.clone();
        wrong["timecards"].as_array_mut().unwrap().pop();
        assert!(store.publish_employee_timecard(id, &scope, &wrong).is_err());
        let other = EmployeeSync {
            employee_code: "E002".into(),
            ..scope.clone()
        };
        assert!(
            store
                .publish_employee_timecard(id, &other, &capture)
                .is_err()
        );
        assert_eq!(
            store.employee_timecard(id, "E001", Some(&scope.period()))?,
            saved
        );
        assert_eq!(
            store
                .employees(id, "", 0, None, false, None)
                .map(|value| serde_json::to_value(value).unwrap())?["total"],
            12
        );
        assert!(
            EmployeeSync::parse(
                &json!({"employeeCode":"E001","from":scope.from,"to":scope.to,"date":scope.from})
            )
            .is_err()
        );
        Ok(())
    }
}
