//! Requested rules, collection-wide edits and timezone/feature retiming.
use super::{
    Scheduled, anchor, next_run,
    storage::{self, ScheduleRow, legacy, revision},
    timezone,
};
use crate::{
    Error, Result,
    collection::{
        api::types::{ScheduleCollection, ScheduleRuns},
        registry::Provider,
        rules::{self, Rule},
    },
    db::{AuditChange, Store, at, now},
    ensure,
    foundation::validate as v,
};
use rusqlite::params;
use serde_json::Value;

const SWITCH: &str = "UPDATE collection_schedules SET enabled=?,next_run=?,last_error=NULL,\
    revision=revision+1 WHERE id=?";
const PAUSE: &str = "UPDATE collection_schedules SET enabled=0,next_run=NULL,last_error=NULL,\
    revision=revision+1 WHERE enabled=1 AND collection=?";
const RETIME: &str = "UPDATE collection_schedules SET anchor=?,next_run=?,last_error=NULL,\
    revision=revision+1 WHERE id=?";
/// How many runs a preview lists.
const UPCOMING: usize = 5;
/// The most rules a DSP's collections have between them.
const MOST_RULES: i64 = 50;

/// The rules of a request: what each collects by, and when.
fn requested(
    scheduled: Scheduled,
    value: &Value,
) -> Result<Vec<(Option<String>, ScheduleCollection, Rule)>> {
    let rules = value["rules"]
        .as_array()
        .filter(|rules| rules.len() as i64 <= MOST_RULES)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    rules
        .iter()
        .map(|rule| {
            v::fields(
                rule,
                &[
                    "id", "collects", "kind", "days", "times", "every", "from", "to",
                ],
            )?;
            let id = match &rule["id"] {
                Value::Null => None,
                _ => Some(v::text(rule, "id", 1, 128)?.to_owned()),
            };
            let collects = scheduled.offers(v::text(rule, "collects", 1, 64)?)?;
            Ok((id, collects, Rule::parse(rule)?))
        })
        .collect()
}
/// A collection's rules, each as words, for the activity log.
fn described(rows: &[(ScheduleCollection, Rule)], scheduled: Scheduled) -> Option<String> {
    let labels = scheduled.options();
    let chooses = labels.len() > 1;
    let words: Vec<String> = rows
        .iter()
        .map(|(collection, rule)| {
            let label = labels
                .iter()
                .find(|(c, _)| c == collection)
                .map(|(_, l)| *l);
            match label.filter(|_| chooses) {
                Some(label) => format!("{} ({label})", rule.describe()),
                None => rule.describe(),
            }
        })
        .collect();
    (!words.is_empty()).then(|| words.join("; "))
}

impl Store {
    /// Replaces a collection's rules with those of `value`, as at its `revision`: a rule
    /// with an id keeps its row, one without is new, and one left out is removed. It stays
    /// on or off as it was; one with no rules yet starts on, unless what it reads isn't ready,
    /// as its collector not connected. Answers what the activity log keeps of the change: its
    /// rules as they were and are, as words.
    pub fn save_rules(
        &self,
        id: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<Option<AuditChange>> {
        v::fields(value, &["rules", "revision"])?;
        let requested = requested(scheduled, value)?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let before = self.rows_of(id, scheduled)?;
            ensure(
                revision(&before) == v::text(value, "revision", 0, 8192)?,
                "schedule_changed",
                409,
            )?;
            let others =
                db.count("SELECT count(*) FROM collection_schedules", [])? - before.len() as i64;
            ensure(
                others + requested.len() as i64 <= MOST_RULES,
                "schedule_limit",
                409,
            )?;
            let mut collects: Vec<ScheduleCollection> = Vec::new();
            for (_, collection, _) in &requested {
                if !collects.contains(collection) {
                    collects.push(*collection);
                }
            }
            let ready = || -> Result<()> {
                for collection in &collects {
                    self.check_schedule_sources(id, *collection)?;
                }
                Ok(())
            };
            // A collection's first rules start it on once what it reads is ready, and off till
            // then; one on stays on, and needs it ready.
            let enabled = if before.is_empty() {
                ready().is_ok()
            } else {
                before.iter().any(|row| row.enabled)
            };
            if enabled {
                ready()?;
            }
            let mut kept = Vec::new();
            for (position, (key, collects, rule)) in requested.iter().enumerate() {
                let row = match key {
                    Some(key) => Some(
                        before
                            .iter()
                            .find(|row| &row.id == key && !kept.contains(&row.id))
                            .ok_or_else(|| Error::new("schedule_changed", 409))?,
                    ),
                    None => None,
                };
                let key = storage::save_rule(&db, row, collects, rule, position, enabled, tz)?;
                kept.push(key);
            }
            for row in &before {
                if !kept.contains(&row.id) {
                    db.exec("DELETE FROM collection_schedules WHERE id=?", [&row.id])?;
                }
            }
            let words = |rows: Vec<(ScheduleCollection, Rule)>| described(&rows, scheduled);
            let was = words(
                before
                    .iter()
                    .map(|r| (r.collection, r.rule.clone()))
                    .collect(),
            );
            let is = words(requested.iter().map(|(_, c, r)| (*c, r.clone())).collect());
            Ok((was != is).then_some(("rules", was, is)))
        })
    }
    /// Turns a collection's schedule on or off, as at its `revision`: every rule at once.
    /// Answers whether that changed it.
    pub fn switch_rules(&self, id: &str, scheduled: Scheduled, value: &Value) -> Result<bool> {
        v::fields(value, &["enabled", "revision"])?;
        let enabled = v::boolean(value, "enabled")?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let rows = self.rows_of(id, scheduled)?;
            ensure(
                revision(&rows) == v::text(value, "revision", 0, 8192)?,
                "schedule_changed",
                409,
            )?;
            ensure(!rows.is_empty(), "schedule_not_found", 404)?;
            let was = rows.iter().any(|row| row.enabled);
            if was == enabled && rows.iter().all(|row| row.enabled == enabled) {
                return Ok(false);
            }
            for row in &rows {
                if enabled {
                    self.check_schedule_sources(id, row.collection)?;
                }
                let next = if enabled {
                    Some(next_run(&row.rule, tz, now())?)
                } else {
                    None
                };
                db.exec(SWITCH, params![enabled, next, row.id])?;
            }
            Ok(was != enabled)
        })
    }
    /// The next runs of the rules of `value`, unsaved, as though the collection were on.
    pub fn preview_rules(
        &self,
        id: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<ScheduleRuns> {
        v::fields(value, &["rules"])?;
        let requested = requested(scheduled, value)?;
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        let rules: Vec<Rule> = requested.into_iter().map(|(_, _, rule)| rule).collect();
        Ok(ScheduleRuns {
            runs: rules::upcoming(&rules, tz, now(), UPCOMING)
                .into_iter()
                .map(at)
                .collect(),
        })
    }

    /// Pauses the schedules that need `provider`: those of its collections, and of each
    /// alias that runs one of them.
    pub fn pause_provider_schedules(&self, id: &str, provider: Provider) -> Result<()> {
        for collection in ScheduleCollection::all() {
            let needs = Self::scheduled_collections(collection).any(|(other, _)| other == provider);
            if needs {
                self.dsp(id)?.exec(PAUSE, [collection.as_str()])?;
            }
        }
        Ok(())
    }
    pub fn retime_schedules(&self, id: &str, tz: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, None)
    }
    pub(crate) fn retime_feature_schedules(&self, id: &str, tz: &str, feature: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, Some(feature))
    }
    /// Runs the DSP's schedules, or a feature's, in `tz` from now on, and wakes the scheduler
    /// once the work is done, whichever route or job it's part of.
    fn retime_schedules_for(&self, id: &str, tz: &str, feature: Option<&str>) -> Result<()> {
        let tz = timezone(tz)?;
        let db = self.dsp(id)?;
        db.transaction(|| {
            let rows: Vec<ScheduleRow> = db.query_as(
                "SELECT * FROM collection_schedules \
                WHERE collection IN (SELECT value FROM json_each(?))",
                [ScheduleCollection::known()?],
            )?;
            for row in rows {
                if feature.is_some_and(|f| {
                    crate::tenancy::catalog::automation(row.collection.as_str()) != Some(f)
                }) {
                    continue;
                }
                let anchor = anchor(legacy(&row.rule).2, tz, now())?;
                let deadline = if row.enabled {
                    Some(next_run(&row.rule, tz, now())?)
                } else {
                    None
                };
                db.exec(RETIME, params![anchor, deadline, row.id])?;
            }
            Ok(())
        })?;
        self.schedules_moved();
        Ok(())
    }
}
