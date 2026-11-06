//! Publish one verified week atomically, retaining shrink-confirmation counts and raw rows.
use super::{keeper, storage::WeeklyScorecardStore};
use dispatch_core::{
    Error, Result,
    db::{DspLease, Store, at, now, s},
    manifest::Keeper,
};
use dispatch_cortex::{
    discovery::Scope,
    weekly_scorecard::{Capture, DATASETS, Dataset, Request},
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
         WHERE week=? AND station=? AND company_id=? AND active=1 AND scope_verified=1",
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
/// The columns a row is indexed by, read from its fields when it has them.
pub(super) struct Keys {
    pub(super) data_date: Option<String>,
    pub(super) transporter_id: Option<String>,
    pub(super) tracking_id: Option<String>,
    pub(super) event_id: Option<String>,
    pub(super) impact: Option<i64>,
}
pub(super) fn text(row: &Value, key: &str) -> Option<String> {
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
pub(super) fn keys(dataset: &Dataset, row: &Value) -> Keys {
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

/// Stores a week's capture as its active publication, or records that the week
/// is not posted yet. One transaction: a reader never sees part of a week.
pub(super) fn publish(
    store: &Store,
    id: &str,
    job: &str,
    capture: &Capture,
    scope: &Scope,
) -> Result<()> {
    let request: Value = serde_json::from_str(&store.job_row(job, Some(id))?.request)?;
    let request = keeper::WeeklyScorecard.bind(store, id, &request)?;
    let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
    capture.validate_scope(&request, scope)?;
    let db = store.weekly_scorecard_db(id)?;
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
        if !hold {
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
