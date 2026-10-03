//! DSP-owned recurring collections.
use super::{
    Error, Result,
    collectors::Provider,
    contracts::{
        Cadence, CollectionSchedule, CollectionSchedules, ScheduleCollection, SchedulePreview,
    },
    crypto,
    db::{FromRow, Row, Store, at, iso, now},
    ensure, job_statuses,
    manifest::{Collection, registry},
    validate as v,
};
use chrono::{NaiveTime, TimeZone};
use rusqlite::params;
use serde_json::{Value, json};

const SAVE: &str = "INSERT INTO collection_schedules\
    (id,name,collection,cadence,interval_minutes,local_time,anchor,enabled,next_run,created_at) \
    VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,\
    collection=excluded.collection,cadence=excluded.cadence,\
    interval_minutes=excluded.interval_minutes,local_time=excluded.local_time,\
    anchor=excluded.anchor,enabled=excluded.enabled,next_run=excluded.next_run,last_error=NULL,\
    revision=collection_schedules.revision+1";
const PAUSE: &str = "UPDATE collection_schedules SET enabled=0,next_run=NULL,last_error=NULL,\
    revision=revision+1 WHERE enabled=1 AND collection IN (?, 'both')";
const RETIME: &str = "UPDATE collection_schedules SET anchor=?,next_run=?,last_error=NULL,\
    revision=revision+1 WHERE id=?";
const NEXT_DEADLINE: &str =
    "SELECT collection,next_run FROM collection_schedules WHERE enabled=1 ORDER BY next_run";
const ACTIVE_JOB: &str = concat!(
    "SELECT id FROM jobs WHERE dsp_id=? AND status IN ",
    job_statuses!(active),
    " LIMIT 1"
);

/// When a schedule runs: all that `next` needs, from a stored row or from a request.
struct Timing<'a> {
    cadence: Cadence,
    interval_minutes: Option<i64>,
    local_time: &'a str,
    anchor: i64,
}
/// A row of `collection_schedules`.
#[derive(Clone)]
pub struct ScheduleRow {
    pub schedule: CollectionSchedule,
    pub anchor: i64,
}
impl FromRow for ScheduleRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            schedule: CollectionSchedule {
                id: row.get("id")?,
                name: row.get("name")?,
                collection: row.get("collection")?,
                cadence: row.get("cadence")?,
                interval_minutes: row.get("interval_minutes")?,
                local_time: row.get("local_time")?,
                enabled: row.get("enabled")?,
                next_run: row.get("next_run")?,
                revision: row.get("revision")?,
                last_error: row.get("last_error")?,
            },
            anchor: row.get("anchor")?,
        })
    }
}
impl ScheduleRow {
    fn timing(&self) -> Timing<'_> {
        Timing {
            cadence: self.schedule.cadence,
            interval_minutes: self.schedule.interval_minutes,
            local_time: &self.schedule.local_time,
            anchor: self.anchor,
        }
    }
}

fn timezone(tz: &str) -> Result<chrono_tz::Tz> {
    tz.parse().map_err(|_| Error::new("invalid_timezone", 400))
}
fn local_time(time: &str) -> Result<NaiveTime> {
    let parsed = NaiveTime::parse_from_str(time, "%H:%M")
        .map_err(|_| Error::new("invalid_schedule_time", 400))?;
    ensure(
        parsed.format("%H:%M").to_string() == time,
        "invalid_schedule_time",
        400,
    )?;
    Ok(parsed)
}
fn local_instant(date: chrono::NaiveDate, time: NaiveTime, tz: chrono_tz::Tz) -> Option<i64> {
    // A repeated local time runs once, at its first occurrence. A missing time
    // on the spring transition is skipped, rather than inventing another time.
    tz.from_local_datetime(&date.and_time(time))
        .earliest()
        .map(|d| d.timestamp_millis())
}
pub fn next_daily(time: &str, tz: &str, after: i64) -> Result<String> {
    let time = local_time(time)?;
    let tz = timezone(tz)?;
    let date = chrono::DateTime::from_timestamp_millis(after)
        .ok_or_else(|| Error::new("invalid_schedule", 400))?
        .with_timezone(&tz)
        .date_naive();
    for days in 0..4 {
        let day = date + chrono::Duration::days(days);
        if let Some(instant) = local_instant(day, time, tz)
            && instant > after
        {
            return Ok(at(instant));
        }
    }
    Err(Error::new("schedule_unresolvable", 400))
}
fn anchor(time: &str, tz: &str, after: i64) -> Result<i64> {
    let tz = timezone(tz)?;
    let date = chrono::DateTime::from_timestamp_millis(after)
        .ok_or_else(|| Error::new("invalid_schedule", 400))?
        .with_timezone(&tz)
        .date_naive();
    let time = local_time(time)?;
    // If today's anchor falls into a DST gap, start at the next valid day.
    for days in 0..4 {
        if let Some(value) = local_instant(date + chrono::Duration::days(days), time, tz) {
            return Ok(value);
        }
    }
    Err(Error::new("schedule_unresolvable", 400))
}
fn next(row: &Timing<'_>, tz: &str, after: i64) -> Result<String> {
    if row.cadence == Cadence::Daily {
        return next_daily(row.local_time, tz, after);
    }
    let period = row.interval_minutes.unwrap_or(0) * 60000;
    ensure(period > 0, "invalid_schedule", 500)?;
    let start = row.anchor;
    Ok(at(if start > after {
        start
    } else {
        start + ((after - start) / period + 1) * period
    }))
}
/// The requested cadence, interval and local time, validated.
fn timing(value: &Value) -> Result<(Cadence, Option<i64>, &str)> {
    let cadence = v::choice(value, "cadence", &["interval", "daily"])?;
    let cadence = Cadence::parse(cadence).ok_or_else(|| Error::new("invalid_input", 400))?;
    let time = v::text(value, "localTime", 5, 5)?;
    local_time(time)?;
    let minutes = if cadence == Cadence::Interval {
        let minutes = v::integer(value, "intervalMinutes", 30, 1440)?;
        ensure(minutes % 30 == 0, "invalid_schedule_interval", 400)?;
        Some(minutes)
    } else {
        ensure(value["intervalMinutes"].is_null(), "invalid_input", 400)?;
        None
    };
    Ok((cadence, minutes, time))
}
fn same_timing(row: &ScheduleRow, (cadence, minutes, time): (Cadence, Option<i64>, &str)) -> bool {
    let row = &row.schedule;
    row.cadence == cadence && row.interval_minutes == minutes && row.local_time == time
}
// The fields a member can edit, compared for the audit log.
pub fn schedule_changes(
    before: &CollectionSchedule,
    after: &CollectionSchedule,
) -> Vec<super::db::AuditChange> {
    let fields = |s: &CollectionSchedule| {
        [
            ("name", Some(s.name.clone())),
            ("collection", Some(s.collection.as_str().to_owned())),
            ("cadence", Some(s.cadence.as_str().to_owned())),
            (
                "interval",
                s.interval_minutes.map(|minutes| minutes.to_string()),
            ),
            ("time", Some(s.local_time.clone())),
            ("enabled", Some(s.enabled.to_string())),
        ]
    };
    fields(before)
        .into_iter()
        .zip(fields(after))
        .filter(|(before, after)| before.1 != after.1)
        .map(|(before, after)| (before.0, before.1, after.1))
        .collect()
}

impl Store {
    pub(crate) fn initialize_schedules(&self, id: &str) -> Result<()> {
        let db = self.dsp(id)?;
        // v0.0.9 imports the DSP's old single schedule unless this is set.
        if db.setting("collectionSchedules.initialized", json!(false))? != json!(true) {
            db.set("collectionSchedules.initialized", &json!(true))?;
        }
        Ok(())
    }
    pub fn collection_schedules(&self, id: &str) -> Result<CollectionSchedules> {
        let dsp = self.find_dsp(id)?;
        let rows: Vec<ScheduleRow> = self.dsp(id)?.query_as(
            "SELECT * FROM collection_schedules ORDER BY created_at,id",
            [],
        )?;
        Ok(CollectionSchedules {
            timezone: dsp.timezone,
            dsp_name: dsp.name,
            schedules: rows.into_iter().map(|row| row.schedule).collect(),
        })
    }
    pub fn collection_schedule(&self, id: &str, schedule: &str) -> Result<CollectionSchedule> {
        Ok(self.schedule_row(id, schedule)?.schedule)
    }
    fn schedule_row(&self, id: &str, schedule: &str) -> Result<ScheduleRow> {
        self.dsp(id)?
            .one_as("SELECT * FROM collection_schedules WHERE id=?", [schedule])?
            .ok_or_else(|| Error::new("schedule_not_found", 404))
    }
    // `both` selects every collector's first collection; any other value selects the
    // collection it names. Each comes with its provider.
    fn scheduled_collections(
        collection: ScheduleCollection,
    ) -> impl Iterator<Item = (Provider, &'static Collection)> {
        Provider::all().flat_map(move |provider| {
            provider
                .collector()
                .collections()
                .iter()
                .enumerate()
                .filter(move |(index, scheduled)| {
                    if collection == ScheduleCollection::BOTH {
                        *index == 0
                    } else {
                        collection.as_str() == scheduled.schedule
                    }
                })
                .map(move |(_, scheduled)| (provider, scheduled))
        })
    }
    /// Today, where the DSP is.
    pub(crate) fn local_date(&self, id: &str) -> Result<String> {
        let tz = timezone(&self.find_dsp(id)?.timezone)?;
        Ok(chrono::Utc::now()
            .with_timezone(&tz)
            .format("%Y-%m-%d")
            .to_string())
    }
    fn check_schedule_sources(&self, id: &str, collection: ScheduleCollection) -> Result<()> {
        for (provider, scheduled) in Self::scheduled_collections(collection) {
            ensure(
                self.connection_for(id, provider)?.enabled,
                scheduled.unconnected,
                409,
            )?;
            registry()
                .keeper(scheduled.job_kind)
                .schedule_ready(self, id)?;
        }
        Ok(())
    }
    pub fn preview_schedule(&self, id: &str, value: &Value) -> Result<SchedulePreview> {
        v::fields(
            value,
            &["scheduleId", "cadence", "intervalMinutes", "localTime"],
        )?;
        let requested = timing(value)?;
        let dsp = self.find_dsp(id)?;
        let tz = dsp.timezone.as_str();
        let before = if value.get("scheduleId").is_some() {
            Some(self.schedule_row(id, v::text(value, "scheduleId", 1, 128)?)?)
        } else {
            None
        };
        let start = match before.as_ref().filter(|row| same_timing(row, requested)) {
            Some(row) => row.anchor,
            None => anchor(requested.2, tz, now())?,
        };
        let (cadence, interval_minutes, local_time) = requested;
        let row = Timing {
            cadence,
            interval_minutes,
            local_time,
            anchor: start,
        };
        Ok(SchedulePreview {
            next_run: next(&row, tz, now())?,
        })
    }
    /// `save_schedule` as JSON, for the integration tests written against it.
    pub fn save_collection_schedule(
        &self,
        id: &str,
        schedule: Option<&str>,
        value: &Value,
    ) -> Result<Value> {
        Ok(serde_json::to_value(
            self.save_schedule(id, schedule, value)?,
        )?)
    }
    pub fn save_schedule(
        &self,
        id: &str,
        schedule: Option<&str>,
        value: &Value,
    ) -> Result<CollectionSchedule> {
        v::fields(
            value,
            &[
                "name",
                "collection",
                "cadence",
                "intervalMinutes",
                "localTime",
                "enabled",
                "revision",
            ],
        )?;
        let name = v::name(value, "name", 60)?;
        let choices: Vec<_> = ScheduleCollection::all()
            .map(ScheduleCollection::as_str)
            .collect();
        let collection = v::choice(value, "collection", &choices)?;
        let collection = ScheduleCollection::parse(collection)
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let requested = timing(value)?;
        let enabled = v::boolean(value, "enabled")?;
        let before = schedule.map(|key| self.schedule_row(id, key)).transpose()?;
        if let Some(before) = &before {
            ensure(
                before.schedule.revision == v::integer(value, "revision", 1, i64::MAX)?,
                "schedule_changed",
                409,
            )?;
        } else {
            let count = self
                .dsp(id)?
                .count("SELECT count(*) FROM collection_schedules", [])?;
            ensure(count < 50, "schedule_limit", 409)?;
        }
        if enabled {
            self.check_schedule_sources(id, collection)?;
        }
        let dsp = self.find_dsp(id)?;
        let tz = dsp.timezone.as_str();
        let key = schedule
            .map(str::to_owned)
            .unwrap_or(crypto::id("schedule")?);
        let (cadence, interval_minutes, local_time) = requested;
        let same_timing = before.as_ref().is_some_and(|r| same_timing(r, requested));
        let start = if same_timing {
            before.as_ref().unwrap().anchor
        } else {
            anchor(local_time, tz, now())?
        };
        let row = Timing {
            cadence,
            interval_minutes,
            local_time,
            anchor: start,
        };
        let next_run = if !enabled {
            None
        } else if same_timing && before.as_ref().is_some_and(|r| r.schedule.enabled) {
            before
                .as_ref()
                .and_then(|r| r.schedule.next_run.clone())
                .or(Some(next(&row, tz, now())?))
        } else {
            Some(next(&row, tz, now())?)
        };
        self.dsp(id)?.exec(
            SAVE,
            params![
                key,
                name,
                collection,
                cadence,
                interval_minutes,
                local_time,
                start,
                enabled,
                next_run,
                iso()
            ],
        )?;
        self.collection_schedule(id, &key)
    }
    /// `enable_schedule` as JSON, for the integration tests written against it.
    pub fn enable_collection_schedule(
        &self,
        id: &str,
        schedule: &str,
        value: &Value,
    ) -> Result<Value> {
        Ok(serde_json::to_value(
            self.enable_schedule(id, schedule, value)?,
        )?)
    }
    pub fn enable_schedule(
        &self,
        id: &str,
        schedule: &str,
        value: &Value,
    ) -> Result<CollectionSchedule> {
        v::fields(value, &["revision", "enabled"])?;
        let row = self.collection_schedule(id, schedule)?;
        let input = json!({
            "name":row.name,
            "collection":row.collection,
            "cadence":row.cadence,
            "intervalMinutes":row.interval_minutes,
            "localTime":row.local_time,
            "revision":value["revision"],
            "enabled":v::boolean(value, "enabled")?,
        });
        self.save_schedule(id, Some(schedule), &input)
    }
    pub fn delete_collection_schedule(
        &self,
        id: &str,
        schedule: &str,
        value: &Value,
    ) -> Result<()> {
        v::fields(value, &["revision"])?;
        let row = self.schedule_row(id, schedule)?;
        ensure(
            row.schedule.revision == v::integer(value, "revision", 1, i64::MAX)?,
            "schedule_changed",
            409,
        )?;
        self.dsp(id)?
            .exec("DELETE FROM collection_schedules WHERE id=?", [schedule])?;
        Ok(())
    }
    pub(crate) fn pause_provider_schedules(&self, id: &str, provider: Provider) -> Result<()> {
        for collection in provider.collector().collections() {
            self.dsp(id)?.exec(PAUSE, [collection.schedule])?;
        }
        Ok(())
    }
    pub(crate) fn retime_schedules(&self, id: &str, tz: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, None)
    }
    pub(crate) fn retime_feature_schedules(&self, id: &str, tz: &str, feature: &str) -> Result<()> {
        self.retime_schedules_for(id, tz, Some(feature))
    }
    fn retime_schedules_for(&self, id: &str, tz: &str, feature: Option<&str>) -> Result<()> {
        let db = self.dsp(id)?;
        db.transaction(|| {
            for mut row in db.query_as::<ScheduleRow>("SELECT * FROM collection_schedules", [])? {
                if feature.is_some_and(|f| {
                    crate::features::automation(row.schedule.collection.as_str()) != f
                }) {
                    continue;
                }
                row.anchor = anchor(&row.schedule.local_time, tz, now())?;
                let deadline = if row.schedule.enabled {
                    Some(next(&row.timing(), tz, now())?)
                } else {
                    None
                };
                db.exec(RETIME, params![row.anchor, deadline, row.schedule.id])?;
            }
            Ok(())
        })
    }
    pub fn schedule_deadlines(&self) -> Result<Vec<(String, i64)>> {
        let mut deadlines = Vec::new();
        let dsps: Vec<(String,)> = self.platform.query_as(
            "SELECT id FROM dsps WHERE status='active' AND environment=?",
            [&self.config.environment],
        )?;
        for (id,) in dsps {
            let enabled = self.features(&id)?;
            if !crate::features::automates(&enabled) {
                continue;
            }
            // The soonest schedule whose page is on; the others wait for their page.
            let rows: Vec<(String, Option<String>)> = self.dsp(&id)?.query_as(NEXT_DEADLINE, [])?;
            let next = rows
                .into_iter()
                .find(|(collection, _)| {
                    let page = crate::features::automation(collection);
                    enabled.iter().any(|f| f == page)
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
    fn enqueue_schedule(&self, id: &str, row: &CollectionSchedule) -> Result<()> {
        let pending = row.next_run.as_deref().unwrap_or("");
        let key = format!("schedule:{}:{}:", row.id, pending);
        // enqueue_batch commits all requests together. If any exists, this exact
        // occurrence already committed; do not rebuild date/scopes after a restart.
        if self
            .jobs
            .one(
                "SELECT id FROM jobs WHERE dsp_id=? AND substr(idempotency_key,1,?)=? LIMIT 1",
                params![id, key.len() as i64, key],
            )?
            .is_some()
        {
            return Ok(());
        }
        self.check_schedule_sources(id, row.collection)?;
        let mut requests = Vec::new();
        for (provider, scheduled) in Self::scheduled_collections(row.collection) {
            for (suffix, request) in registry().keeper(scheduled.job_kind).scheduled(self, id)? {
                requests.push((format!("{key}{suffix}"), provider, request));
            }
        }
        // Other collections finish before another recurring batch enters the queue.
        ensure(
            self.jobs.one(ACTIVE_JOB, [id])?.is_none(),
            "sync_in_progress",
            409,
        )?;
        self.enqueue_batch(id, None, &requests)?;
        Ok(())
    }
    pub fn schedule_due(&self, id: &str) -> Result<Option<i64>> {
        let dsp = self.find_dsp(id)?;
        let enabled = self.features(id)?;
        if !self.serves(&dsp) || !crate::features::automates(&enabled) {
            return Ok(None);
        }
        let db = self.dsp(id)?;
        let mut earliest = None;
        let rows: Vec<ScheduleRow> = db.query_as(
            "SELECT * FROM collection_schedules WHERE enabled=1 ORDER BY next_run,id",
            [],
        )?;
        for mut row in rows {
            // A schedule whose page is off waits, as every schedule does without the page.
            let page = crate::features::automation(row.schedule.collection.as_str());
            if !enabled.iter().any(|f| f == page) {
                continue;
            }
            let pending = row.schedule.next_run.clone();
            if pending.as_ref().is_some_and(|value| value <= &iso()) {
                if let Err(error) = self.enqueue_schedule(id, &row.schedule) {
                    db.exec(
                        "UPDATE collection_schedules SET last_error=? WHERE id=?",
                        [&error.code, &row.schedule.id],
                    )?;
                    let retry = now() + 60000;
                    earliest = Some(earliest.map_or(retry, |v: i64| v.min(retry)));
                    continue;
                }
                row.schedule.next_run = None;
            }
            let computed = next(&row.timing(), &dsp.timezone, now())?;
            let deadline = row.schedule.next_run.clone().unwrap_or(computed);
            db.exec(
                "UPDATE collection_schedules SET next_run=?,last_error=NULL WHERE id=?",
                [&deadline, &row.schedule.id],
            )?;
            let ms = chrono::DateTime::parse_from_rfc3339(&deadline)
                .map_err(|_| Error::new("invalid_schedule", 500))?
                .timestamp_millis();
            earliest = Some(earliest.map_or(ms, |v: i64| v.min(ms)));
        }
        Ok(earliest)
    }
}

#[cfg(test)]
#[path = "../tests/backend/schedules.rs"]
mod tests;
