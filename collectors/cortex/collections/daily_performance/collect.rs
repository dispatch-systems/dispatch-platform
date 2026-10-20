//! The days a collection was asked for over the shared authenticated performance transport.
//! On a job's first attempt each run of up to a week of consecutive days is one read per
//! dataset, its rows given back to their days; a retry, whatever the first attempt met,
//! reads each day on its own as a job of one day always did.
use super::{Capture, Collection, DATASETS, Days, Request};
use crate::{
    codes,
    connection::{
        Driver,
        performance::{Found, mismatched},
    },
    discovery::Scope,
    performance::{Dataset, DatasetCapture, Read},
};
use chrono::NaiveDate;
use dispatch_core::{Error, Result, collection::browser::Run, db::now};
use serde_json::Value;
use std::collections::BTreeMap;

/// Reads at once: every dataset of a run of days together. Cortex answered all 22 at once
/// without throttling, in about a fifth of the time six at once took. Each reply is held
/// until all have answered; the largest day seen was 2 MB, a week's range seven times that.
const LANES: usize = 22;

/// The most days one range reads: a week, as read side by side with each day's own reads.
const RANGE_DAYS: usize = 7;

/// The request's days as runs of consecutive days of at most a week, first and last of each.
fn runs(dates: &[String]) -> Result<Vec<(String, String)>> {
    let mut runs: Vec<(String, String, NaiveDate, usize)> = Vec::new();
    for text in dates {
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        match runs.last_mut() {
            Some((_, last, previous, days))
                if previous.succ_opt() == Some(date) && *days < RANGE_DAYS =>
            {
                *last = text.clone();
                *previous = date;
                *days += 1;
            }
            _ => runs.push((text.clone(), text.clone(), date, 1)),
        }
    }
    Ok(runs
        .into_iter()
        .map(|(first, last, ..)| (first, last))
        .collect())
}
/// One dataset's rows over the days `from` to `to`, as each day's own read answers them.
/// One day's read is that day's. A dataset read as a range gives each day the rows naming
/// it; the thresholds, which name no station and whose rows name no day, answer every day
/// alike, as each day's read did when both were read side by side. None when a station's row
/// names no day, or a row a day outside the range: the range can't be told apart that way.
fn split(
    dataset: &Dataset,
    rows: &[Value],
    from: &str,
    to: &str,
) -> Option<BTreeMap<String, Vec<Value>>> {
    if from == to {
        return Some(BTreeMap::from([(from.to_owned(), rows.to_vec())]));
    }
    let (first, last) = (
        NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?,
        NaiveDate::parse_from_str(to, "%Y-%m-%d").ok()?,
    );
    let days = first.iter_days().take_while(|day| *day <= last);
    if !dataset.station && !rows.iter().any(|row| row["data_date"].is_string()) {
        return Some(days.map(|day| (day.to_string(), rows.to_vec())).collect());
    }
    let mut split: BTreeMap<String, Vec<Value>> =
        days.map(|day| (day.to_string(), Vec::new())).collect();
    for row in rows {
        let day = row["data_date"].as_str()?.get(..10)?;
        split.get_mut(day)?.push(row.clone());
    }
    Some(split)
}

impl Driver {
    pub(crate) async fn collect_daily_performance(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Days, Scope)> {
        request.validate()?;
        // A kept scope whose reads named another station, company or DSP is found again.
        let (scope, found) = self.scope_for(&request.scope_request(), false, run).await?;
        match self.daily_capture(request, &scope, found, run).await {
            Err(error) if mismatched(&error) => {
                let _ = self.known.forget(&request.station);
                if found == Found::Discovered {
                    return Err(error);
                }
                run.metrics.detail("known_scope_mismatched");
                let (scope, found) = self.scope_for(&request.scope_request(), true, run).await?;
                self.daily_capture(request, &scope, found, run).await
            }
            captured => captured,
        }
    }
    async fn daily_capture(
        &mut self,
        request: &Request,
        scope: &Scope,
        found: Found,
        run: &Run<'_>,
    ) -> Result<(Days, Scope)> {
        let started_at = now();
        // A first attempt reads ranges; a retry, which may follow a range that couldn't be
        // split, reads each day on its own.
        let ranges = if run.attempt <= 1 {
            runs(&request.dates)?
        } else {
            request
                .dates
                .iter()
                .map(|d| (d.clone(), d.clone()))
                .collect()
        };
        let reads: Vec<Read> = ranges
            .iter()
            .flat_map(|(from, to)| {
                DATASETS.iter().map(|dataset| Read {
                    dataset,
                    from: from.clone(),
                    to: to.clone(),
                })
            })
            .collect();
        let ranged = ranges.iter().any(|(from, to)| from != to);
        let (company_id, dsp_code, datasets) = self
            .read_performance(scope, found, &request.dsp_abbreviation, &reads, LANES, run)
            .await
            .map_err(|error| {
                // A range too large to read is read a day at a time by the retry.
                if ranged && error.is_any(&[codes::PERFORMANCE_SOURCE_TOO_LARGE]) {
                    run.metrics.detail("daily_range_too_large");
                    Error::new(codes::CORTEX_SOURCE_CHANGED.as_str(), 502)
                } else {
                    error
                }
            })?;
        let mut by_day: BTreeMap<String, Vec<DatasetCapture>> = request
            .dates
            .iter()
            .map(|date| (date.clone(), Vec::new()))
            .collect();
        for (asked, read) in reads.iter().zip(datasets) {
            let split =
                split(asked.dataset, &read.rows, &read.from, &read.to).ok_or_else(|| {
                    run.metrics.detail("daily_range_unsplit");
                    Error::new(codes::CORTEX_SOURCE_CHANGED.as_str(), 502)
                })?;
            for (date, rows) in split {
                if let Some(day) = by_day.get_mut(&date) {
                    day.push(DatasetCapture {
                        id: read.id.clone(),
                        from: date.clone(),
                        to: date,
                        source_url: read.source_url.clone(),
                        rows,
                    });
                }
            }
        }
        let finished_at = now().max(started_at);
        let days = Days {
            days: by_day
                .into_iter()
                .map(|(date, datasets)| Capture {
                    version: 1,
                    collection: Collection::DailyPerformance,
                    date,
                    station: request.station.clone(),
                    company_id: company_id.clone(),
                    dsp_code: dsp_code.clone(),
                    started_at,
                    finished_at,
                    datasets,
                })
                .collect(),
        };
        days.validate_scope(request, scope)?;
        // Checked, the capture needs the browser no more.
        self.browser.close().await;
        Ok((days, scope.clone()))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/daily_performance/collect.rs"]
mod tests;
