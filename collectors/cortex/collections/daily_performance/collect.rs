//! One date's datasets over the shared authenticated performance transport.
use super::{Capture, Collection, DATASETS, Request};
use crate::{connection::Driver, discovery::Scope, performance::Read};
use dispatch_core::{Result, collection::browser::Run, db::now};
impl Driver {
    pub(crate) async fn collect_daily_performance(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        let started_at = now();
        let reads: Vec<Read> = DATASETS
            .iter()
            .map(|dataset| Read {
                dataset,
                from: request.date.clone(),
                to: request.date.clone(),
            })
            .collect();
        let (company_id, dsp_code, datasets, scope) = self
            .read_performance(
                &request.scope_request(),
                &request.dsp_abbreviation,
                &reads,
                run,
            )
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
        capture.validate_scope(request, &scope)?;
        Ok((capture, scope))
    }
}
