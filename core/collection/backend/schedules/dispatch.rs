//! Immediate collection requests, due batches and their next deadlines.
use super::{Scheduled, next_run, storage::ScheduleRow, timezone};
use crate::{
    Error, Result,
    collection::{
        api::{
            jobs::{JobKind, PublicJob},
            types::ScheduleCollection,
        },
        registry::Provider,
    },
    db::{Store, iso, now},
    ensure,
    foundation::validate as v,
    job_statuses,
    manifest::registry,
};
use rusqlite::params;
use serde_json::Value;

// Each query that reads, retimes or runs a schedule takes `ScheduleCollection::known()`,
// and each that asks for a DSP's jobs `JobKind::known()`, so that none reaches what this
// release doesn't run.
const NEXT_DEADLINE: &str = "SELECT collection,next_run FROM collection_schedules \
    WHERE enabled=1 AND collection IN (SELECT value FROM json_each(?)) ORDER BY next_run";
const ACTIVE_JOB: &str = concat!(
    "SELECT id FROM jobs WHERE dsp_id=? AND kind IN (SELECT value FROM json_each(?)) \
     AND status IN ",
    job_statuses!(active),
    " LIMIT 1"
);

impl Store {
    /// Queues now what one run of a schedule collecting by `value`'s `collects` queues, for
    /// `actor`, once for its `requestId`.
    pub fn collect_now(
        &self,
        id: &str,
        actor: &str,
        scheduled: Scheduled,
        value: &Value,
    ) -> Result<Vec<PublicJob>> {
        v::fields(value, &["collects", "requestId"])?;
        let collects = scheduled.offers(v::text(value, "collects", 1, 64)?)?;
        let request = v::text(value, "requestId", 1, 128)?;
        self.check_schedule_sources(id, collects)?;
        let key = format!("collect:{request}:");
        self.enqueue_batch(
            id,
            Some(actor),
            &self.scheduled_requests(id, collects, &key)?,
        )
    }
    /// What one run of a schedule collecting by `collection` queues, each request keyed
    /// under `key`.
    fn scheduled_requests(
        &self,
        id: &str,
        collection: ScheduleCollection,
        key: &str,
    ) -> Result<Vec<(String, Provider, Value)>> {
        let mut requests = Vec::new();
        for (provider, scheduled) in Self::scheduled_collections(collection) {
            let keeper = registry().keeper(scheduled.job_kind);
            for (suffix, request) in keeper.scheduled(self, id, collection.as_str())? {
                requests.push((format!("{key}{suffix}"), provider, request));
            }
        }
        Ok(requests)
    }

    pub fn schedule_deadlines(&self) -> Result<Vec<(String, i64)>> {
        let mut deadlines = Vec::new();
        let dsps: Vec<(String,)> = self.platform.query_as(
            "SELECT id FROM dsps WHERE status='active' AND environment=?",
            [&self.config.environment],
        )?;
        for (id,) in dsps {
            let enabled = self.features(&id)?;
            if !crate::tenancy::catalog::automates(&enabled) {
                continue;
            }
            // The soonest schedule whose page is on; the others wait for their page.
            let rows: Vec<(String, Option<String>)> = self
                .dsp(&id)?
                .query_as(NEXT_DEADLINE, [ScheduleCollection::known()?])?;
            let next = rows
                .into_iter()
                .find(|(collection, _)| {
                    crate::tenancy::catalog::automation(collection)
                        .is_some_and(|page| enabled.iter().any(|f| f == page))
                })
                .map(|(_, next_run)| next_run);
            if let Some(next_run) = next {
                let deadline = next_run
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
                    .map_or(0, |d| d.timestamp_millis());
                deadlines.push((id, deadline));
            }
        }
        Ok(deadlines)
    }
    /// What the rule `row`'s pending run queues, unless that run was queued already, as when
    /// the server stopped after queueing it and before moving the rule on.
    fn due_requests(&self, id: &str, row: &ScheduleRow) -> Result<Vec<(String, Provider, Value)>> {
        let pending = row.next_run.as_deref().unwrap_or("");
        let key = format!("schedule:{}:{}:", row.id, pending);
        if self
            .jobs
            .one(
                "SELECT id FROM jobs WHERE dsp_id=? AND substr(idempotency_key,1,?)=? LIMIT 1",
                params![id, key.len() as i64, key],
            )?
            .is_some()
        {
            return Ok(vec![]);
        }
        self.check_schedule_sources(id, row.collection)?;
        self.scheduled_requests(id, row.collection, &key)
    }
    /// Queues the runs of the DSP's rules that are due, all in one batch, so the collections
    /// due together run together, and moves each rule on to its next run. A rule that can't
    /// run yet keeps its run and says why, and is tried again in a minute. Answers when the
    /// next is due.
    pub fn schedule_due(&self, id: &str) -> Result<Option<i64>> {
        let dsp = self.find_dsp(id)?;
        let enabled = self.features(id)?;
        if !self.serves(&dsp) || !crate::tenancy::catalog::automates(&enabled) {
            return Ok(None);
        }
        let tz = timezone(&dsp.timezone)?;
        let db = self.dsp(id)?;
        let rows: Vec<ScheduleRow> = db.query_as(
            "SELECT * FROM collection_schedules WHERE enabled=1 \
            AND collection IN (SELECT value FROM json_each(?)) ORDER BY next_run,id",
            [ScheduleCollection::known()?],
        )?;
        // A schedule whose page is off waits, as one no feature keeps does.
        let rows: Vec<ScheduleRow> = rows
            .into_iter()
            .filter(|row| {
                let page = crate::tenancy::catalog::automation(row.collection.as_str());
                page.is_some_and(|page| enabled.iter().any(|f| f == page))
            })
            .collect();
        let now_iso = iso();
        let due = rows
            .iter()
            .filter(|row| row.next_run.as_ref().is_some_and(|value| value <= &now_iso));
        // The first of a collection's rules due together queues its run, for them all. The
        // others move on before it's queued: stopped once it is, only the first is still due,
        // and that run is queued already.
        let mut queueing: Vec<(&str, ScheduleCollection)> = Vec::new();
        let mut moved: Vec<&str> = Vec::new();
        let mut waiting: Vec<(&str, String)> = Vec::new();
        let mut requests = Vec::new();
        for row in due {
            if queueing
                .iter()
                .any(|(_, collection)| *collection == row.collection)
            {
                db.exec(
                    "UPDATE collection_schedules SET next_run=?,last_error=NULL WHERE id=?",
                    [&next_run(&row.rule, tz, now())?, &row.id],
                )?;
                moved.push(&row.id);
                continue;
            }
            match self.due_requests(id, row) {
                Ok(due) if due.is_empty() => moved.push(&row.id),
                Ok(due) => {
                    requests.extend(due);
                    queueing.push((&row.id, row.collection));
                }
                Err(error) => waiting.push((&row.id, error.code)),
            }
        }
        if !requests.is_empty() {
            // Other collections finish before another recurring batch enters the queue.
            let started = ensure(
                self.jobs
                    .one(ACTIVE_JOB, [id, JobKind::known()?.as_str()])?
                    .is_none(),
                "sync_in_progress",
                409,
            )
            .and_then(|()| self.enqueue_batch(id, None, &requests));
            let queued = queueing.into_iter().map(|(row, _)| row);
            match started {
                Ok(_) => moved.extend(queued),
                // None of the batch is queued: each rule that queued it waits.
                Err(error) => waiting.extend(queued.map(|row| (row, error.code.clone()))),
            }
        }
        let mut earliest = None;
        let mut soonest = |ms: i64| earliest = Some(earliest.map_or(ms, |v: i64| v.min(ms)));
        for row in &rows {
            if let Some((_, code)) = waiting.iter().find(|(key, _)| *key == row.id) {
                db.exec(
                    "UPDATE collection_schedules SET last_error=? WHERE id=?",
                    [code, &row.id],
                )?;
                soonest(now() + 60000);
                continue;
            }
            let deadline = match &row.next_run {
                Some(pending) if !moved.contains(&row.id.as_str()) => pending.clone(),
                _ => next_run(&row.rule, tz, now())?,
            };
            db.exec(
                "UPDATE collection_schedules SET next_run=?,last_error=NULL WHERE id=?",
                [&deadline, &row.id],
            )?;
            let ms = chrono::DateTime::parse_from_rfc3339(&deadline)
                .map_err(|_| Error::new("invalid_schedule", 500))?
                .timestamp_millis();
            soonest(ms);
        }
        Ok(earliest)
    }
}
