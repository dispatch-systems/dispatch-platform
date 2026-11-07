//! Each DSP's backups as the platform keeps them: asked for, taken or not, and the files each
//! holds by what they're kept as, until each expires.
use super::{
    api::types::{Backup, BackupItem, DspBackups},
    contents, schedule,
};
use crate::{
    Error, Result,
    db::{Db, FromRow, Row, Store, at, now},
    foundation::crypto,
};
use rusqlite::params;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The most of a DSP's backups its tab lists.
const LISTED: i64 = 200;
/// How long a backup taken by hand is kept.
pub const BY_HAND_DAYS: i64 = 30;

impl FromRow for Backup {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            rule_id: row.get("rule_id")?,
            status: row.get("status")?,
            created_at: at(row.get("created_at")?),
            finished_at: row.get::<Option<i64>>("finished_at")?.map(at),
            expires_at: row.get::<Option<i64>>("expires_at")?.map(at),
            size: row.get("size")?,
            error: row.get("error")?,
        })
    }
}

/// The DSP's backups as its Backups tab shows them; `configured`, whether the server has
/// somewhere to keep them.
pub fn shown(db: &Store, dsp: &str, configured: bool) -> Result<DspBackups> {
    let tz = schedule::timezone(db, dsp)?;
    let kept = schedule::schedule(&db.platform, dsp)?;
    let rules: Vec<_> = kept
        .rules
        .iter()
        .map(|rule| rule.shown(tz, kept.enabled))
        .collect();
    let upcoming = if kept.enabled {
        let timings: Vec<_> = kept.rules.iter().map(|rule| rule.rule.clone()).collect();
        crate::collection::rules::upcoming(&timings, tz, now(), 5)
            .into_iter()
            .map(at)
            .collect()
    } else {
        vec![]
    };
    let items = contents::items(&db.dsp_folder(dsp)?)
        .into_iter()
        .map(|item| BackupItem {
            always: item.id == contents::DISPATCH,
            id: item.id,
            label: item.label,
        })
        .collect();
    Ok(DspBackups {
        configured,
        enabled: kept.enabled,
        revision: kept.revision,
        timezone: tz.name().to_owned(),
        items,
        rules,
        upcoming,
        backups: db.platform.query_as(
            "SELECT * FROM backups WHERE dsp_id=? ORDER BY created_at DESC LIMIT ?",
            params![dsp, LISTED],
        )?,
    })
}

/// Asks for a backup of the DSP: by the rule `rule`, or by hand by `by`, kept until `expires`.
pub fn ask(
    platform: &Db,
    dsp: &str,
    rule: Option<&str>,
    by: Option<&str>,
    expires: i64,
) -> Result<String> {
    let id = crypto::id("bak")?;
    platform.exec(
        "INSERT INTO backups(id,dsp_id,rule_id,requested_by,status,created_at,expires_at) \
         VALUES (?,?,?,?,'queued',?,?)",
        params![id, dsp, rule, by, now(), expires],
    )?;
    Ok(id)
}

/// Asks for a backup of the DSP by hand, unless one is waiting or being taken already.
pub fn ask_by_hand(platform: &Db, dsp: &str, by: &str) -> Result<()> {
    let waiting: Option<(String,)> = platform.one_as(
        "SELECT id FROM backups WHERE dsp_id=? AND status IN ('queued','running') LIMIT 1",
        [dsp],
    )?;
    if waiting.is_none() {
        let taken = now();
        ask(
            platform,
            dsp,
            None,
            Some(by),
            schedule::expires(taken, BY_HAND_DAYS, super::api::types::KeepUnit::Day),
        )?;
    }
    Ok(())
}

/// The backup that has waited longest: its ID, DSP and rule.
pub fn next(platform: &Db) -> Result<Option<(String, String, Option<String>)>> {
    platform.one_as(
        "SELECT id,dsp_id,rule_id FROM backups WHERE status='queued' ORDER BY created_at,id LIMIT 1",
        [],
    )
}

pub fn started(platform: &Db, id: &str) -> Result<()> {
    platform.exec("UPDATE backups SET status='running' WHERE id=?", [id])?;
    Ok(())
}

pub fn succeeded(platform: &Db, id: &str, files: &[Value], size: i64) -> Result<()> {
    platform.exec(
        "UPDATE backups SET status='succeeded',finished_at=?,size=?,files=?,error=NULL WHERE id=?",
        params![now(), size, serde_json::to_string(files)?, id],
    )?;
    Ok(())
}

/// The backup `id` wasn't taken, for `error`, though it sent the files `sent`, which it holds
/// until it expires.
pub fn failed(platform: &Db, id: &str, error: &str, sent: &[String]) -> Result<()> {
    let files: Vec<Value> = sent
        .iter()
        .map(|object| serde_json::json!({"object": object}))
        .collect();
    platform.exec(
        "UPDATE backups SET status='failed',finished_at=?,error=?,files=? WHERE id=?",
        params![now(), error, serde_json::to_string(&files)?, id],
    )?;
    Ok(())
}

/// Ends each backup the last server was taking when it stopped.
pub fn interrupted(platform: &Db) -> Result<()> {
    platform.exec(
        "UPDATE backups SET status='failed',finished_at=?,error='backup_interrupted' \
         WHERE status='running'",
        [now()],
    )?;
    Ok(())
}

/// A backup that expired: its ID and the files it held, by what they're kept as.
pub struct Expired {
    pub id: String,
    pub objects: Vec<String>,
}

/// The backups that expired by `now`, by their DSPs.
pub fn expired(platform: &Db, now: i64) -> Result<Vec<(String, Vec<Expired>)>> {
    let rows: Vec<(String, String, Option<String>)> = platform.query_as(
        "SELECT dsp_id,id,files FROM backups WHERE expires_at<=? \
         AND status IN ('succeeded','failed') ORDER BY dsp_id",
        [now],
    )?;
    let mut by_dsp: Vec<(String, Vec<Expired>)> = Vec::new();
    for (dsp, id, files) in rows {
        let expired = Expired {
            id,
            objects: objects(files.as_deref())?,
        };
        match by_dsp.last_mut() {
            Some((last, gone)) if *last == dsp => gone.push(expired),
            _ => by_dsp.push((dsp, vec![expired])),
        }
    }
    Ok(by_dsp)
}

/// Forgets the backups `ids`, once what they held is let go of.
pub fn forget(platform: &Db, ids: &[String]) -> Result<()> {
    for id in ids {
        platform.exec("DELETE FROM backups WHERE id=?", [id])?;
    }
    Ok(())
}

fn objects(files: Option<&str>) -> Result<Vec<String>> {
    let Some(files) = files else {
        return Ok(vec![]);
    };
    Ok(serde_json::from_str::<Vec<Value>>(files)?
        .iter()
        .filter_map(|file| file["object"].as_str().map(String::from))
        .collect())
}

/// The files the DSP's backups that are kept past `now` hold, by what they're kept as.
pub fn held(platform: &Db, dsp: &str, now: i64) -> Result<HashSet<String>> {
    let rows: Vec<(Option<String>,)> = platform.query_as(
        "SELECT files FROM backups WHERE dsp_id=? AND status='succeeded' AND expires_at>?",
        params![dsp, now],
    )?;
    let mut held = HashSet::new();
    for (files,) in rows {
        held.extend(objects(files.as_deref())?);
    }
    Ok(held)
}

/// How many bytes each file the DSP's backups hold holds, by what it's kept as.
pub fn sizes(platform: &Db, dsp: &str) -> Result<HashMap<String, i64>> {
    let rows: Vec<(String,)> = platform.query_as(
        "SELECT files FROM backups WHERE dsp_id=? AND status='succeeded' AND files IS NOT NULL",
        [dsp],
    )?;
    let mut sizes = HashMap::new();
    for (files,) in rows {
        for file in serde_json::from_str::<Vec<Value>>(&files)? {
            if let Some(object) = file["object"].as_str() {
                sizes.insert(object.to_owned(), file["size"].as_i64().unwrap_or(0));
            }
        }
    }
    Ok(sizes)
}

/// The files one of the DSP's backups holds, once it's taken.
pub fn files(platform: &Db, dsp: &str, id: &str) -> Result<(i64, Vec<Value>)> {
    let row: Option<(i64, Option<String>)> = platform.one_as(
        "SELECT created_at,files FROM backups WHERE id=? AND dsp_id=? AND status='succeeded'",
        params![id, dsp],
    )?;
    let (taken, files) = row.ok_or_else(|| Error::new("backup_not_found", 404))?;
    Ok((
        taken,
        serde_json::from_str(&files.ok_or_else(|| Error::new("backup_not_found", 404))?)?,
    ))
}
