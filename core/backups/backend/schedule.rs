//! A DSP's backup schedule: whether it's on, and its rules, each on days of the week or dates of
//! the month at set times or every so often between two times (`when`), with how long what it
//! takes is kept and what it leaves out. A DSP no one has scheduled has the first rule every DSP
//! starts with: every day at 2:00 AM, kept 30 days, all but its pictures.
use super::{
    api::types::{BackupOn, BackupRule, BackupTiming, KeepFor, KeepUnit},
    contents,
    when::{self, EVERY_DAY, On, Timing, When},
};
use crate::{
    Error, Result,
    db::{Db, FromRow, Row, Store, at, now},
    ensure,
    foundation::{crypto, validate as v},
};
use chrono::{Months, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::params;
use serde_json::Value;

/// The most rules one DSP's schedule has.
pub const MAX_RULES: usize = 10;
/// The most of a unit a rule keeps what it takes for, and the most of its newest it keeps.
pub const MAX_KEEP: i64 = 999;
/// How many days of the coming weeks the Backups tab draws.
pub const CALENDAR_DAYS: i64 = 35;

/// How long what a rule takes is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    Forever,
    For(i64, KeepUnit),
    /// Its newest this many.
    Last(i64),
}
impl Keep {
    /// When what's taken at `taken` expires: never, while kept forever or as one of its rule's
    /// newest, which pruning counts.
    pub fn expires(self, taken: i64) -> Option<i64> {
        match self {
            Self::For(count, unit) => Some(expires(taken, count, unit)),
            Self::Forever | Self::Last(_) => None,
        }
    }
    /// How many of its rule's newest it's kept as one of, when it's kept so.
    pub fn last(self) -> Option<i64> {
        match self {
            Self::Last(count) => Some(count),
            _ => None,
        }
    }
}

/// One rule of a DSP's backup schedule, as it's kept: none kept yet for the rule every DSP starts
/// with.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub id: Option<String>,
    pub when: When,
    pub keep: Keep,
    pub skipped: Vec<String>,
    pub next_run: Option<i64>,
}
impl FromRow for Kept {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let on = match row.get::<BackupOn>("on_days")? {
            BackupOn::Week => On::Week(row.get("days")?),
            BackupOn::Month => On::Month {
                dates: row.get("dates")?,
                last: row.get("last_day")?,
            },
        };
        let timing = match row.get::<Option<i64>>("every")? {
            Some(every) => Timing::Repeat {
                every,
                from: when::time(&row.get::<String>("from_time")?)?,
                to: when::time(&row.get::<String>("to_time")?)?,
            },
            None => Timing::At(
                serde_json::from_str::<Vec<String>>(&row.get::<String>("times")?)?
                    .iter()
                    .map(|at| when::time(at))
                    .collect::<Result<_>>()?,
            ),
        };
        let keep = match row.get::<KeepFor>("keep")? {
            KeepFor::Forever => Keep::Forever,
            KeepFor::For => Keep::For(row.get("keep_count")?, row.get("keep_unit")?),
            KeepFor::Last => Keep::Last(row.get("keep_count")?),
        };
        Ok(Self {
            id: Some(row.get("id")?),
            when: When { on, timing },
            keep,
            skipped: serde_json::from_str(&row.get::<String>("skipped")?)?,
            next_run: row.get("next_run")?,
        })
    }
}
impl Kept {
    /// The rule every DSP starts with.
    fn first() -> Self {
        Self {
            id: None,
            when: When {
                on: On::Week(EVERY_DAY),
                timing: Timing::At(vec![when::time("02:00").expect("a time")]),
            },
            keep: Keep::For(30, KeepUnit::Day),
            skipped: vec![contents::PICTURES.into()],
            next_run: None,
        }
    }
    /// It as the Backups tab shows it, its next run shown while the schedule is on.
    pub fn shown(&self, tz: Tz, enabled: bool) -> BackupRule {
        let bits = |mask: i64, range: std::ops::RangeInclusive<i64>| {
            range.filter(|bit| mask & (1 << bit) != 0).collect()
        };
        let (on, days, dates, last_day) = match self.when.on {
            On::Week(days) => (BackupOn::Week, bits(days, 0..=6), vec![], false),
            On::Month { dates, last } => (BackupOn::Month, vec![], bits(dates, 1..=31), last),
        };
        let (kind, times, every, from, to) = match &self.when.timing {
            Timing::At(times) => (
                BackupTiming::Times,
                times.iter().map(|at| when::text(*at)).collect(),
                None,
                None,
                None,
            ),
            Timing::Repeat { every, from, to } => (
                BackupTiming::Repeat,
                vec![],
                Some(*every),
                Some(when::text(*from)),
                Some(when::text(*to)),
            ),
        };
        let (keep, keep_count, keep_unit) = match self.keep {
            Keep::Forever => (KeepFor::Forever, None, None),
            Keep::For(count, unit) => (KeepFor::For, Some(count), Some(unit)),
            Keep::Last(count) => (KeepFor::Last, Some(count), None),
        };
        let next_run = enabled
            .then(|| self.next_run.or_else(|| self.when.next(tz, now())))
            .flatten()
            .map(at);
        BackupRule {
            id: self.id.clone(),
            on,
            days,
            dates,
            last_day,
            kind,
            times,
            every,
            from,
            to,
            keep,
            keep_count,
            keep_unit,
            skipped: self.skipped.clone(),
            next_run,
        }
    }
    /// A rule as a request states it: when it runs as `When::parse` reads it, with `keep`,
    /// `keepCount`, `keepUnit` and `skipped`; `id`, the rule it changes, if it's kept already.
    fn parse(value: &Value) -> Result<Self> {
        v::fields(
            value,
            &[
                "id", "on", "days", "dates", "lastDay", "kind", "times", "every", "from", "to",
                "keep", "keepCount", "keepUnit", "skipped",
            ],
        )?;
        let when = When::parse(value)?;
        let keep = match v::choice(value, "keep", &["forever", "for", "last"])? {
            "forever" => Keep::Forever,
            "for" => Keep::For(
                v::integer(value, "keepCount", 1, MAX_KEEP)?,
                KeepUnit::parse(v::text(value, "keepUnit", 1, 8)?)
                    .ok_or_else(|| Error::new("invalid_input", 400))?,
            ),
            _ => Keep::Last(v::integer(value, "keepCount", 1, MAX_KEEP)?),
        };
        let skipped = value["skipped"]
            .as_array()
            .ok_or_else(|| Error::new("invalid_input", 400))?
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|item| (1..=64).contains(&item.len()) && *item != contents::DISPATCH)
                    .map(String::from)
                    .ok_or_else(|| Error::new("invalid_input", 400))
            })
            .collect::<Result<Vec<_>>>()?;
        let id = match &value["id"] {
            Value::Null => None,
            id => Some(
                id.as_str()
                    .filter(|id| crate::db::identifier(id, "bkr_"))
                    .ok_or_else(|| Error::new("invalid_input", 400))?
                    .to_owned(),
            ),
        };
        Ok(Self {
            id,
            when,
            keep,
            skipped,
            next_run: None,
        })
    }
}

/// `keep` `unit`s after `taken`, in milliseconds since the epoch: a month as the calendar has it.
pub fn expires(taken: i64, keep: i64, unit: KeepUnit) -> i64 {
    let day = 24 * 60 * 60 * 1000;
    let months = |count: i64| {
        Utc.timestamp_millis_opt(taken)
            .single()
            .and_then(|at| at.checked_add_months(Months::new(count as u32)))
            .map(|at| at.timestamp_millis())
            .unwrap_or(i64::MAX)
    };
    match unit {
        KeepUnit::Day => taken + keep * day,
        KeepUnit::Week => taken + keep * 7 * day,
        KeepUnit::Month => months(keep),
        KeepUnit::Year => months(keep * 12),
    }
}

/// A DSP's schedule: whether it's on, its revision and its rules, in order.
pub struct Schedule {
    pub enabled: bool,
    pub revision: i64,
    pub rules: Vec<Kept>,
}

const RULES: &str = "SELECT * FROM backup_rules WHERE dsp_id=? ORDER BY position";

/// Where the DSP is, which its rules' times are read in.
pub fn timezone(db: &Store, dsp: &str) -> Result<Tz> {
    let (zone,): (String,) = db
        .platform
        .one_as("SELECT timezone FROM dsps WHERE id=?", [dsp])?
        .ok_or_else(|| Error::new("dsp_not_found", 404))?;
    zone.parse()
        .map_err(|_| Error::new("invalid_timezone", 400))
}

/// The DSP's schedule as it's kept, or as every DSP's starts.
pub fn schedule(platform: &Db, dsp: &str) -> Result<Schedule> {
    let kept: Option<(bool, i64)> = platform.one_as(
        "SELECT enabled,revision FROM backup_schedules WHERE dsp_id=?",
        [dsp],
    )?;
    Ok(match kept {
        Some((enabled, revision)) => Schedule {
            enabled,
            revision,
            rules: platform.query_as(RULES, [dsp])?,
        },
        None => Schedule {
            enabled: true,
            revision: 0,
            rules: vec![Kept::first()],
        },
    })
}

/// Keeps the schedule every DSP starts with for one that has none kept: a DSP's first is kept
/// once there's somewhere to keep its backups.
pub fn start(db: &Store, dsp: &str, tz: Tz) -> Result<()> {
    if db
        .platform
        .one_as::<(i64,)>(
            "SELECT revision FROM backup_schedules WHERE dsp_id=?",
            [dsp],
        )?
        .is_some()
    {
        return Ok(());
    }
    db.platform.transaction(|| {
        db.platform.exec(
            "INSERT INTO backup_schedules(dsp_id,enabled,revision,generation) VALUES (?,1,1,?)",
            params![dsp, generation()?],
        )?;
        write_rules(&db.platform, dsp, &[Kept::first()], tz)
    })
}

fn generation() -> Result<String> {
    Ok(crypto::hex(&crypto::random::<16>()?))
}

/// The generation the DSP's backups' files are named for, once its schedule is kept.
pub fn generation_of(platform: &Db, dsp: &str) -> Result<String> {
    let (generation,): (String,) = platform
        .one_as(
            "SELECT generation FROM backup_schedules WHERE dsp_id=?",
            [dsp],
        )?
        .ok_or_else(|| Error::new("backup_schedule_missing", 409))?;
    Ok(generation)
}

/// The rules `value` states, at most `MAX_RULES` of them.
fn parsed(value: &Value) -> Result<Vec<Kept>> {
    value["rules"]
        .as_array()
        .filter(|rules| rules.len() <= MAX_RULES)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(Kept::parse)
        .collect()
}

/// Replaces the DSP's rules with those `value` states, if its `revision` is the schedule's: a
/// schedule changed since it was read is refused as `backup_schedule_changed`.
pub fn save(db: &Store, dsp: &str, value: &Value) -> Result<()> {
    v::fields(value, &["revision", "rules"])?;
    let revision = v::integer(value, "revision", 0, i64::MAX)?;
    let rules = parsed(value)?;
    let tz = timezone(db, dsp)?;
    db.platform.transaction(|| {
        let current = schedule(&db.platform, dsp)?;
        ensure(
            current.revision == revision,
            "backup_schedule_changed",
            409,
        )?;
        // A rule keeps its ID only if it's this DSP's.
        let ids: Vec<String> = current.rules.iter().filter_map(|k| k.id.clone()).collect();
        for rule in &rules {
            ensure(
                rule.id.as_ref().is_none_or(|id| ids.contains(id)),
                "invalid_input",
                400,
            )?;
        }
        db.platform.exec(
            "INSERT INTO backup_schedules(dsp_id,enabled,revision,generation) VALUES (?,1,1,?) \
             ON CONFLICT(dsp_id) DO UPDATE SET revision=revision+1",
            params![dsp, generation()?],
        )?;
        db.platform
            .exec("DELETE FROM backup_rules WHERE dsp_id=?", [dsp])?;
        write_rules(&db.platform, dsp, &rules, tz)
    })
}

fn write_rules(platform: &Db, dsp: &str, rules: &[Kept], tz: Tz) -> Result<()> {
    for (position, kept) in rules.iter().enumerate() {
        let (on, days, dates, last) = match kept.when.on {
            On::Week(days) => (BackupOn::Week, days, 0, false),
            On::Month { dates, last } => (BackupOn::Month, 0, dates, last),
        };
        let (times, every, from, to) = match &kept.when.timing {
            Timing::At(times) => (
                Some(serde_json::to_string(
                    &times.iter().map(|at| when::text(*at)).collect::<Vec<_>>(),
                )?),
                None,
                None,
                None,
            ),
            Timing::Repeat { every, from, to } => (
                None,
                Some(*every),
                Some(when::text(*from)),
                Some(when::text(*to)),
            ),
        };
        let (keep, count, unit) = match kept.keep {
            Keep::Forever => (KeepFor::Forever, None, None),
            Keep::For(count, unit) => (KeepFor::For, Some(count), Some(unit)),
            Keep::Last(count) => (KeepFor::Last, Some(count), None),
        };
        let id = match &kept.id {
            Some(id) => id.clone(),
            None => crypto::id("bkr")?,
        };
        platform.exec(
            "INSERT INTO backup_rules(id,dsp_id,position,on_days,days,dates,last_day,times,every,\
             from_time,to_time,keep,keep_count,keep_unit,skipped,next_run) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                id,
                dsp,
                position as i64,
                on,
                days,
                dates,
                last,
                times,
                every,
                from,
                to,
                keep,
                count,
                unit,
                serde_json::to_string(&kept.skipped)?,
                kept.when.next(tz, now()),
            ],
        )?;
    }
    Ok(())
}

/// Turns the DSP's schedule on or off: on, each rule runs next when it would from now.
pub fn switch(db: &Store, dsp: &str, enabled: bool) -> Result<()> {
    let tz = timezone(db, dsp)?;
    start(db, dsp, tz)?;
    db.platform.transaction(|| {
        db.platform.exec(
            "UPDATE backup_schedules SET enabled=?,revision=revision+1 WHERE dsp_id=?",
            params![enabled, dsp],
        )?;
        if enabled {
            let rules: Vec<Kept> = db.platform.query_as(RULES, [dsp])?;
            for kept in rules {
                db.platform.exec(
                    "UPDATE backup_rules SET next_run=? WHERE id=?",
                    params![kept.when.next(tz, now()), kept.id],
                )?;
            }
        }
        Ok(())
    })
}

/// When `rules` run, in the DSP's timezone `tz`: their next `count` runs, and which of them run on
/// each of the coming weeks' days, from today.
pub type Runs = (Vec<String>, Vec<(NaiveDate, Vec<usize>)>);
pub fn runs(rules: &[When], tz: Tz, count: usize) -> Runs {
    let upcoming = when::upcoming(rules, tz, now(), count)
        .into_iter()
        .map(at)
        .collect();
    let today = Utc::now().with_timezone(&tz).date_naive();
    (upcoming, when::calendar(rules, today, CALENDAR_DAYS))
}

/// When the rules `value` states would run, in the DSP's timezone.
pub fn preview(db: &Store, dsp: &str, value: &Value, count: usize) -> Result<Runs> {
    v::fields(value, &["rules"])?;
    let rules: Vec<When> = parsed(value)?.into_iter().map(|kept| kept.when).collect();
    Ok(runs(&rules, timezone(db, dsp)?, count))
}

/// The rules of the DSP's that are due by `now`, each moved to its next run.
pub fn due(platform: &Db, dsp: &str, tz: Tz, now: i64) -> Result<Vec<Kept>> {
    let rules: Vec<Kept> = platform.query_as(
        "SELECT r.* FROM backup_rules r JOIN backup_schedules s ON s.dsp_id=r.dsp_id \
         WHERE r.dsp_id=? AND s.enabled=1 AND r.next_run IS NOT NULL AND r.next_run<=? \
         ORDER BY r.position",
        params![dsp, now],
    )?;
    for kept in &rules {
        platform.exec(
            "UPDATE backup_rules SET next_run=? WHERE id=?",
            params![kept.when.next(tz, now), kept.id],
        )?;
    }
    Ok(rules)
}

/// Forgets the DSP's schedule as it's deleted, and who asked for its backups: what they hold is
/// kept until each expires, so a DSP deleted by mistake can still be brought back.
pub fn forget_dsp(platform: &Db, dsp: &str) -> Result<()> {
    platform.exec("DELETE FROM backup_rules WHERE dsp_id=?", [dsp])?;
    platform.exec("DELETE FROM backup_schedules WHERE dsp_id=?", [dsp])?;
    platform.exec(
        "UPDATE backups SET requested_by=NULL WHERE dsp_id=?",
        [dsp],
    )?;
    platform.exec(
        "UPDATE backups SET status='failed',finished_at=?,error='dsp_deleted' \
         WHERE dsp_id=? AND status='queued'",
        params![now(), dsp],
    )?;
    Ok(())
}

/// Runs the rule `id` again within the hour, after it failed, unless it runs sooner anyway.
pub fn retry(platform: &Db, id: &str, now: i64) -> Result<()> {
    platform.exec(
        "UPDATE backup_rules SET next_run=MIN(COALESCE(next_run,?1),?1) WHERE id=?2",
        params![now + 60 * 60 * 1000, id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/schedule.rs"]
mod tests;
