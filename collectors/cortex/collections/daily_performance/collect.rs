//! The days a collection was asked for over the shared authenticated performance transport.
//! Each run of up to a week of consecutive days is one read per dataset, its rows given back
//! to their days; once an attempt met a run it couldn't read whole, the job's next attempts
//! read each day on its own as a job of one day always did.
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
use dispatch_core::{
    Error, Result,
    collection::{browser::Run, metrics::Metrics},
    db::now,
};
use jiff::{ToSpan, civil::Date};
use serde_json::Value;
use std::collections::BTreeMap;

/// Bounded dataset reads across runs of days. Every sibling read drains before its
/// reply is released. Inspect dataset requests with the `probe_scorecard_api` operator probe.
const LANES: usize = 22;

/// The most days one range reads: a week, as read side by side with each day's own reads.
const RANGE_DAYS: usize = 7;
/// What an attempt's metrics name when a range it read was too large, or couldn't be told
/// apart into its days.
const RANGE_TOO_LARGE: &str = "daily_range_too_large";
const RANGE_UNSPLIT: &str = "daily_range_unsplit";

/// The request's days as runs of consecutive days of at most a week, first and last of each.
fn runs(dates: &[String]) -> Result<Vec<(String, String)>> {
    let mut runs: Vec<(String, String, Date, usize)> = Vec::new();
    for text in dates {
        let date = dispatch_core::foundation::clock::date(text)?;
        match runs.last_mut() {
            Some((_, last, previous, days))
                if previous.tomorrow().ok() == Some(date) && *days < RANGE_DAYS =>
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
/// Whether one of the job's attempts before `attempt`, as `metrics` has them, met a range it
/// couldn't read whole: the range would meet it again, where each day on its own reads.
fn range_failed_before(metrics: &[Metrics], attempt: i64) -> bool {
    metrics.iter().any(|earlier| {
        earlier.attempt < attempt
            && earlier
                .detail
                .as_deref()
                .is_some_and(|detail| [RANGE_TOO_LARGE, RANGE_UNSPLIT].contains(&detail))
    })
}
/// Whether an earlier attempt of the job met a range it couldn't read whole.
async fn range_failed(run: &Run<'_>) -> Result<bool> {
    if run.attempt <= 1 {
        return Ok(false);
    }
    let job = run.job.to_owned();
    let metrics = run.state.read(move |db| db.metrics(&job)).await?;
    Ok(range_failed_before(&metrics, run.attempt))
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
        dispatch_core::foundation::clock::date(from).ok()?,
        dispatch_core::foundation::clock::date(to).ok()?,
    );
    let days = first.series(1.day()).take_while(|day| *day <= last);
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
        // Ranges, unless an earlier attempt met one it couldn't read whole.
        let ranges = if range_failed(run).await? {
            request
                .dates
                .iter()
                .map(|d| (d.clone(), d.clone()))
                .collect()
        } else {
            runs(&request.dates)?
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
                // A range too large to read is read a day at a time by the retries.
                if ranged && error.is_any(&[codes::PERFORMANCE_SOURCE_TOO_LARGE]) {
                    run.metrics.detail(RANGE_TOO_LARGE);
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
                    run.metrics.detail(RANGE_UNSPLIT);
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
        self.close_browser().await;
        Ok((days, scope.clone()))
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/daily_performance/collect.rs"]
mod tests;
