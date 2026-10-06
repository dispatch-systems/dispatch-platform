//! Collects one published scorecard week through the shared performance transport.
use super::capture::{Capture, Collection, DATASETS, POSTED_SIGNAL, Request};
use crate::{connection::Driver, discovery::Scope, performance::Read};
use dispatch_core::{Result, collection::browser::Run, db::now};

impl Driver {
    pub(crate) async fn collect_weekly_scorecard(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        let started_at = now();
        let reads = DATASETS
            .iter()
            .map(|dataset| {
                let (from, to) = request.interval(dataset)?;
                Ok(Read { dataset, from, to })
            })
            .collect::<Result<Vec<_>>>()?;
        let (company_id, dsp_code, datasets, scope) = self
            .read_performance(
                &request.scope_request(),
                &request.dsp_abbreviation,
                &reads,
                run,
            )
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
        capture.validate_scope(request, &scope)?;
        Ok((capture, scope))
    }
}
