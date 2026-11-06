use super::{
    Checkpoint, EmployeeTimecardPeriod, extract, projection,
    roster::{Roster, with_known_employees},
};
use crate::{codes, connection::Driver};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{
            Run,
            page::{Page, call},
        },
        metrics::Recorder,
    },
    db::{self, s},
    ensure,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};
use tokio::time::Instant;

mod proof;
mod reading;

use proof::{Direct, Proof};
#[cfg(not(feature = "operator-probes"))]
use reading::source_url;
use reading::{Queue, Reader, read_timecard};
// The operator probes measure a live collection at the addresses the lanes read.
#[cfg(feature = "operator-probes")]
pub(crate) use reading::{source_url, timecard_url};
// A timecard page that is still rendering reads as one of these; wait for it to settle.
const UNSETTLED_PAGE: &[dispatch_core::Code] = &[
    codes::TIMECARD_EXTRACTION_FAILED,
    codes::INVALID_TIMECARD_HOURS,
    dispatch_core::Code::ProviderHoursMismatch,
    dispatch_core::Code::BrowserNavigationPending,
];
// A page read that failed with one of these is read once more.
const PAGE_RETRY: &[dispatch_core::Code] = &[
    dispatch_core::Code::ProviderNavigationTimeout,
    dispatch_core::Code::ProviderContentTimeout,
    dispatch_core::Code::ProviderContentMissing,
    dispatch_core::Code::BrowserNavigationPending,
];
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(45);
const CONTENT_TIMEOUT: Duration = Duration::from_secs(30);
fn page_deadline(started: Instant, content: Option<Instant>, now: Instant) -> Result<()> {
    if let Some(content) = content {
        ensure(
            now < content + CONTENT_TIMEOUT,
            "provider_content_timeout",
            504,
        )
    } else {
        ensure(
            now < started + NAVIGATION_TIMEOUT,
            "provider_navigation_timeout",
            504,
        )
    }
}
/// Bounded parallel HTTP timecard reads after rendered parity is proven.
/// Compare concurrency with the `http_concurrency` operator probe.
const HTTP_LANES: usize = 6;

pub(crate) enum Selection<'a> {
    Date(Option<NaiveDate>),
    Period(&'a EmployeeTimecardPeriod, &'a [Value]),
}

/// Why a job reads through the browser after all. Fixed labels only.
pub(super) fn fallback(stage: &str, code: &str) {
    dispatch_core::foundation::observability::event(
        "warn",
        "paycom.http_fallback",
        json!({"stage":stage,"error":code}),
    );
}

impl Driver {
    pub(crate) async fn collect_employee(
        &mut self,
        run: &Run<'_>,
        employee: &Value,
        requested: &crate::timecards::EmployeeTimecardPeriod,
    ) -> Result<Value> {
        self.credentials = Value::Null;
        self.assistance = None;
        let period = requested.provider_period()?;
        run.progress(20, format!("Reading timecard for {}", s(employee, "code")))
            .await?;
        self.new_page().await?;
        let records = read_timecard(
            &mut self.page,
            &self.origin,
            employee,
            &period,
            run.metrics,
            0,
            &Direct::new(&[0]),
        )
        .await?;
        Ok(json!({"employees":[employee],"timecards":records,
            "sources":[{"employeeCode":employee["code"],"periodKey":period["key"],"url":source_url(&self.origin,employee,&period)}],
            "from":requested.from,"to":requested.to,"collectedAt":db::iso()}))
    }
    pub(crate) async fn collect<F, Fut>(
        &mut self,
        timezone: &str,
        selection: Selection<'_>,
        metrics: &Recorder,
        checkpoint: Option<&Checkpoint>,
        mut progress: F,
        http: bool,
    ) -> Result<Value>
    where
        F: FnMut(i64, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        self.credentials = Value::Null;
        self.assistance = None;
        progress(10, "Reading employee roster".into()).await?;
        let selected_date = match &selection {
            Selection::Date(date) => *date,
            Selection::Period(period, _) => Some(period.start()?),
        };
        let Roster {
            raw,
            period,
            employees,
            http: reader,
        } = self.roster(timezone, selected_date, http).await?;
        let employees = match selection {
            Selection::Date(_) => employees,
            Selection::Period(requested, known) => {
                ensure(
                    period["start"] == requested.from && period["end"] == requested.to,
                    "timecard_identity_mismatch",
                    502,
                )?;
                with_known_employees(employees, known, &period)?
            }
        };
        let resume = if let Some(checkpoint) = checkpoint {
            Some(checkpoint.prepare(&period, &employees, timezone).await?)
        } else {
            None
        };
        let (token, mut pages) = resume.map(|r| (r.token, r.pages)).unwrap_or_default();
        metrics.resumed(pages.len());
        let mut todo = employees
            .iter()
            .enumerate()
            .filter_map(|(index, employee)| {
                (!pages.contains_key(s(employee, "code"))).then_some(index)
            })
            .collect::<Vec<_>>();
        let mut second = if todo.len() > 1 {
            Some(Page::open(self.browser.clone(), self.origin.clone()).await?)
        } else {
            None
        };
        let origin = self.origin.clone();
        let queue = |todo, direct, progress| Queue {
            employees: &employees,
            todo,
            next: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
            progress: tokio::sync::Mutex::new(progress),
            origin: &origin,
            period: &period,
            metrics,
            checkpoint,
            token: &token,
            direct,
        };
        let mut direct = Direct::new(&todo);
        let mut progress = (pages.len(), progress);
        // Render a few employees while reading the same responses here. Only when
        // they all agree does the browser close and the rest come over HTTP.
        let reader = match reader.filter(|_| todo.len() >= Direct::SAMPLED_ROSTER) {
            Some(http) => {
                let sample = Proof::sample(&raw, &employees, &todo);
                let proof = Proof::default();
                let proving = queue(sample.clone(), Direct::disabled(), progress);
                let (first, others) = tokio::join!(
                    proving.lane(Reader::Proving(&mut self.page, &http, &proof)),
                    async {
                        match &mut second {
                            Some(page) => proving.lane(Reader::Proving(page, &http, &proof)).await,
                            None => Ok(BTreeMap::new()),
                        }
                    }
                );
                progress = proving.progress.into_inner();
                pages.extend(first?);
                pages.extend(others?);
                todo.retain(|index| !sample.contains(index));
                if proof.proven() {
                    Some(http)
                } else {
                    // As when the first comparison differs: nothing from a response.
                    fallback("comparison", "provider_response_mismatch");
                    direct = Direct::disabled();
                    None
                }
            }
            None => None,
        };
        let queue = queue(todo, direct, progress);
        // Drain every lane even when one fails. Dropping a sibling's in-flight
        // CDP command intentionally closes the shared browser transport.
        let lanes = match &reader {
            Some(http) => {
                drop(second);
                self.browser.close().await;
                futures_util::future::join_all(
                    (0..HTTP_LANES).map(|_| queue.lane(Reader::Http(http))),
                )
                .await
            }
            None => {
                let (first, others) =
                    tokio::join!(queue.lane(Reader::Tab(&mut self.page)), async {
                        match &mut second {
                            Some(page) => queue.lane(Reader::Tab(page)).await,
                            None => Ok(BTreeMap::new()),
                        }
                    });
                vec![first, others]
            }
        };
        for lane in lanes {
            pages.extend(lane?);
        }
        let timecards = employees
            .iter()
            .flat_map(|employee| pages.remove(s(employee, "code")).unwrap_or_default())
            .collect::<Vec<_>>();
        ensure(
            timecards.len() == employees.len() * 14,
            "roster_not_complete",
            409,
        )?;
        let sources = employees
            .iter()
            .map(|employee| {
                json!({"employeeCode":employee["code"],"periodKey":period["key"],
                "url":source_url(&self.origin, employee, &period)})
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"employees":employees,"timecards":timecards,"sources":sources,"from":period["start"],
                "to":period["end"],"collectedAt":db::iso()}),
        )
    }
}

async fn extract(
    page: &Page,
    employee: &Value,
    period: &Value,
    source: &str,
) -> Result<Vec<Value>> {
    let config = json!({"employeeCode":employee["code"],"period":period,"sourceUrl":source});
    let record = page
        .evaluate(&call(include_str!("../../scripts/timecard.js"), &config))
        .await
        .map_err(|error| {
            if error.is(dispatch_core::Code::BrowserScriptFailed) {
                Error::new("timecard_extraction_failed", 502)
            } else {
                error
            }
        })?;
    projection::project(&record, s(employee, "code"))
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/collect.rs"]
mod tests;
