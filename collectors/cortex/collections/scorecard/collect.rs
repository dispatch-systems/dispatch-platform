//! A week's scorecard from Cortex's performance API, over plain HTTP with the
//! browser's session. The API's address, and how it names this DSP, come from the
//! station's last publication; when there is none, or nothing answers there, the
//! overview page's own first data request names them. Every dataset is read at once,
//! and the browser closes as soon as the API has answered one.
use super::*;
use crate::{
    browsers::http::{Http, Refusal},
    db::now,
    job_metrics::Recorder,
    meals::Scope,
    scorecard::{
        Capture, Collection, DATASETS, Dataset, DatasetCapture, MAX_ROWS, POSTED_SIGNAL, Request,
        token,
    },
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// As much as one reply may hold once decompressed. The largest week seen was 2 MB.
const LIMIT: usize = 16 * 1024 * 1024;
/// Reads at once: as many as the overview page itself sends when it opens. Cortex
/// takes over a second to answer even the smallest, so they wait on it together.
const LANES: usize = DATASETS.len();

/// Where the page sends its data requests, and how it names this DSP there.
pub(super) struct Api {
    /// The origin and path up to the version segment.
    pub(super) base: String,
    /// The `dsp` parameter, the DSP's code.
    pub(super) dsp: String,
    /// The `companyId` the page settled on.
    pub(super) company_id: String,
}
impl Api {
    fn address(&self, dataset: &Dataset, station: &str, from: &str, to: &str) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("dataSetId", dataset.id)
            .append_pair("dsp", &self.dsp)
            .append_pair("from", from);
        if let Some(program) = dataset.program {
            query.append_pair("program", program);
        }
        query
            .append_pair("station", station)
            .append_pair("timeFrame", dataset.time_frame.as_str())
            .append_pair("to", to);
        format!("{}/getData?{}", self.base, query.finish())
    }
}
/// The error a refusal is. The page shares the session and the provider, so nothing
/// else can read a dataset the API refused; the job's next attempt asks again.
fn refused(refusal: Refusal, metrics: &Recorder) -> Error {
    match refusal {
        Refusal::Unavailable => Error::new("provider_unavailable", 502),
        Refusal::Unreadable(label) => {
            metrics.detail(label);
            Error::new("scorecard_api_unreadable", 502)
        }
    }
}
/// The rows of a data reply for `dataset`: `tableData.<dataset>.rows`, each a JSON
/// object or a JSON string holding one. A reply without the dataset has no rows.
fn rows(dataset: &str, value: &Value) -> Result<Vec<Value>> {
    let invalid = || Error::new("scorecard_row_invalid", 502);
    let tables = value["tableData"].as_object().ok_or_else(invalid)?;
    let Some(table) = tables.get(dataset) else {
        return Ok(Vec::new());
    };
    let rows = table["rows"].as_array().ok_or_else(invalid)?;
    ensure(rows.len() <= MAX_ROWS, "scorecard_source_too_large", 502)?;
    rows.iter()
        .map(|row| {
            let row = match row {
                Value::String(text) => serde_json::from_str(text).map_err(|_| invalid())?,
                other => other.clone(),
            };
            ensure(row.is_object(), "scorecard_row_invalid", 502)?;
            Ok(row)
        })
        .collect()
}
/// The API a data request names: its base address, the `dsp` code and the station.
fn data_request(url: &str, origin: &str) -> Result<(String, String, String)> {
    let incomplete = || Error::new("cortex_content_incomplete", 502);
    let url = url::Url::parse(url).map_err(|_| incomplete())?;
    ensure(
        url.origin().ascii_serialization() == origin,
        "cortex_scope_mismatch",
        502,
    )?;
    let segments: Vec<&str> = url.path().trim_matches('/').split('/').collect();
    ensure(
        segments.len() == 4
            && segments[..2] == ["performance", "api"]
            && token(segments[2], 32)
            && segments[3] == "getData",
        "cortex_content_incomplete",
        502,
    )?;
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let dsp = query
        .get("dsp")
        .filter(|value| token(value, 32))
        .ok_or_else(incomplete)?;
    Ok((
        format!("{origin}/performance/api/{}", segments[2]),
        dsp.clone(),
        query.get("station").cloned().unwrap_or_default(),
    ))
}
/// Reading every dataset failed: the first error, and whether the API had answered
/// any read at that address.
struct Failed {
    error: Error,
    answered: bool,
}
impl From<Error> for Failed {
    fn from(error: Error) -> Self {
        Self {
            error,
            answered: false,
        }
    }
}

impl Driver {
    /// Fetch.disable already resumes outstanding requests. Drain their queued
    /// notifications without issuing stale continueRequest commands, which retire
    /// the browser session on a protocol error.
    async fn drain_paused(&self) {
        while let Ok(event) = self.browser.event(&self.page.id).await {
            if event.is_null() {
                break;
            }
        }
    }
    /// Opens the overview for the provider identity discovery resolved, then accepts
    /// only a data request and settled page that still name that provider.
    pub(super) async fn performance_api(
        &mut self,
        scope: &Scope,
        expected_dsp: &str,
        metrics: &Recorder,
    ) -> Result<Api> {
        self.page
            .command(
                "Fetch.enable",
                json!({"patterns":[{"urlPattern":"*/performance/api/*getData*","requestStage":"Request"}]}),
            )
            .await?;
        let result = self.observe_api(scope, expected_dsp, metrics).await;
        let _ = self.page.command("Fetch.disable", json!({})).await;
        self.drain_paused().await;
        result
    }
    async fn observe_api(
        &mut self,
        scope: &Scope,
        expected_dsp: &str,
        metrics: &Recorder,
    ) -> Result<Api> {
        let address = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query
                .append_pair("pageId", "dsp_dashboard_overview")
                .append_pair("station", &scope.station)
                .append_pair("companyId", &scope.provider);
            format!("{}/performance?{}", self.origin, query.finish())
        };
        self.page.start_navigation(&address).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut mismatched = false;
        let (base, dsp) = loop {
            ensure(
                Instant::now() < deadline,
                if mismatched {
                    "cortex_scope_mismatch"
                } else {
                    "cortex_content_incomplete"
                },
                502,
            )?;
            let event = self.browser.event(&self.page.id).await?;
            if event.is_null() {
                continue;
            }
            // Every paused request goes on, watched or not: the page must keep loading.
            let _ = self
                .page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
            let url = s(&event["request"], "url");
            if event["request"]["method"] != "GET" || !url.contains("/getData") {
                continue;
            }
            let (base, dsp, station) = data_request(url, &self.origin)?;
            if station == scope.station && dsp.eq_ignore_ascii_case(expected_dsp) {
                break (base, dsp);
            }
            mismatched = true;
            metrics.detail(if station != scope.station {
                "station_mismatch"
            } else {
                "provider_mismatch"
            });
        };
        // Stop intercepting before waiting on the page: nothing else must be held up.
        self.page.command("Fetch.disable", json!({})).await?;
        self.drain_paused().await;
        // The page must settle on the exact provider discovery selected.
        let settled = Instant::now() + Duration::from_secs(15);
        loop {
            let frame = self.page.frame().await?;
            if let Ok(url) = url::Url::parse(s(&frame, "url"))
                && url.origin().ascii_serialization() == self.origin
            {
                let values: std::collections::HashMap<_, _> =
                    url.query_pairs().into_owned().collect();
                if let (Some(company), Some(station)) =
                    (values.get("companyId"), values.get("station"))
                    && token(company, 128)
                {
                    ensure(
                        company == &scope.provider && station == &scope.station,
                        "cortex_scope_mismatch",
                        502,
                    )?;
                    break;
                }
            }
            ensure(Instant::now() < settled, "cortex_content_incomplete", 502)?;
            sleep(Duration::from_millis(300)).await;
        }
        Ok(Api {
            base,
            dsp,
            company_id: scope.provider.clone(),
        })
    }
    /// The API where the station's last publication read it, if that still names this
    /// origin and station.
    async fn saved_api(
        &self,
        scope: &Scope,
        expected_dsp: &str,
        run: &Run<'_>,
    ) -> Result<Option<Api>> {
        let (job, at) = (run.job.to_owned(), scope.station.clone());
        let saved = run
            .state
            .read(move |db| {
                let dsp = db.job_row(&job, None)?.dsp_id;
                db.scorecard_address(&dsp, &at)
            })
            .await?;
        Ok(saved.and_then(|(url, company_id)| {
            let (base, dsp, named) = data_request(&url, &self.origin).ok()?;
            (named == scope.station
                && company_id == scope.provider
                && dsp.eq_ignore_ascii_case(expected_dsp))
            .then_some(Api {
                base,
                dsp,
                company_id,
            })
        }))
    }
    /// Every dataset at once, answering each one's address and rows in `DATASETS`
    /// order. The first answer shows the session and the address work, and the
    /// browser closes then: nothing after it needs a page.
    async fn read_all(
        &self,
        api: &Api,
        request: &Request,
        run: &Run<'_>,
    ) -> std::result::Result<Vec<(String, Vec<Value>)>, Failed> {
        let http = Http::signed_in(&self.browser, &self.origin).await?;
        let referer = format!("{}/performance", self.origin);
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let answered = AtomicBool::new(false);
        let reporting = AtomicBool::new(false);
        let lanes = futures_util::future::join_all((0..LANES).map(|_| async {
            let mut read = Vec::new();
            loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(dataset) = DATASETS.get(index) else {
                    break;
                };
                let (from, to) = request.interval(dataset)?;
                let url = api.address(dataset, &request.station, &from, &to);
                let value = http
                    .json(&url, &referer, LIMIT)
                    .await
                    .map_err(|refusal| refused(refusal, run.metrics))?;
                let rows = rows(dataset.id, &value)?;
                if !answered.swap(true, Ordering::SeqCst) {
                    self.browser.close().await;
                }
                done.fetch_add(1, Ordering::SeqCst);
                // Each progress update is a database write, and the datasets finish
                // together: one write at a time shows the count, where one per lane would
                // hold every database slot the platform answers requests with.
                if !reporting.swap(true, Ordering::SeqCst) {
                    let finished = done.load(Ordering::SeqCst);
                    let written = run
                        .progress(
                            20 + (70 * finished / DATASETS.len()) as i64,
                            format!("Reading scorecard datasets ({finished}/{})", DATASETS.len()),
                        )
                        .await;
                    reporting.store(false, Ordering::SeqCst);
                    written?;
                }
                read.push((index, url, rows));
            }
            Ok::<_, Error>(read)
        }))
        .await;
        let mut replies: Vec<Option<(String, Vec<Value>)>> =
            DATASETS.iter().map(|_| None).collect();
        for lane in lanes {
            let lane = lane.map_err(|error| Failed {
                error,
                answered: answered.load(Ordering::SeqCst),
            })?;
            for (index, url, rows) in lane {
                replies[index] = Some((url, rows));
            }
        }
        Ok(replies
            .into_iter()
            .map(|reply| reply.ok_or_else(|| Error::new("scorecard_capture_invalid", 502)))
            .collect::<Result<Vec<_>>>()?)
    }
    pub(super) async fn collect_scorecard(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<(Capture, Scope)> {
        request.validate()?;
        let started_at = now();
        run.progress(10, "Finding the scorecard".into()).await?;
        let scope = self
            .resolve_scope(&request.scope_request(), run.metrics)
            .await?;
        let (mut api, mut saved) = match self
            .saved_api(&scope, &request.dsp_abbreviation, run)
            .await?
        {
            Some(api) => (api, true),
            None => (
                self.performance_api(&scope, &request.dsp_abbreviation, run.metrics)
                    .await?,
                false,
            ),
        };
        run.progress(20, "Reading scorecard datasets".into())
            .await?;
        let replies = loop {
            match self.read_all(&api, request, run).await {
                Ok(replies) => break replies,
                // Nothing answered where the last publication read: Cortex moved its
                // API, and the page names it again.
                Err(failed)
                    if saved
                        && !failed.answered
                        && failed.error.is(crate::Code::ScorecardApiUnreadable) =>
                {
                    api = self
                        .performance_api(&scope, &request.dsp_abbreviation, run.metrics)
                        .await?;
                    saved = false;
                }
                Err(failed) => return Err(failed.error),
            }
        };
        let datasets = DATASETS
            .iter()
            .zip(replies)
            .map(|(dataset, (source_url, rows))| {
                let (from, to) = request.interval(dataset)?;
                Ok(DatasetCapture {
                    id: dataset.id.into(),
                    from,
                    to,
                    source_url,
                    rows,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let posted = datasets
            .iter()
            .find(|d| d.id == POSTED_SIGNAL)
            .is_some_and(|d| !d.rows.is_empty());
        run.progress(95, "Validating scorecard".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Scorecard,
            week: request.week.clone(),
            station: request.station.clone(),
            company_id: api.company_id,
            dsp_code: api.dsp,
            started_at,
            finished_at: now().max(started_at),
            posted,
            datasets,
        };
        capture.validate_scope(request, &scope)?;
        Ok((capture, scope))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_come_as_objects_or_json_strings_for_the_named_dataset() {
        let value = json!({"tableData":{"other":{"rows":[{"week":1}]},
            "dsp_weekly_cdf":{"rows":["{\"week\":38}",{"week":39}]}}});
        assert_eq!(
            rows("dsp_weekly_cdf", &value).unwrap(),
            vec![json!({"week":38}), json!({"week":39})]
        );
        assert!(
            rows("dsp_weekly_cdf", &json!({"tableData":{}}))
                .unwrap()
                .is_empty()
        );
        assert!(rows("x", &json!({"tableData":{"x":{"rows":["not json"]}}})).is_err());
        assert!(rows("x", &json!({"tableData":{"x":{"rows":[1]}}})).is_err());
        assert!(rows("x", &json!({"rows":[]})).is_err());
    }
    #[test]
    fn data_requests_name_the_api_and_this_dsp_on_the_pages_origin_only() {
        let origin = "https://logistics.amazon.com";
        let (base, dsp, station) = data_request(
            "https://logistics.amazon.com/performance/api/v1/getData?dataSetId=x&dsp=NLOG&station=TST1",
            origin,
        )
        .unwrap();
        assert_eq!(base, "https://logistics.amazon.com/performance/api/v1");
        assert_eq!((dsp.as_str(), station.as_str()), ("NLOG", "TST1"));
        assert!(
            data_request(
                "https://evil.example/performance/api/v1/getData?dsp=NLOG",
                origin
            )
            .is_err()
        );
        assert!(
            data_request(
                "https://logistics.amazon.com/other/api/v1/getData?dsp=NLOG",
                origin
            )
            .is_err()
        );
        assert!(
            data_request(
                "https://logistics.amazon.com/performance/api/v1/getData",
                origin
            )
            .is_err()
        );
    }
    #[test]
    fn addresses_follow_the_pages_parameter_order() {
        let api = Api {
            base: "https://logistics.amazon.com/performance/api/v1".into(),
            dsp: "NLOG".into(),
            company_id: "company".into(),
        };
        let dataset = crate::scorecard::dataset("da_dsp_station_weekly_performance").unwrap();
        assert_eq!(
            api.address(dataset, "TST1", "2026-W38", "2026-W38"),
            concat!(
                "https://logistics.amazon.com/performance/api/v1/getData?dataSetId=",
                "da_dsp_station_weekly_performance&dsp=NLOG&from=2026-W38&program=AMZL",
                "&station=TST1&timeFrame=Weekly&to=2026-W38"
            )
        );
    }
}
