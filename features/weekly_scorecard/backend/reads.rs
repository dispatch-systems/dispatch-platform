//! Checked weeks and their active, verified publications at the DSP's station.
use super::{keeper::weekly_scorecard_today, storage::WeeklyScorecardStore};
use crate::api::types::{
    WeeklyScorecardDatasetCount, WeeklyScorecardPublication, WeeklyScorecardWeek,
    WeeklyScorecardWeeks,
};
use dispatch_core::{
    Result,
    db::{Store, s},
    foundation::weeks::last_completed_week,
};
use dispatch_cortex::weekly_scorecard::DATASETS;
use std::collections::HashMap;

/// Every week checked so far at the DSP's station, newest first, with its
/// active publication and how many rows each table holds for it.
pub(super) fn weeks(store: &Store, id: &str) -> Result<WeeklyScorecardWeeks> {
    let today = weekly_scorecard_today(store, id)?;
    let station = store.profile(id)?.station_code;
    let db = store.weekly_scorecard_db(id)?;
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
