//! Checked weeks and their active, verified publications at the DSP's station.
use super::{keeper::weekly_scorecard_today, storage::WeeklyScorecardStore};
use crate::api::types::{
    WeeklyScorecardDatasetCount, WeeklyScorecardPublication, WeeklyScorecardWeek,
    WeeklyScorecardWeeks,
};
use dispatch_core::{
    Result,
    db::{FromRow, Row, Store},
    foundation::weeks::last_completed_week,
};
use dispatch_cortex::weekly_scorecard::DATASETS;
use std::collections::HashMap;

struct CheckedWeekRow {
    week: String,
    checked_at: String,
    posted: i64,
    publication: Option<PublicationRow>,
}
struct PublicationRow {
    id: String,
    station: String,
    dsp_code: String,
    collected_at: String,
    row_count: i64,
}
impl FromRow for CheckedWeekRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let publication = match row.get::<Option<String>>("id")? {
            Some(id) => Some(PublicationRow {
                id,
                station: row.get("station")?,
                dsp_code: row.get("dsp_code")?,
                collected_at: row.get("collected_at")?,
                row_count: row.get("row_count")?,
            }),
            None => None,
        };
        Ok(Self {
            week: row.get("week")?,
            checked_at: row.get("checked_at")?,
            posted: row.get("posted")?,
            publication,
        })
    }
}
impl CheckedWeekRow {
    fn into_week(self, counts: &HashMap<(String, String), usize>) -> WeeklyScorecardWeek {
        let publication = self.publication.map(|publication| {
            let datasets = DATASETS
                .iter()
                .map(|dataset| {
                    let key = (publication.id.clone(), dataset.id.to_owned());
                    WeeklyScorecardDatasetCount {
                        id: dataset.id.into(),
                        table: dataset.table.into(),
                        rows: counts.get(&key).copied().unwrap_or(0),
                    }
                })
                .collect();
            WeeklyScorecardPublication {
                id: publication.id,
                week: self.week.clone(),
                station: publication.station,
                dsp_code: publication.dsp_code,
                collected_at: publication.collected_at,
                row_count: publication.row_count as usize,
                datasets,
            }
        });
        WeeklyScorecardWeek {
            week: self.week,
            posted: self.posted == 1,
            checked_at: self.checked_at,
            publication,
        }
    }
}

/// Every week checked so far at the DSP's station, newest first, with its
/// active publication and how many rows each table holds for it.
pub(super) fn weeks(store: &Store, id: &str) -> Result<WeeklyScorecardWeeks> {
    let today = weekly_scorecard_today(store, id)?;
    let station = store.profile(id)?.station_code;
    let db = store.weekly_scorecard_db(id)?;
    let states = db.query_as::<CheckedWeekRow>(
        "SELECT w.week,w.checked_at,w.posted,p.id,p.station,p.dsp_code,p.collected_at,p.row_count \
         FROM weekly_scorecard_weeks w LEFT JOIN weekly_scorecard_publications p \
         ON p.week=w.week AND p.station=w.station AND p.active=1 AND p.scope_verified=1 \
         WHERE w.station=? AND w.scope_verified=1 ORDER BY w.week DESC",
        [&station],
    )?;
    let publications: Vec<&str> = states
        .iter()
        .filter_map(|state| state.publication.as_ref().map(|p| p.id.as_str()))
        .collect();
    // Current publications record their dataset counts at capture time. Older
    // publications adopted before source metadata existed need a bounded fallback.
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for (publication_id, dataset, row_count) in db.query_as::<(String, String, i64)>(
        "SELECT publication_id,dataset,row_count FROM weekly_scorecard_sources \
         WHERE publication_id IN (SELECT value FROM json_each(?))",
        [serde_json::to_string(&publications)?],
    )? {
        counts.insert((publication_id, dataset), row_count as usize);
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
        for (publication_id, rows) in db.query_as::<(String, i64)>(
            &format!(
                "SELECT publication_id,count(*) rows FROM {} \
                 WHERE publication_id IN (SELECT value FROM json_each(?)) GROUP BY publication_id",
                dataset.table
            ),
            [serde_json::to_string(&missing)?],
        )? {
            counts.insert((publication_id, dataset.id.to_owned()), rows as usize);
        }
    }
    let weeks = states
        .into_iter()
        .map(|state| state.into_week(&counts))
        .collect();
    Ok(WeeklyScorecardWeeks {
        station,
        latest_week: last_completed_week(today),
        weeks,
    })
}
