//! Schedule rows and the current/preceding-release encoding written to them.
use super::{Scheduled, next_run};
use crate::{
    Error, Result,
    collection::{
        api::types::ScheduleCollection,
        rules::{self, Rule, Timing},
    },
    db::{Db, FromRow, Row, Store, iso, now},
    ensure,
    foundation::crypto,
};
use chrono::{NaiveTime, TimeZone};
use chrono_tz::Tz;
use rusqlite::params;

// Keep the name and legacy timing writes for the preceding release.
const SAVE: &str = "INSERT INTO collection_schedules(id,name,collection,cadence,interval_minutes,\
    local_time,anchor,enabled,next_run,created_at,days,times,until_time,position) \
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,\
    collection=excluded.collection,cadence=excluded.cadence,\
    interval_minutes=excluded.interval_minutes,local_time=excluded.local_time,\
    anchor=excluded.anchor,enabled=excluded.enabled,next_run=excluded.next_run,last_error=NULL,\
    days=excluded.days,times=excluded.times,until_time=excluded.until_time,\
    position=excluded.position,\
    revision=collection_schedules.revision+1";

/// A row of `collection_schedules`: one rule of a collection's schedule.
#[derive(Clone)]
pub(super) struct ScheduleRow {
    pub(super) id: String,
    /// What it collects by: its collection's own schedule name, or an alias that reads it.
    pub(super) collection: ScheduleCollection,
    pub(super) rule: Rule,
    pub(super) enabled: bool,
    pub(super) next_run: Option<String>,
    revision: i64,
    pub(super) last_error: Option<String>,
    /// The first run of its single time or repeat, which only the previous release reads.
    anchor: i64,
}
impl FromRow for ScheduleRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let first = rules::time(&row.get::<String>("local_time")?)?;
        let written: Option<String> = row.get("times")?;
        let days: i64 = row.get("days")?;
        let interval: Option<i64> = match row.get::<String>("cadence")?.as_str() {
            "daily" => None,
            _ => Some(row.get("interval_minutes")?),
        };
        let rule = match interval {
            None => written_times(written.as_deref(), first),
            Some(every) => written_repeat(written.as_deref(), first, every, row.get("until_time")?),
        }
        .map(|timing| Rule { days, timing });
        // A row this release didn't write, or the previous release changed since, is what that
        // release ran: one time every day, or a repeat all day from its start. Changed back
        // just as it was, it is this release's again.
        let rule = match rule {
            Some(rule) => rule,
            None => Rule {
                days: rules::EVERY_DAY,
                timing: legacy_timing(first, interval)?,
            },
        };
        Ok(Self {
            id: row.get("id")?,
            collection: row.get("collection")?,
            rule,
            enabled: row.get("enabled")?,
            next_run: row.get("next_run")?,
            revision: row.get("revision")?,
            last_error: row.get("last_error")?,
            anchor: row.get("anchor")?,
        })
    }
}

/// The times of a times rule as this release wrote them, while the previous release kept its
/// first.
fn written_times(written: Option<&str>, first: NaiveTime) -> Option<Timing> {
    let times: Vec<String> = serde_json::from_str(written?).ok()?;
    let times = times
        .iter()
        .map(|at| rules::time(at).ok())
        .collect::<Option<Vec<_>>>()?;
    (times.first() == Some(&first)).then_some(Timing::At(times))
}
/// What this release wrote of a repeat: its start and interval, beside the time it runs until,
/// while the previous release kept both.
#[derive(serde::Deserialize, serde::Serialize)]
struct Written {
    from: String,
    every: i64,
}
fn written_repeat(
    written: Option<&str>,
    first: NaiveTime,
    interval: i64,
    until: Option<String>,
) -> Option<Timing> {
    let written: Written = serde_json::from_str(written?).ok()?;
    let to = rules::time(&until?).ok()?;
    (rules::time(&written.from).ok()? == first && written.every == interval).then_some(
        Timing::Repeat {
            every: written.every,
            from: first,
            to,
        },
    )
}
/// How the previous release runs a row: once a day at its time, or every `interval` minutes all
/// day from it, which, longer than a rule repeats, are the times it falls at.
fn legacy_timing(first: NaiveTime, interval: Option<i64>) -> Result<Timing> {
    let Some(every) = interval else {
        return Ok(Timing::At(vec![first]));
    };
    ensure(every > 0, "invalid_schedule", 500)?;
    if every <= rules::EVERY.1 {
        return Ok(Timing::Repeat {
            every,
            from: first,
            to: first,
        });
    }
    let mut times: Vec<NaiveTime> = (0..)
        .map(|run| run * every)
        .take_while(|offset| *offset < 24 * 60)
        .map(|offset| first + chrono::Duration::minutes(offset))
        .collect();
    times.sort();
    Ok(Timing::At(times))
}

/// When a rule first runs, as the previous release times a schedule: its first time today,
/// or the next day that has it.
pub fn anchor(time: NaiveTime, tz: Tz, after: i64) -> Result<i64> {
    let date = chrono::DateTime::from_timestamp_millis(after)
        .ok_or_else(|| Error::new("invalid_schedule", 400))?
        .with_timezone(&tz)
        .date_naive();
    (0..4)
        .find_map(|days| {
            tz.from_local_datetime(&(date + chrono::Duration::days(days)).and_time(time))
                .earliest()
        })
        .map(|first| first.timestamp_millis())
        .ok_or_else(|| Error::new("schedule_unresolvable", 400))
}
/// The single time and interval the previous release runs a rule by: its first time, or the
/// start of its repeat.
pub(super) fn legacy(rule: &Rule) -> (&'static str, Option<i64>, NaiveTime) {
    match &rule.timing {
        Timing::At(times) => ("daily", None, times[0]),
        Timing::Repeat { every, from, .. } => ("interval", Some(*every), *from),
    }
}

/// What a collection's schedule was at: each of its rules and the revision it was saved at.
pub(super) fn revision(rows: &[ScheduleRow]) -> String {
    let mut kept: Vec<String> = rows
        .iter()
        .map(|row| format!("{}.{}", row.id, row.revision))
        .collect();
    kept.sort();
    kept.join(",")
}

/// Writes a rule on the caller's DSP connection, retaining the preceding release's fields.
pub(super) fn save_rule(
    db: &Db,
    row: Option<&ScheduleRow>,
    collects: &ScheduleCollection,
    rule: &Rule,
    position: usize,
    enabled: bool,
    tz: Tz,
) -> Result<String> {
    let same = row.filter(|row| row.collection == *collects && row.rule == *rule);
    let (cadence, interval, first) = legacy(rule);
    let anchor = match same {
        Some(row) => row.anchor,
        None => anchor(first, tz, now())?,
    };
    let next = if !enabled {
        None
    } else {
        match same
            .filter(|row| row.enabled)
            .and_then(|row| row.next_run.clone())
        {
            Some(pending) => Some(pending),
            None => Some(next_run(rule, tz, now())?),
        }
    };
    let (times, until) = match &rule.timing {
        Timing::At(times) => {
            let times: Vec<String> = times.iter().map(|at| rules::text(*at)).collect();
            (serde_json::to_string(&times)?, None)
        }
        Timing::Repeat { every, from, to } => {
            let from = rules::text(*from);
            let written = Written {
                from,
                every: *every,
            };
            (serde_json::to_string(&written)?, Some(rules::text(*to)))
        }
    };
    let key = match row {
        Some(row) => row.id.clone(),
        None => crypto::id("schedule")?,
    };
    db.exec(
        SAVE,
        params![
            key,
            rule.describe().chars().take(60).collect::<String>(),
            collects,
            cadence,
            interval,
            rules::text(first),
            anchor,
            enabled,
            next,
            iso(),
            rule.days,
            times,
            until,
            position as i64
        ],
    )?;
    Ok(key)
}

impl Store {
    pub(super) fn rows_of(&self, id: &str, scheduled: Scheduled) -> Result<Vec<ScheduleRow>> {
        self.dsp(id)?.query_as(
            "SELECT * FROM collection_schedules \
            WHERE collection IN (SELECT value FROM json_each(?)) ORDER BY position,created_at,id",
            [scheduled.names()?],
        )
    }
}
