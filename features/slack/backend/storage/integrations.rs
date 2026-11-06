//! Per-workspace integration switches and when each integration came on.
use super::workspaces::kept_workspace;
use crate::backend::integrations;
use dispatch_core::{
    Result,
    db::{Db, Store, iso},
};
use rusqlite::params;

/// Whether the integration `id` starts on in every workspace, as its feature declares it.
fn starts_on(id: &str) -> bool {
    integrations::declared().any(|(_, integration)| integration.id == id && integration.starts_on)
}
/// Whether the integration `id` is on in the workspace `team`: switched on there, or starting
/// on and not switched off there.
fn switched_on(db: &Db, team: &str, id: &str) -> Result<bool> {
    let row = |table: &str| {
        db.one(
            &format!("SELECT 1 FROM {table} WHERE team_id=? AND integration=?"),
            [team, id],
        )
    };
    Ok(kept_workspace(db, team)?
        && (row("slack_integrations")?.is_some()
            || (starts_on(id) && row("slack_integrations_off")?.is_none())))
}
/// An ISO 8601 time `column` holds, in milliseconds, as SQL reads it.
fn ms(column: &str) -> String {
    format!("CAST((julianday({column}) - 2440587.5) * 86400000 AS INTEGER)")
}

pub trait IntegrationStore {
    /// The IDs of the integrations on in the workspace `team`, in order: each switched on there,
    /// and each that starts on and wasn't switched off there. None for a workspace the DSP
    /// doesn't have.
    fn slack_integrations(&self, dsp: &str, team: &str) -> Result<Vec<String>>;
    /// When the integration `id` came on in the workspace `team`, in milliseconds: when someone
    /// switched it on there, or, for one on from the start, when the DSP got the workspace,
    /// however often it was reconnected since. None while it's off.
    fn slack_integration_on_since(&self, dsp: &str, team: &str, id: &str) -> Result<Option<i64>>;
    /// Switches the integration `id` on in the workspace `team`, as the member `by`: whether it
    /// was off.
    fn turn_on_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str) -> Result<bool>;
    /// Switches the integration `id` off in the workspace `team`, as the member `by`: whether it
    /// was on.
    fn turn_off_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str)
    -> Result<bool>;
}

impl IntegrationStore for Store {
    fn slack_integrations(&self, dsp: &str, team: &str) -> Result<Vec<String>> {
        let db = self.dsp(dsp)?;
        if !kept_workspace(&db, team)? {
            return Ok(Vec::new());
        }
        let ids = |table: &str| -> Result<Vec<String>> {
            let rows: Vec<(String,)> = db.query_as(
                &format!("SELECT integration FROM {table} WHERE team_id=?"),
                [team],
            )?;
            Ok(rows.into_iter().map(|(id,)| id).collect())
        };
        let (mut on, off) = (ids("slack_integrations")?, ids("slack_integrations_off")?);
        for (_, integration) in integrations::declared() {
            let id = integration.id.to_owned();
            if integration.starts_on && !off.contains(&id) && !on.contains(&id) {
                on.push(id);
            }
        }
        on.sort();
        Ok(on)
    }
    fn slack_integration_on_since(&self, dsp: &str, team: &str, id: &str) -> Result<Option<i64>> {
        let db = self.dsp(dsp)?;
        let switched: Option<(i64,)> = db.one_as(
            &format!(
                "SELECT {} FROM slack_integrations WHERE team_id=? AND integration=?",
                ms("turned_on_at")
            ),
            [team, id],
        )?;
        if switched.is_some() || !starts_on(id) {
            return Ok(switched.map(|(at,)| at));
        }
        let connected: Option<(i64,)> = db.one_as(
            &format!(
                "SELECT {} FROM slack_workspaces WHERE team_id=?1 AND NOT EXISTS \
                 (SELECT 1 FROM slack_integrations_off WHERE team_id=?1 AND integration=?2)",
                ms("added_at")
            ),
            [team, id],
        )?;
        Ok(connected.map(|(at,)| at))
    }
    fn turn_on_slack_integration(&self, dsp: &str, team: &str, id: &str, by: &str) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            if switched_on(&db, team, id)? {
                return Ok(false);
            }
            db.exec(
                "DELETE FROM slack_integrations_off WHERE team_id=? AND integration=?",
                [team, id],
            )?;
            db.exec(
                "INSERT OR IGNORE INTO slack_integrations \
                 (team_id,integration,turned_on_by,turned_on_at) VALUES (?,?,?,?)",
                params![team, id, by, iso()],
            )?;
            Ok(true)
        })
    }
    fn turn_off_slack_integration(
        &self,
        dsp: &str,
        team: &str,
        id: &str,
        by: &str,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            if !switched_on(&db, team, id)? {
                return Ok(false);
            }
            db.exec(
                "DELETE FROM slack_integrations WHERE team_id=? AND integration=?",
                [team, id],
            )?;
            // One that starts on stays off once switched off.
            if starts_on(id) {
                db.exec(
                    "INSERT OR REPLACE INTO slack_integrations_off \
                     (team_id,integration,turned_off_by,turned_off_at) VALUES (?,?,?,?)",
                    params![team, id, by, iso()],
                )?;
            }
            Ok(true)
        })
    }
}
