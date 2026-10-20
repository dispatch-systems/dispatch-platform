//! One date's datasets over the shared authenticated performance transport.
use super::{Capture, Collection, DATASETS, Request};
use crate::{
    connection::{
        Driver,
        performance::{Found, mismatched},
    },
    discovery::Scope,
    performance::Read,
};
use dispatch_core::{Result, collection::browser::Run, db::now};

/// Reads at once: every dataset of the day together. Cortex answered all 22 at once
/// without throttling, in about a fifth of the time six at once took.
const LANES: usize = 22;

impl Driver {
    pub(crate) async fn collect_daily_performance(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        // A kept scope whose reads named another station, company or DSP is found again.
        let (scope, found) = self.scope_for(&request.scope_request(), false, run).await?;
        match self.daily_capture(request, &scope, run).await {
            Err(error) if found == Found::Known && mismatched(&error) => {
                run.metrics.detail("known_scope_mismatched");
                let _ = self.known.forget(&request.station);
                let (scope, _) = self.scope_for(&request.scope_request(), true, run).await?;
                self.daily_capture(request, &scope, run).await
            }
            captured => captured,
        }
    }
    async fn daily_capture(
        &mut self,
        request: &Request,
        scope: &Scope,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        let started_at = now();
        let reads: Vec<Read> = DATASETS
            .iter()
            .map(|dataset| Read {
                dataset,
                from: request.date.clone(),
                to: request.date.clone(),
            })
            .collect();
        let (company_id, dsp_code, datasets) = self
            .read_performance(scope, &request.dsp_abbreviation, &reads, LANES, run)
            .await?;
        let capture = Capture {
            version: 1,
            collection: Collection::DailyPerformance,
            date: request.date.clone(),
            station: request.station.clone(),
            company_id,
            dsp_code,
            started_at,
            finished_at: now().max(started_at),
            datasets,
        };
        capture.validate_scope(request, scope)?;
        Ok((capture, scope.clone()))
    }
}
