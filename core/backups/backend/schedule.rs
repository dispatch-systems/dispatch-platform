//! A DSP's backup schedule: whether it's on, and its rules, each on chosen days at set times or
//! every so often between two times, as a collection's rules run (`collection::rules`), with how
//! long what it takes is kept and what it leaves out. A DSP no one has scheduled has the first
//! rule every DSP starts with: every day at 2:00 AM, kept 30 days, all but its pictures.
use super::{
    api::types::{BackupRule, KeepUnit},
    contents,
};
use crate::{
    Error, Result,
    collection::{
        api::types::RuleKind,
        rules::{self, Rule, Timing},
    },
    db::{Db, FromRow, Row, Store, at, now},
    ensure,
    foundation::{crypto, validate as v},
};
use chrono::{Months, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::params;
use serde_json::Value;

/// The most rules one DSP's schedule has.
pub const MAX_RULES: usize = 10;
/// The most of a unit a rule keeps what it takes for.
pub const MAX_KEEP: i64 = 999;

/// One rule of a DSP's backup schedule, as it's kept: none kept yet for the rule every DSP starts
/// with.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub id: Option<String>,
    pub rule: Rule,
    pub keep: i64,
    pub unit: KeepUnit,
    pub skipped: Vec<String>,
    pub next_run: Option<i64>,
}
impl FromRow for Kept {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let timing = match row.get::<Option<i64>>("every")? {
            Some(every) => Timing::Repeat {
                every,
                from: rules::time(&row.get::<String>("from_time")?)?,
                to: rules::time(&row.get::<String>("to_time")?)?,
            },
            None => Timing::At(
                serde_json::from_str::<Vec<String>>(&row.get::<String>("times")?)?
                    .iter()
                    .map(|at| rules::time(at))
                    .collect::<Result<_>>()?,
            ),
        };
        Ok(Self {
            id: Some(row.get("id")?),
            rule: Rule {
                days: row.get("days")?,
                timing,
            },
            keep: row.get("keep_count")?,
            unit: row.get("keep_unit")?,
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
            rule: Rule {
                days: rules::EVERY_DAY,
                timing: Timing::At(vec![rules::time("02:00").expect("a time")]),
            },
            keep: 30,
            unit: KeepUnit::Day,
            skipped: vec![contents::PICTURES.into()],
            next_run: None,
        }
    }
    /// When what it takes at `taken` expires.
    pub fn expires(&self, taken: i64) -> i64 {
        expires(taken, self.keep, self.unit)
    }
    /// It as the Backups tab shows it, its next run shown while the schedule is on.
    pub fn shown(&self, tz: Tz, enabled: bool) -> BackupRule {
        let days = (0..7).filter(|day| self.rule.days & (1 << day) != 0).collect();
        let (kind, times, every, from, to) = match &self.rule.timing {
            Timing::At(times) => (
                RuleKind::Times,
                times.iter().map(|at| rules::text(*at)).collect(),
                None,
                None,
                None,
            ),
            Timing::Repeat { every, from, to } => (
                RuleKind::Repeat,
                vec![],
                Some(*every),
                Some(rules::text(*from)),
                Some(rules::text(*to)),
            ),
        };
        let next_run = enabled
            .then(|| self.next_run.or_else(|| self.rule.next(tz, now())))
            .flatten()
            .map(at);
        BackupRule {
            id: self.id.clone(),
            kind,
            days,
            times,
            every,
            from,
            to,
            keep: self.keep,
            keep_unit: self.unit,
            skipped: self.skipped.clone(),
            next_run,
        }
    }
    /// A rule as a request states it: its timing as a collection's rule states it, with `keep`,
    /// `keepUnit` and `skipped`; `id`, the rule it changes, if it's kept already.
    fn parse(value: &Value) -> Result<Self> {
        v::fields(
            value,
            &[
                "id", "kind", "days", "times", "every", "from", "to", "keep", "keepUnit",
                "skipped",
            ],
        )?;
        let rule = Rule::parse(value)?;
        let keep = v::integer(value, "keep", 1, MAX_KEEP)?;
        let unit = KeepUnit::parse(v::text(value, "keepUnit", 1, 8)?)
            .ok_or_else(|| Error::new("invalid_input", 400))?;
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
            rule,
            keep,
            unit,
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
    zone.parse().map_err(|_| Error::new("invalid_timezone", 400))
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
        .one_as::<(i64,)>("SELECT revision FROM backup_schedules WHERE dsp_id=?", [dsp])?
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
        .one_as("SELECT generation FROM backup_schedules WHERE dsp_id=?", [dsp])?
        .ok_or_else(|| Error::new("backup_schedule_missing", 409))?;
    Ok(generation)
}

/// Replaces the DSP's rules with those `value` states, if its `revision` is the schedule's: a
/// schedule changed since it was read is refused as `backup_schedule_changed`.
pub fn save(db: &Store, dsp: &str, value: &Value) -> Result<()> {
    v::fields(value, &["revision", "rules"])?;
    let revision = v::integer(value, "revision", 0, i64::MAX)?;
    let rules = value["rules"]
        .as_array()
        .filter(|rules| rules.len() <= MAX_RULES)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(Kept::parse)
        .collect::<Result<Vec<_>>>()?;
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
        let (times, every, from, to) = match &kept.rule.timing {
            Timing::At(times) => (
                Some(serde_json::to_string(
                    &times.iter().map(|at| rules::text(*at)).collect::<Vec<_>>(),
                )?),
                None,
                None,
                None,
            ),
            Timing::Repeat { every, from, to } => (
                None,
                Some(*every),
                Some(rules::text(*from)),
                Some(rules::text(*to)),
            ),
        };
        let id = match &kept.id {
            Some(id) => id.clone(),
            None => crypto::id("bkr")?,
        };
        platform.exec(
            "INSERT INTO backup_rules(id,dsp_id,position,days,times,every,from_time,to_time,\
             keep_count,keep_unit,skipped,next_run) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                id,
                dsp,
                position as i64,
                kept.rule.days,
                times,
                every,
                from,
                to,
                kept.keep,
                kept.unit,
                serde_json::to_string(&kept.skipped)?,
                kept.rule.next(tz, now()),
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
            moved_on(&db.platform, dsp, tz, now())?;
        }
        Ok(())
    })
}

/// Moves each of the DSP's rules to the first time it runs after `after`.
fn moved_on(platform: &Db, dsp: &str, tz: Tz, after: i64) -> Result<()> {
    let rules: Vec<Kept> = platform.query_as(RULES, [dsp])?;
    for kept in rules {
        platform.exec(
            "UPDATE backup_rules SET next_run=? WHERE id=?",
            params![kept.rule.next(tz, after), kept.id],
        )?;
    }
    Ok(())
}

/// The next `count` runs of the rules `value` states, in the DSP's timezone.
pub fn preview(db: &Store, dsp: &str, value: &Value, count: usize) -> Result<Vec<String>> {
    v::fields(value, &["rules"])?;
    let rules = value["rules"]
        .as_array()
        .filter(|rules| rules.len() <= MAX_RULES)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(|rule| Kept::parse(rule).map(|kept| kept.rule))
        .collect::<Result<Vec<_>>>()?;
    let tz = timezone(db, dsp)?;
    Ok(rules::upcoming(&rules, tz, now(), count)
        .into_iter()
        .map(at)
        .collect())
}

/// A rule of the DSP's that's due by `now`, moved to its next run: its ID, and its keep.
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
            params![kept.rule.next(tz, now), kept.id],
        )?;
    }
    Ok(rules)
}

/// Forgets the DSP's schedule as it's deleted, and who asked for its backups: what they hold is
/// kept until each expires, so a DSP deleted by mistake can still be brought back.
pub fn forget_dsp(platform: &Db, dsp: &str) -> Result<()> {
    platform.exec("DELETE FROM backup_rules WHERE dsp_id=?", [dsp])?;
    platform.exec("DELETE FROM backup_schedules WHERE dsp_id=?", [dsp])?;
    platform.exec("UPDATE backups SET requested_by=NULL WHERE dsp_id=?", [dsp])?;
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
