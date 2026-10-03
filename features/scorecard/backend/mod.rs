//! Weekly scorecards from Cortex's performance API: the datasets behind each scorecard
//! page, one week at a time, published into the DSP's scorecard database. Every row
//! Amazon sends is kept as JSON beside the keys reads filter on. Browser and HTTP
//! data is untrusted input.
#[path = "keeper.rs"]
pub mod keeper;
#[path = "people.rs"]
pub mod people;

use crate::{
    Error, Result,
    collectors::{
        AddedStorage,
        cortex::{
            self,
            discovery::Scope,
            scorecard::{Capture, Collection, DATASETS, Dataset, Request},
        },
    },
    contracts::{ScorecardDatasetCount, ScorecardPublication, ScorecardWeek, ScorecardWeeks},
    db::{Db, DspLease, Kind, Store, at, migrations::add_column, now, s},
    ensure,
    manifest::Keeper,
    read_cache::DataDomain,
    weeks::{last_completed_week, week_or_latest},
};
use chrono::NaiveDate;
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::HashMap;

pub const ADAPTER_VERSION: i64 = 1;
/// The scorecard weeks it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("scorecard");
/// The scorecard database beside `cortex.sqlite`.
pub const DATABASE: Kind = Kind::new("scorecard", 1);
pub static STORAGE: AddedStorage = AddedStorage {
    id: "scorecard",
    kind: DATABASE,
    marker: "storage.scorecard",
    source: "scorecard-v1",
    verify,
};
/// The latest week, found not posted yet, is asked for again this much later.
pub const RECHECK_AFTER_MS: i64 = 20 * 60 * 60 * 1000;
fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM scorecard_schema", [])? == vec![json!({"version":1})],
        "unsupported_scorecard_schema",
        503,
    )?;
    // Missing initialized tables fail closed, rather than recreating lost data.
    for table in [
        "scorecard_publications",
        "scorecard_weeks",
        "scorecard_sources",
    ]
    .into_iter()
    .chain(DATASETS.iter().map(|d| d.table))
    {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// Migration 3: whether a publication's and a week's station scope was verified.
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
fn keys(dataset: &Dataset, row: &Value) -> Keys {
    let impact = dataset.impact.and_then(|field| match row.get(field)? {
        Value::String(value) => match value.as_str() {
            "Y" | "y" | "true" | "TRUE" | "1" => Some(1),
            "N" | "n" | "false" | "FALSE" | "0" => Some(0),
            _ => None,
        },
        Value::Bool(value) => Some(i64::from(*value)),
        Value::Number(value) => value.as_i64().filter(|v| *v == 0 || *v == 1),
        _ => None,
    });
    Keys {
        data_date: text(row, "data_date"),
        transporter_id: text(row, "transporter_id"),
        tracking_id: text(row, "tracking_id"),
        event_id: text(row, "event_id"),
        impact,
    }
}

impl Store {
    pub fn scorecard(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }
    /// Today, where the DSP is.
    fn scorecard_today(&self, id: &str) -> Result<NaiveDate> {
        let tz: chrono_tz::Tz = self
            .find_dsp(id)?
            .timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        Ok(chrono::Utc::now().with_timezone(&tz).date_naive())
    }
    /// The station the DSP's profile names, which every scorecard request carries.
    fn scorecard_station(&self, id: &str) -> Result<String> {
        let station = self.profile(id)?.station_code;
        ensure(!station.is_empty(), "scorecard_station_required", 409)?;
        Ok(station)
    }
    pub(crate) fn bind_scorecard_request(&self, id: &str, week: &str) -> Result<Value> {
        let dsp = self.find_dsp(id)?;
        let profile = self.profile(id)?;
        ensure(
            !profile.station_code.is_empty(),
            "scorecard_station_required",
            409,
        )?;
        let request = Request {
            collection: Collection::Scorecard,
            week: week.into(),
            station: profile.station_code,
            timezone: dsp.timezone,
            dsp_name: dsp.name,
            dsp_abbreviation: profile.abbreviation,
        };
        request.validate()?;
        Ok(serde_json::to_value(request)?)
    }
    pub(crate) fn scorecard_request(&self, id: &str, week: &str) -> Result<Value> {
        let request = self.bind_scorecard_request(id, week)?;
        // Keep the durable job payload readable by the previous binary. The worker
        // binds live tenant context immediately before collection and publication.
        Ok(json!({"collection":"scorecard","week":request["week"],"station":request["station"]}))
    }
    /// Queues `week`, or the most recent completed one, answering with the job.
    pub fn enqueue_scorecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        week: Option<&str>,
    ) -> Result<Value> {
        let week = week_or_latest(week, self.scorecard_today(id)?)?;
        let request = self.scorecard_request(id, &week)?;
        let job = self
            .enqueue_batch(id, actor, &[(key.into(), cortex::PROVIDER, request)])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    pub(crate) fn scorecard_schedule_ready(&self, id: &str) -> Result<()> {
        self.scorecard_station(id).map(|_| ())
    }
    /// What a scheduled run collects: the latest completed week, until it is
    /// published. A week not posted yet is asked for again after `RECHECK_AFTER_MS`.
    pub fn scorecard_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let week = last_completed_week(self.scorecard_today(id)?);
        let station = self.scorecard_station(id)?;
        let db = self.scorecard(id)?;
        let published = db
            .one(
                "SELECT id FROM scorecard_publications WHERE week=? AND station=? AND active=1 AND scope_verified=1",
                [&week, &station],
            )?
            .is_some();
        let checked = db
            .one(
                "SELECT checked_at FROM scorecard_weeks WHERE week=? AND station=? AND scope_verified=1",
                [&week, &station],
            )?
            .and_then(|row| chrono::DateTime::parse_from_rfc3339(s(&row, "checked_at")).ok())
            .map(|at| now() - at.timestamp_millis());
        if published || checked.is_some_and(|age| age < RECHECK_AFTER_MS) {
            return Ok(Vec::new());
        }
        Ok(vec![(
            format!("scorecard:{week}"),
            self.scorecard_request(id, &week)?,
        )])
    }
    /// The API address the station's last publication read, with the company it
    /// named. A job reads there again without opening Cortex's page.
    pub fn scorecard_address(&self, id: &str, station: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .scorecard(id)?
            .one(
                "SELECT s.url,p.company_id FROM scorecard_publications p JOIN scorecard_sources s \
                 ON s.publication_id=p.id WHERE p.station=? AND s.source='api' \
                 AND p.scope_verified=1 \
                 ORDER BY p.collected_at DESC,s.dataset LIMIT 1",
                [station],
            )?
            .map(|row| (s(&row, "url").to_owned(), s(&row, "company_id").to_owned())))
    }
    /// Stores a week's capture as its active publication, or records that the week
    /// is not posted yet. One transaction: a reader never sees part of a week.
    pub fn publish_scorecard(
        &self,
        id: &str,
        job: &str,
        capture: &Capture,
        scope: &Scope,
    ) -> Result<()> {
        let request: Value = serde_json::from_str(&self.job_row(job, Some(id))?.request)?;
        let request = keeper::Scorecard.bind(self, id, &request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        capture.validate_scope(&request, scope)?;
        let db = self.scorecard(id)?;
        let checked_at = at(capture.finished_at);
        db.transaction(|| {
            if !capture.posted {
                // A week published before stays published; only the check is noted.
                db.exec(
                    "INSERT INTO scorecard_weeks(week,station,checked_at,posted,publication_id,scope_verified) \
                     VALUES (?,?,?,0,NULL,1) ON CONFLICT(week,station) DO UPDATE SET \
                     checked_at=excluded.checked_at,\
                     posted=CASE WHEN scorecard_weeks.scope_verified=1 THEN scorecard_weeks.posted ELSE 0 END,\
                     publication_id=CASE WHEN scorecard_weeks.scope_verified=1 \
                         THEN scorecard_weeks.publication_id ELSE NULL END,\
                     scope_verified=1",
                    params![capture.week, capture.station, checked_at],
                )?;
                return Ok(());
            }
            let publication = crate::crypto::id("scorecard")?;
            db.exec(
                "INSERT INTO scorecard_publications(id,job_id,week,station,company_id,dsp_code,started_at,\
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
            for (dataset, captured) in DATASETS.iter().zip(&capture.datasets) {
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
                    "INSERT INTO scorecard_sources(publication_id,dataset,source,url,row_count) \
                     VALUES (?,?,'api',?,?)",
                    params![
                        publication,
                        captured.id,
                        captured.source_url,
                        captured.rows.len() as i64
                    ],
                )?;
            }
            db.exec(
                "UPDATE scorecard_publications SET active=0 WHERE week=? AND station=? AND active=1",
                params![capture.week, capture.station],
            )?;
            db.exec(
                "UPDATE scorecard_publications SET active=1 WHERE id=?",
                [&publication],
            )?;
            db.exec(
                "INSERT INTO scorecard_weeks(week,station,checked_at,posted,publication_id,scope_verified) \
                 VALUES (?,?,?,1,?,1) ON CONFLICT(week,station) DO UPDATE SET \
                 checked_at=excluded.checked_at,posted=1,publication_id=excluded.publication_id,scope_verified=1",
                params![capture.week, capture.station, checked_at, publication],
            )?;
            Ok(())
        })
    }
    /// Every week checked so far at the DSP's station, newest first, with its
    /// active publication and how many rows each table holds for it.
    pub fn scorecard_weeks(&self, id: &str) -> Result<ScorecardWeeks> {
        let today = self.scorecard_today(id)?;
        let station = self.profile(id)?.station_code;
        let db = self.scorecard(id)?;
        let states = db.all(
            "SELECT w.week,w.checked_at,w.posted,p.id,p.station,p.dsp_code,p.collected_at,p.row_count \
             FROM scorecard_weeks w LEFT JOIN scorecard_publications p \
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
            "SELECT publication_id,dataset,row_count FROM scorecard_sources \
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
                let datasets: Vec<ScorecardDatasetCount> = DATASETS
                    .iter()
                    .map(|dataset| {
                        let key = (publication.to_owned(), dataset.id.to_owned());
                        ScorecardDatasetCount {
                            id: dataset.id.into(),
                            table: dataset.table.into(),
                            rows: counts.get(&key).copied().unwrap_or(0),
                        }
                    })
                    .collect();
                ScorecardPublication {
                    id: publication.into(),
                    week: s(&state, "week").into(),
                    station: s(&state, "station").into(),
                    dsp_code: s(&state, "dsp_code").into(),
                    collected_at: s(&state, "collected_at").into(),
                    row_count: state["row_count"].as_i64().unwrap_or(0) as usize,
                    datasets,
                }
            });
            weeks.push(ScorecardWeek {
                week: s(&state, "week").into(),
                posted: state["posted"] == 1,
                checked_at: s(&state, "checked_at").into(),
                publication,
            });
        }
        Ok(ScorecardWeeks {
            station,
            latest_week: last_completed_week(today),
            weeks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::cortex::scorecard::{dataset, fixture};
    #[test]
    fn requests_are_recognized_by_their_collection() {
        assert!(
            Request::parse(&json!({"date":"2026-09-13"}))
                .unwrap()
                .is_none()
        );
        let request =
            Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
                "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
                .unwrap()
                .unwrap();
        assert_eq!(
            request
                .interval(dataset("da_dsp_daily_psb_stop").unwrap())
                .unwrap()
                .0,
            "2026-09-13"
        );
        assert!(
            Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"tst1",
                "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
                .is_err()
        );
        assert!(
            Request::parse(
                &json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
                    "timezone":"America/Los_Angeles","dspName":"Fixture Delivery",
                    "dspAbbreviation":"FXTR","extra":1})
            )
            .is_err()
        );
    }
    #[test]
    fn the_fixture_is_a_valid_posted_week_and_its_keys_are_read_from_rows() {
        let request =
            Request::parse(&json!({"collection":"scorecard","week":"2026-W38","station":"TST1",
                "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
                .unwrap()
                .unwrap();
        let capture = fixture(&request).unwrap();
        assert!(capture.posted);
        assert_eq!(capture.datasets.len(), DATASETS.len());
        let returns = dataset("da_dsp_weekly_rts_deep_dive").unwrap();
        let rows = &capture
            .datasets
            .iter()
            .find(|d| d.id == returns.id)
            .unwrap()
            .rows;
        let first = keys(returns, &rows[0]);
        assert_eq!(first.impact, Some(1));
        assert_eq!(first.tracking_id.as_deref(), Some("TBA000000000001"));
        assert_eq!(keys(returns, &rows[1]).impact, Some(0));
        let concessions = dataset("da_dsp_station_daily_dsb_dnr_tba").unwrap();
        let rows = &capture
            .datasets
            .iter()
            .find(|d| d.id == concessions.id)
            .unwrap()
            .rows;
        assert_eq!(keys(concessions, &rows[0]).impact, Some(1));
        assert_eq!(
            keys(concessions, &rows[0]).data_date.as_deref(),
            Some("2026-09-13")
        );
        let mut other = capture.clone();
        other.posted = false;
        assert!(other.validate(&request).is_err());
    }
}
