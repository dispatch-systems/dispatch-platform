//! Collects one published scorecard week through the shared performance transport.
use super::capture::{Capture, Collection, DATASETS, POSTED_SIGNAL, Request};
use crate::{
    connection::{
        Driver,
        performance::{Found, mismatched},
    },
    discovery::Scope,
    performance::Read,
};
use dispatch_core::{Result, collection::browser::Run, db::now};

/// Reads at once: every dataset of the week together. Cortex answered all 14 at once
/// without throttling, in under half the time six at once took.
const LANES: usize = 14;

impl Driver {
    pub(crate) async fn collect_weekly_scorecard(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        // A kept scope whose reads named another station, company or DSP is found again.
        let (scope, found) = self.scope_for(&request.scope_request(), false, run).await?;
        match self.weekly_capture(request, &scope, found, run).await {
            Err(error) if mismatched(&error) => {
                let _ = self.known.forget(&request.station);
                if found == Found::Discovered {
                    return Err(error);
                }
                run.metrics.detail("known_scope_mismatched");
                let (scope, found) = self.scope_for(&request.scope_request(), true, run).await?;
                self.weekly_capture(request, &scope, found, run).await
            }
            captured => captured,
        }
    }
    async fn weekly_capture(
        &mut self,
        request: &Request,
        scope: &Scope,
        found: Found,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        let started_at = now();
        let reads = DATASETS
            .iter()
            .map(|dataset| {
                let (from, to) = request.interval(dataset)?;
                Ok(Read { dataset, from, to })
            })
            .collect::<Result<Vec<_>>>()?;
        let (company_id, dsp_code, datasets) = self
            .read_performance(scope, found, &request.dsp_abbreviation, &reads, LANES, run)
            .await?;
        let posted = datasets
            .iter()
            .find(|d| d.id == POSTED_SIGNAL)
            .is_some_and(|d| !d.rows.is_empty());
        let capture = Capture {
            version: 1,
            collection: Collection::WeeklyScorecard,
            week: request.week.clone(),
            station: request.station.clone(),
            company_id,
            dsp_code,
            started_at,
            finished_at: now().max(started_at),
            posted,
            datasets,
        };
        capture.validate_scope(request, scope)?;
        // Checked, the capture needs the browser no more.
        self.browser.close().await;
        Ok((capture, scope.clone()))
    }
}
