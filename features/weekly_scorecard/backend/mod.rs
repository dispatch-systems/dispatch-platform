//! Weekly Scorecards from Cortex's performance API: the datasets behind each scorecard
//! page, one week at a time, published into the DSP's scorecard database. Every row
//! Amazon sends is kept as JSON beside the keys reads filter on. Browser and HTTP
//! data is untrusted input.
pub(crate) mod archive;
pub(crate) mod exposed;
pub(crate) mod keeper;
pub(crate) mod people;
pub(crate) mod retirement;
pub(crate) mod settings;

use crate::api::types::{
    WeeklyScorecardDatasetCount, WeeklyScorecardPublication, WeeklyScorecardWeek,
    WeeklyScorecardWeeks,
};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, at, migrations::add_column, now, s},
    ensure,
    foundation::weeks::{last_completed_week, week_or_latest},
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    weekly_scorecard::{Capture, Collection, DATASETS, Dataset, Request},
};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::HashMap;

pub const ADAPTER_VERSION: i64 = 1;
/// How long before a held read another must confirm what it saw; a dataset gone empty, as
/// it reads while Cortex answers nothing, waits longer.
const CONFIRM_AFTER_MS: i64 = 30 * 60_000;
const CONFIRM_EMPTY_AFTER_MS: i64 = 12 * 3_600_000;

/// Whether `capture` is held back rather than published: a dataset answered with fewer rows
/// than the week's active publication read, which is what Cortex answers while it republishes
/// a week, and no read held at least half an hour before, or twelve hours for a dataset gone
/// empty, saw the same counts for those datasets. A drop seen twice that far apart is what
/// the week now is, as a dispute settles.
fn held(db: &DspLease<'_>, capture: &Capture) -> Result<bool> {
    let Some(active) = db.one(
        "SELECT id,collected_at FROM weekly_scorecard_publications \
         WHERE week=? AND station=? AND company_id=? AND active=1",
        [&capture.week, &capture.station, &capture.company_id],
    )?
    else {
        return Ok(false);
    };
    let counts = |publication: &str| -> Result<HashMap<String, i64>> {
        Ok(db
            .all(
                "SELECT dataset,row_count FROM weekly_scorecard_sources WHERE publication_id=?",
                [publication],
            )?
            .iter()
            .map(|row| {
                (
                    s(row, "dataset").to_owned(),
                    row["row_count"].as_i64().unwrap_or(0),
                )
            })
            .collect())
    };
    let kept = counts(s(&active, "id"))?;
    let dropped: Vec<(&str, i64)> = capture
        .datasets
        .iter()
        .map(|captured| (captured.id.as_str(), captured.rows.len() as i64))
        .filter(|(dataset, rows)| kept.get(*dataset).is_some_and(|kept| rows < kept))
        .collect();
    if dropped.is_empty() {
        return Ok(false);
    }
    let wait = if dropped.iter().any(|(_, rows)| *rows == 0) {
        CONFIRM_EMPTY_AFTER_MS
    } else {
        CONFIRM_AFTER_MS
    };
    let earlier = db.all(
        "SELECT id FROM weekly_scorecard_publications WHERE week=? AND station=? AND company_id=? \
         AND active=0 AND collected_at>? AND collected_at<=?",
        params![
            capture.week,
            capture.station,
            capture.company_id,
            s(&active, "collected_at"),
            at(now() - wait)
        ],
    )?;
    for held in earlier {
        let seen = counts(s(&held, "id"))?;
        if dropped
            .iter()
            .all(|(dataset, rows)| seen.get(*dataset) == Some(rows))
        {
            return Ok(false);
        }
    }
    dispatch_core::foundation::observability::event(
        "info",
        "weekly_scorecard.publication_held",
        json!({"week": capture.week, "datasets": dropped.iter().map(|(dataset, rows)| {
            json!({"dataset": dataset, "rows": rows, "published": kept.get(*dataset)})
        }).collect::<Vec<_>>()}),
    );
    Ok(true)
}
/// The scorecard weeks it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("weekly_scorecard");
/// The weekly scorecard database beside `cortex.sqlite`. Its durable identity stays
/// `weekly_scorecard`; the startup import preserves prior publications.
pub const DATABASE: Kind = Kind::new("weekly_scorecard", 1);
pub static STORAGE: AddedStorage = AddedStorage {
    id: "weekly_scorecard",
    kind: DATABASE,
    marker: "storage.weekly_scorecard",
    source: "weekly-scorecard-v1",
    verify,
};
/// What starting the server does to storage kept before: imports the retired scorecard
/// database, then moves the policy its own database kept into Settings.
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    retirement::upgrade(store, id)?;
    settings::upgrade(store, id)
}
fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM weekly_scorecard_schema", [])? == vec![json!({"version":1})],
        "unsupported_weekly_scorecard_schema",
        503,
    )?;
    // Missing initialized tables fail closed, rather than recreating lost data.
    for table in [
        "weekly_scorecard_publications",
        "weekly_scorecard_weeks",
        "weekly_scorecard_sources",
    ]
    .into_iter()
    .chain(DATASETS.iter().map(|d| d.table))
    {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// Whether the current publication and week have verified station scope.
pub(crate) fn add_weekly_verified_scope(db: &Db) -> Result<()> {
    for table in ["weekly_scorecard_publications", "weekly_scorecard_weeks"] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}

/// Preserve the shipped migration's identity and behavior for retired databases.
pub(crate) fn add_verified_scope(db: &Db) -> Result<()> {
    for table in ["scorecard_publications", "scorecard_weeks"] {
        add_column(
            db,
            table,
            "scope_verified",
            "INTEGER NOT NULL DEFAULT 0 CHECK(scope_verified IN (0,1))",
        )?;
    }
    Ok(())
}

/// The columns a row is indexed by, read from its fields when it has them.
struct Keys {
    data_date: Option<String>,
    transporter_id: Option<String>,
    tracking_id: Option<String>,
    event_id: Option<String>,
    impact: Option<i64>,
}
fn text(row: &Value, key: &str) -> Option<String> {
    match row.get(key)? {
        Value::String(value) if !value.is_empty() => Some(value.chars().take(256).collect()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
/// Whether a flag Amazon writes is set: Y or N, true or false, 1 or 0, as text, a boolean or a
/// number. None when it's anything else.
pub(crate) fn flag(value: &Value) -> Option<bool> {
    match value {
        Value::String(value) => match value.as_str() {
            "Y" | "y" | "true" | "TRUE" | "1" => Some(true),
            "N" | "n" | "false" | "FALSE" | "0" => Some(false),
            _ => None,
        },
        Value::Bool(value) => Some(*value),
        Value::Number(value) => match value.as_f64()? {
            1.0 => Some(true),
            0.0 => Some(false),
            _ => None,
        },
        _ => None,
    }
}
fn keys(dataset: &Dataset, row: &Value) -> Keys {
    let impact = dataset
        .impact
        .and_then(|field| flag(row.get(field)?))
        .map(i64::from);
    Keys {
        data_date: text(row, "data_date"),
        transporter_id: text(row, "transporter_id"),
        tracking_id: text(row, "tracking_id"),
        event_id: text(row, "event_id"),
        impact,
    }
}

/// What Weekly Scorecard reads and writes for a DSP: its weekly scorecards, in the database beside
/// Cortex's, and the collections that bring them.
pub trait WeeklyScorecardStore {
    fn weekly_scorecard_db(&self, id: &str) -> Result<DspLease<'_>>;
    fn enqueue_weekly_scorecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
    ) -> Result<Value>;
    fn weekly_scorecard_jobs(&self, id: &str) -> Result<Vec<(String, Value)>>;
    fn weekly_scorecard_address(&self, id: &str, station: &str)
    -> Result<Option<(String, String)>>;
    fn publish_weekly_scorecard(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()>;
    fn weekly_scorecard_weeks(&self, id: &str) -> Result<WeeklyScorecardWeeks>;
}
impl WeeklyScorecardStore for Store {
    fn weekly_scorecard_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }
    /// Queues `week`, or the most recent completed one, answering with the job.
    fn enqueue_weekly_scorecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
    ) -> Result<Value> {
        let week = week_or_latest(week, weekly_scorecard_today(self, id)?)?;
        let request = weekly_scorecard_request(self, id, &week)?;
        let job = self
            .enqueue_batch(id, actor, &[(key.into(), cortex::PROVIDER, request)])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    /// Refresh recent completed weeks, including posted weeks whose outcomes can change.
    fn weekly_scorecard_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let policy: settings::Policy =
            dispatch_core::settings::read(self, id, &settings::COLLECTION)?;
        let latest = last_completed_week(weekly_scorecard_today(self, id)?);
        let (_, saturday) = dispatch_core::foundation::weeks::week_days(&latest)?;
        let station = weekly_scorecard_station(self, id)?;
        let data = self.weekly_scorecard_db(id)?;
        let cutoff = at(now() - i64::from(policy.refresh_hours) * 3_600_000);
        let mut jobs = Vec::new();
        for offset in 0..policy.lookback_weeks {
            let day = saturday - chrono::Duration::weeks(i64::from(offset));
            let iso = chrono::Datelike::iso_week(&day);
            let week = format!("{}-W{:02}", iso.year(), iso.week());
            if data.one("SELECT week FROM weekly_scorecard_weeks WHERE week=? AND station=? AND scope_verified=1 AND checked_at>=?",
                [&week,&station,&cutoff])?.is_none() {
                jobs.push((format!("weekly_scorecard:{week}"),weekly_scorecard_request(self,id,&week)?));
            }
        }
        Ok(jobs)
    }
    /// The API address the station's last publication read, with the company it
    /// named. A job reads there again without opening Cortex's page.
    fn weekly_scorecard_address(
        &self,
        id: &str,
        station: &str,
    ) -> Result<Option<(String, String)>> {
        Ok(self
            .weekly_scorecard_db(id)?
            .one(
                "SELECT s.url,p.company_id FROM weekly_scorecard_publications p JOIN weekly_scorecard_sources s \
                 ON s.publication_id=p.id WHERE p.station=? AND s.source='api' \
                 AND p.scope_verified=1 \
                 ORDER BY p.collected_at DESC,s.dataset LIMIT 1",
                [station],
            )?
            .map(|row| (s(&row, "url").to_owned(), s(&row, "company_id").to_owned())))
    }
    /// Stores a week's capture as its active publication, or records that the week
    /// is not posted yet. One transaction: a reader never sees part of a week.
    fn publish_weekly_scorecard(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()> {
        let request: Value = serde_json::from_str(&self.job_row(job, Some(id))?.request)?;
        let request = keeper::WeeklyScorecard.bind(self, id, &request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        capture.validate_scope(&request, scope)?;
        let db = self.weekly_scorecard_db(id)?;
        let checked_at = at(capture.finished_at);
        db.transaction(|| {
            if !capture.posted {
                // A week published before stays published; only the check is noted.
                db.exec(
                    "INSERT INTO weekly_scorecard_weeks(week,station,checked_at,posted,publication_id,scope_verified) \
                     VALUES (?,?,?,0,NULL,1) ON CONFLICT(week,station) DO UPDATE SET \
                     checked_at=excluded.checked_at,\
                     posted=CASE WHEN weekly_scorecard_weeks.scope_verified=1 THEN weekly_scorecard_weeks.posted ELSE 0 END,\
                     publication_id=CASE WHEN weekly_scorecard_weeks.scope_verified=1 \
                         THEN weekly_scorecard_weeks.publication_id ELSE NULL END,\
                     scope_verified=1",
                    params![capture.week, capture.station, checked_at],
                )?;
                return Ok(());
            }
            let hold = held(&db, capture)?;
            let publication = dispatch_core::foundation::crypto::id("weekly_scorecard")?;
            db.exec(
                "INSERT INTO weekly_scorecard_publications(id,job_id,week,station,company_id,dsp_code,started_at,\
                 collected_at,active,row_count,adapter_version,scope_verified) VALUES (?,?,?,?,?,?,?,?,0,?,?,1)",
                params![
                    publication,
                    job,
                    capture.week,
                    capture.station,
                    capture.company_id,
                    capture.dsp_code,
                    at(capture.started_at),
                    checked_at,
                    capture.row_count() as i64,
                    ADAPTER_VERSION
                ],
            )?;
            // A held week keeps its counts alone, which a later read is compared with.
            for (dataset, captured) in DATASETS
                .iter()
                .zip(&capture.datasets)
                .filter(|_| !hold)
            {
                let sql = format!(
                    "INSERT INTO {}(publication_id,row_index,week,data_date,transporter_id,tracking_id,\
                     event_id,impact,row) VALUES (?,?,?,?,?,?,?,?,?)",
                    dataset.table
                );
                for (index, row) in captured.rows.iter().enumerate() {
                    let keys = keys(dataset, row);
                    db.exec(
                        &sql,
                        params![
                            publication,
                            index as i64,
                            capture.week,
                            keys.data_date,
                            keys.transporter_id,
                            keys.tracking_id,
                            keys.event_id,
                            keys.impact,
                            row.to_string()
                        ],
                    )?;
                }
            }
            for captured in &capture.datasets {
                db.exec(
                    "INSERT INTO weekly_scorecard_sources(publication_id,dataset,source,url,row_count) \
                     VALUES (?,?,'api',?,?)",
                    params![
                        publication,
                        captured.id,
                        captured.source_url,
                        captured.rows.len() as i64
                    ],
                )?;
            }
            // A held week keeps its active publication, and stays due to be read again.
            if hold {
                return Ok(());
            }
            db.exec(
                "UPDATE weekly_scorecard_publications SET active=0 WHERE week=? AND station=? AND active=1",
                params![capture.week, capture.station],
            )?;
            db.exec(
                "UPDATE weekly_scorecard_publications SET active=1 WHERE id=?",
                [&publication],
            )?;
            db.exec(
                "INSERT INTO weekly_scorecard_weeks(week,station,checked_at,posted,publication_id,scope_verified) \
                 VALUES (?,?,?,1,?,1) ON CONFLICT(week,station) DO UPDATE SET \
                 checked_at=excluded.checked_at,posted=1,publication_id=excluded.publication_id,scope_verified=1",
                params![capture.week, capture.station, checked_at, publication],
            )?;
            Ok(())
        })
    }
    /// Every week checked so far at the DSP's station, newest first, with its
    /// active publication and how many rows each table holds for it.
    fn weekly_scorecard_weeks(&self, id: &str) -> Result<WeeklyScorecardWeeks> {
        let today = weekly_scorecard_today(self, id)?;
        let station = self.profile(id)?.station_code;
        let db = self.weekly_scorecard_db(id)?;
        let states = db.all(
            "SELECT w.week,w.checked_at,w.posted,p.id,p.station,p.dsp_code,p.collected_at,p.row_count \
             FROM weekly_scorecard_weeks w LEFT JOIN weekly_scorecard_publications p \
             ON p.week=w.week AND p.station=w.station AND p.active=1 AND p.scope_verified=1 \
             WHERE w.station=? AND w.scope_verified=1 ORDER BY w.week DESC",
            [&station],
        )?;
        let publications: Vec<&str> = states
            .iter()
            .filter_map(|state| state["id"].as_str())
            .collect();
        // Current publications record their dataset counts at capture time. Older
        // publications adopted before source metadata existed need a bounded fallback.
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for row in db.all(
            "SELECT publication_id,dataset,row_count FROM weekly_scorecard_sources \
             WHERE publication_id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&publications)?],
        )? {
            counts.insert(
                (s(&row, "publication_id").into(), s(&row, "dataset").into()),
                row["row_count"].as_i64().unwrap_or(0) as usize,
            );
        }
        for dataset in DATASETS {
            let missing: Vec<&str> = publications
                .iter()
                .copied()
                .filter(|publication| {
                    !counts.contains_key(&(publication.to_string(), dataset.id.to_string()))
                })
                .collect();
            if missing.is_empty() {
                continue;
            }
            for row in db.all(
                &format!(
                    "SELECT publication_id,count(*) rows FROM {} \
                     WHERE publication_id IN (SELECT value FROM json_each(?)) GROUP BY publication_id",
                    dataset.table
                ),
                [serde_json::to_string(&missing)?],
            )? {
                counts.insert(
                    (s(&row, "publication_id").to_owned(), dataset.id.to_owned()),
                    row["rows"].as_i64().unwrap_or(0) as usize,
                );
            }
        }
        let mut weeks = Vec::new();
        for state in states {
            let publication = state["id"].as_str().map(|publication| {
                let datasets: Vec<WeeklyScorecardDatasetCount> = DATASETS
                    .iter()
                    .map(|dataset| {
                        let key = (publication.to_owned(), dataset.id.to_owned());
                        WeeklyScorecardDatasetCount {
                            id: dataset.id.into(),
                            table: dataset.table.into(),
                            rows: counts.get(&key).copied().unwrap_or(0),
                        }
                    })
                    .collect();
                WeeklyScorecardPublication {
                    id: publication.into(),
                    week: s(&state, "week").into(),
                    station: s(&state, "station").into(),
                    dsp_code: s(&state, "dsp_code").into(),
                    collected_at: s(&state, "collected_at").into(),
                    row_count: state["row_count"].as_i64().unwrap_or(0) as usize,
                    datasets,
                }
            });
            weeks.push(WeeklyScorecardWeek {
                week: s(&state, "week").into(),
                posted: state["posted"] == 1,
                checked_at: s(&state, "checked_at").into(),
                publication,
            });
        }
        Ok(WeeklyScorecardWeeks {
            station,
            latest_week: last_completed_week(today),
            weeks,
        })
    }
}
/// Where the DSP is.
fn weekly_scorecard_timezone(store: &Store, id: &str) -> Result<chrono_tz::Tz> {
    store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))
}
/// Today, where the DSP is.
fn weekly_scorecard_today(store: &Store, id: &str) -> Result<NaiveDate> {
    Ok(dispatch_core::foundation::clock::today(
        weekly_scorecard_timezone(store, id)?,
    ))
}
/// The station the DSP's profile names, which every scorecard request carries.
fn weekly_scorecard_station(store: &Store, id: &str) -> Result<String> {
    let station = store.profile(id)?.station_code;
    ensure(
        !station.is_empty(),
        "weekly_scorecard_station_required",
        409,
    )?;
    Ok(station)
}
pub(crate) fn bind_weekly_scorecard_request(store: &Store, id: &str, week: &str) -> Result<Value> {
    let dsp = store.find_dsp(id)?;
    let profile = store.profile(id)?;
    ensure(
        !profile.station_code.is_empty(),
        "weekly_scorecard_station_required",
        409,
    )?;
    let request = Request {
        collection: Collection::WeeklyScorecard,
        week: week.into(),
        station: profile.station_code,
        timezone: dsp.timezone,
        dsp_name: dsp.name,
        dsp_abbreviation: profile.abbreviation,
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}
pub(crate) fn weekly_scorecard_request(store: &Store, id: &str, week: &str) -> Result<Value> {
    let request = bind_weekly_scorecard_request(store, id, week)?;
    // Bind live tenant context immediately before collection and publication.
    Ok(json!({"collection":"weekly_scorecard","week":request["week"],"station":request["station"]}))
}
pub(crate) fn weekly_scorecard_schedule_ready(store: &Store, id: &str) -> Result<()> {
    weekly_scorecard_station(store, id).map(|_| ())
}

#[cfg(test)]
#[path = "../tests/backend/mod.rs"]
mod tests;
