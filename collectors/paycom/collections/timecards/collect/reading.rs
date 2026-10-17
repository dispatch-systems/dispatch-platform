//! Reading the roster and each employee's timecard: in the browser tab, rendered; over HTTP,
//! from the response, once `proof` allows; and the lanes that work through the roster.
use super::*;

pub(super) struct Queue<'a, F> {
    pub(super) employees: &'a [Value],
    pub(super) todo: Vec<usize>,
    pub(super) next: AtomicUsize,
    pub(super) stopped: AtomicBool,
    pub(super) progress: tokio::sync::Mutex<(usize, F)>,
    pub(super) origin: &'a str,
    pub(super) period: &'a Value,
    pub(super) metrics: &'a Recorder,
    pub(super) checkpoint: Option<&'a Checkpoint>,
    pub(super) token: &'a str,
    pub(super) direct: Direct,
}
impl<F, Fut> Queue<'_, F>
where
    F: FnMut(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    pub(super) async fn lane(&self, reader: Reader<'_>) -> Result<BTreeMap<String, Vec<Value>>> {
        let result = self.read_queue(reader).await;
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    pub(super) async fn read_queue(
        &self,
        mut reader: Reader<'_>,
    ) -> Result<BTreeMap<String, Vec<Value>>> {
        let mut pages = BTreeMap::new();
        while !self.stopped.load(Ordering::SeqCst) {
            let Some(&index) = self.todo.get(self.next.fetch_add(1, Ordering::SeqCst)) else {
                break;
            };
            let employee = &self.employees[index];
            let (origin, period, metrics) = (self.origin, self.period, self.metrics);
            let records = match &mut reader {
                Reader::Tab(page) => {
                    read_timecard(
                        page,
                        origin,
                        employee,
                        period,
                        metrics,
                        index + 1,
                        &self.direct,
                    )
                    .await?
                }
                Reader::Proving(page, http, proof) => {
                    prove(
                        page,
                        http,
                        proof,
                        origin,
                        employee,
                        period,
                        metrics,
                        index + 1,
                    )
                    .await?
                }
                Reader::Http(http) => {
                    read_http(http, origin, employee, period, metrics, Some(index + 1)).await?
                }
            };
            if let Some(checkpoint) = self.checkpoint {
                checkpoint
                    .save(self.token, employee, self.period, &records)
                    .await?;
            }
            pages.insert(s(employee, "code").to_owned(), records);
            if let Reader::Tab(page) | Reader::Proving(page, ..) = &reader {
                page.collect_garbage().await?;
            }
            let mut progress = self.progress.lock().await;
            progress.0 += 1;
            let done = progress.0;
            (progress.1)(
                20 + (done * 70 / self.employees.len()) as i64,
                format!("Reading timecards ({done} of {})", self.employees.len()),
            )
            .await?;
        }
        Ok(pages)
    }
}

/// How a lane reads each timecard.
pub(super) enum Reader<'r> {
    /// In the browser tab: rendered, or from the response once `Direct` allows.
    Tab(&'r mut Page),
    /// Rendered in the tab and read over HTTP; the rendered read is kept.
    Proving(&'r mut Page, &'r Http, &'r Proof),
    /// Over HTTP only, after the browser has closed.
    Http(&'r Http),
}

/// The roster request the search page made, sent again with the selected period.
pub(super) async fn read_roster(
    http: &Http,
    url: &str,
    headers: &serde_json::Map<String, Value>,
    body: String,
) -> Result<Value> {
    let text = http
        .post(url, headers, body, Duration::from_secs(55))
        .await?;
    let value: Value =
        serde_json::from_str(&text).map_err(|_| Error::new("roster_not_complete", 409))?;
    ensure(value.is_object(), "roster_not_complete", 409)?;
    Ok(value)
}

/// Why a job reads through the browser after all. Fixed labels only.
pub(super) fn fallback(stage: &str, code: &str) {
    dispatch_core::foundation::observability::event(
        "warn",
        "paycom.http_fallback",
        json!({"stage":stage,"error":code}),
    );
}

/// One timecard read over HTTP and extracted here. With an `ordinal` it supplies
/// published data; without one it only checks another read, at the verification
/// address. A page naming another employee fails as a rendered read would; anything
/// else unreadable sends the job to its retry, which reads through the browser.
pub(super) async fn read_http(
    http: &Http,
    origin: &str,
    employee: &Value,
    period: &Value,
    metrics: &Recorder,
    ordinal: Option<usize>,
) -> Result<Vec<Value>> {
    let source = timecard_url(origin, employee, period, ordinal.is_none());
    if let Some(ordinal) = ordinal {
        metrics.page_start(ordinal, 1);
    }
    let unreadable = |label: &str| {
        metrics.detail(label);
        Error::new("provider_response_unreadable", 502)
    };
    let result = async {
        let html = http
            .page(&source, &format!("{origin}{SEARCH}"))
            .await
            .map_err(|refusal| match refusal {
                Refusal::Unavailable => Error::new("provider_unavailable", 502),
                Refusal::Unreadable(label) => unreadable(label),
            })?;
        if let Some(ordinal) = ordinal {
            metrics.page_stage(ordinal, "extraction");
        }
        let code = s(employee, "code").to_owned();
        let period = period.clone();
        let read = tokio::task::spawn_blocking(move || {
            let source = extract::Source {
                employee: &code,
                period: &period,
                url: &source,
            };
            let record = extract::timecard(&html, &source)?;
            project(&record, &code).map_err(|error| {
                extract::Unreadable::Invalid(if error.is(codes::INVALID_TIMECARD_HOURS) {
                    "invalid_timecard_hours"
                } else {
                    "provider_hours_mismatch"
                })
            })
        })
        .await
        .map_err(|_| unreadable("extraction_stopped"))?;
        read.map_err(|reason| match reason {
            extract::Unreadable::WrongEmployee => Error::new("timecard_extraction_failed", 502),
            extract::Unreadable::Invalid(label) => unreadable(label),
        })
    }
    .await;
    if let Some(ordinal) = ordinal {
        metrics.page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
        if result.is_ok() {
            metrics.direct();
        }
    }
    result
}

pub(super) async fn read_timecard(
    page: &mut Page,
    origin: &str,
    employee: &Value,
    period: &Value,
    metrics: &Recorder,
    ordinal: usize,
    direct: &Direct,
) -> Result<Vec<Value>> {
    // The provider's response already holds the whole timecard, so once this job
    // has proven that, read it without rendering. Anything a response cannot fully
    // validate still falls through to a rendered read.
    if direct.enabled() {
        metrics.page_start(ordinal, 1);
        match read_response(page, origin, employee, period, false).await {
            Ok(Some(records)) => {
                metrics.page_stage(ordinal, "extraction");
                metrics.direct();
                metrics.page_finish(ordinal, None);
                if !direct.sample.contains(&ordinal) {
                    return Ok(records);
                }
                let rendered =
                    read_rendered(page, origin, employee, period, metrics, ordinal, true).await?;
                // A punch can land between the two reads; only a response that
                // still disagrees with the rendered page is a provider mismatch.
                let agrees = rendered == records
                    || read_response(page, origin, employee, period, true)
                        .await?
                        .is_some_and(|again| again == rendered);
                ensure(agrees, "provider_response_mismatch", 502)?;
                metrics.spot_checked();
                return Ok(rendered);
            }
            Ok(None) => metrics.page_cancel(ordinal),
            Err(error) => {
                metrics.page_finish(ordinal, Some(&error.code));
                return Err(error);
            }
        }
    }
    let records = read_rendered(page, origin, employee, period, metrics, ordinal, false).await?;
    direct
        .verify(page, origin, employee, period, &records)
        .await?;
    Ok(records)
}

/// A rendered read with its one local retry. A verification read re-reads an
/// employee that already counts as completed, so only its failures are recorded.
pub(super) async fn read_rendered(
    page: &mut Page,
    origin: &str,
    employee: &Value,
    period: &Value,
    metrics: &Recorder,
    ordinal: usize,
    verification: bool,
) -> Result<Vec<Value>> {
    for attempt in 1..=2 {
        metrics.page_start(ordinal, attempt);
        let result = read_once(
            page,
            origin,
            employee,
            period,
            metrics,
            ordinal,
            verification,
        )
        .await;
        if verification && result.is_ok() {
            metrics.page_cancel(ordinal);
        } else {
            metrics.page_finish(
                ordinal,
                result.as_ref().err().map(|error| error.code.as_str()),
            );
        }
        let retry = result
            .as_ref()
            .err()
            .is_some_and(|error| error.is_any(PAGE_RETRY));
        if attempt == 2 || !retry {
            return result;
        }
        // Only this page is reloaded. Authentication, throttling, extraction and
        // validation failures stay fail-closed and use the job policy if allowed.
        page.reset().await?;
        sleep(Duration::from_millis(1000 + (ordinal as u64 * 347 % 1000))).await;
    }
    unreachable!()
}

/// The extractor accepts two forms of a timecard address. The second marks a read
/// that only checks another read of the same employee and publishes nothing new.
/// The provider's own timecard page for one employee and pay period. This is the
/// link retained with a publication; it carries identifiers only, never a session.
pub(crate) fn source_url(origin: &str, employee: &Value, period: &Value) -> String {
    format!(
        "{origin}/v4/cl/web.php/timecard/index?firstrefno={}&perioddates={}&formtype=SUMMARY",
        s(employee, "code"),
        s(period, "key")
    )
}

pub(crate) fn timecard_url(
    origin: &str,
    employee: &Value,
    period: &Value,
    verification: bool,
) -> String {
    format!(
        "{}&dispatch_timecards={}",
        source_url(origin, employee, period),
        if verification { 2 } else { 1 }
    )
}

/// Fetch one timecard inside the authenticated tab and extract it from a detached
/// document: no provider script runs and the HTML never leaves the page. `None`
/// means "use a rendered read" (wrong origin, redirect, unexpected response, or
/// records that fail validation); only throttling and server errors stop the job.
pub(super) async fn read_response(
    page: &Page,
    origin: &str,
    employee: &Value,
    period: &Value,
    verification: bool,
) -> Result<Option<Vec<Value>>> {
    let source = timecard_url(origin, employee, period, verification);
    let frame = page.frame().await?;
    if !s(&frame, "url").starts_with(&format!("{origin}/")) || !page.trusted(s(&frame, "url")) {
        return Ok(None);
    }
    let config = json!({"employeeCode":employee["code"],"period":period,"sourceUrl":source});
    let extractor = include_str!("../../../scripts/timecard.js")
        .trim()
        .trim_end_matches(';');
    let started = page
        .evaluate(&format!(
            r#"(()=>{{globalThis.dispatchTimecard=null;(async()=>{{try{{
            const response=await fetch({source},{{credentials:'include',redirect:'error',cache:'no-store',
                signal:AbortSignal.timeout(30000)}});
            if(response.status===429||response.status>=500)throw 'unavailable';
            if(response.status!==200||!/^text\/html(?:;|$)/i.test(response.headers.get('content-type')||'')||!response.body)throw 'response';
            const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{{fatal:true}});let text='',size=0;
            for(;;){{const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>2097152){{await reader.cancel();throw 'response';}}text+=decoder.decode(part.value,{{stream:true}});}}
            text+=decoder.decode();const document=new DOMParser().parseFromString(text,'text/html'),location={{href:{source}}};
            globalThis.dispatchTimecard={{ok:true,record:({extractor})({config})}};
          }}catch(error){{globalThis.dispatchTimecard={{ok:false,unavailable:error==='unavailable'}};}}}})();return true;}})()"#,
            source = json!(source)
        ))
        .await;
    if started.is_err() {
        return Ok(None);
    }
    let deadline = Instant::now() + Duration::from_secs(35);
    let outcome = loop {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        match page.evaluate("globalThis.dispatchTimecard").await {
            Ok(value) if !value.is_null() => break value,
            Ok(_) => sleep(Duration::from_millis(100)).await,
            Err(_) => return Ok(None),
        }
    };
    let _ = page.evaluate("delete globalThis.dispatchTimecard").await;
    ensure(outcome["unavailable"] != true, "provider_unavailable", 502)?;
    if outcome["ok"] != true {
        return Ok(None);
    }
    Ok(project(&outcome["record"], s(employee, "code")).ok())
}

pub(super) async fn read_once(
    page: &Page,
    origin: &str,
    employee: &Value,
    period: &Value,
    metrics: &Recorder,
    ordinal: usize,
    verification: bool,
) -> Result<Vec<Value>> {
    let source = timecard_url(origin, employee, period, verification);
    page.monitor_loading().await?;
    let previous_loader = page.start_navigation(&source).await?;
    let started = Instant::now();
    let mut content_started = None;
    let mut missing_since = None;
    let mut candidate: Option<(Vec<Value>, Instant)> = None;
    loop {
        page_deadline(started, content_started, Instant::now())?;
        let frame = page.navigation(&previous_loader).await?;
        if frame.is_null() {
            sleep(Duration::from_millis(200)).await;
            continue;
        }
        ensure(
            s(&frame, "url") == "about:blank" || page.trusted(s(&frame, "url")),
            "authentication_failed",
            409,
        )?;
        if s(&frame, "loaderId") != previous_loader && s(&frame, "url") != "about:blank" {
            // A completed redirect away from the requested employee cannot be
            // mistaken for a slowly rendering timecard (including same-origin login).
            ensure(s(&frame, "url") == source, "authentication_failed", 409)?;
            content_started.get_or_insert_with(Instant::now);
            metrics.page_stage(ordinal, "content");
            match page.evaluate("({parsed:document.readyState!=='loading',\
                complete:document.readyState==='complete',present:!!document.querySelector('#tbltimesheet')\
                &&!!document.querySelector('#periodtotals'),login:!document.querySelector('#tbltimesheet')\
                &&Array.from(document.querySelectorAll('input[type=password]'))\
                .some(e=>e.offsetParent!==null&&e.getClientRects().length>0),\
                status:performance.getEntriesByType('navigation')[0]?.responseStatus||0})").await {
                Ok(value) => {
                    let status = value["status"].as_u64().unwrap_or(0);
                    ensure(![401,403].contains(&status) && value["login"] != true, "authentication_failed", 409)?;
                    ensure(status != 429 && status < 500, "provider_unavailable", 502)?;
                    let loading = page.loading(s(&frame,"loaderId")).await?;
                    metrics.page_loading(ordinal, loading["pending"].as_u64().map(|n| n as usize),
                        if value["complete"] == true { "complete" } else if value["parsed"] == true { "interactive" } else { "loading" });
                    if value["complete"] == true && value["present"] == true
                        && (loading["known"] != true || loading["pending"] == 0) { break; }
                    let quiet = loading["known"] == true && loading["failed"] == false
                        && loading["pending"] == 0 && loading["quietMs"].as_u64().unwrap_or(0) >= 750;
                    if value["parsed"] == true && value["present"] == true && quiet {
                        // Validate the actual data, then require it to remain stable
                        // across polls. Images/fonts do not hold up a valid timecard.
                        match extract(page, employee, period, &source).await {
                            Ok(records) => {
                                if let Some((previous, since)) = &candidate
                                    && *previous == records && since.elapsed() >= Duration::from_millis(750) {
                                    metrics.page_stage(ordinal, "extraction");
                                    if value["complete"] != true { metrics.early_ready(); }
                                    return Ok(records);
                                }
                                if candidate.as_ref().is_none_or(|(previous,_)| *previous != records) {
                                    candidate = Some((records, Instant::now()));
                                }
                            }
                            Err(error) if value["complete"] == true => return Err(error),
                            Err(error) if error.is_any(UNSETTLED_PAGE) => {candidate=None;}
                            Err(error) => return Err(error),
                        }
                    } else { candidate = None; }
                    if value["complete"] == true && value["present"] != true {
                        let missing = missing_since.get_or_insert_with(Instant::now);
                        ensure(missing.elapsed() < Duration::from_secs(3), "provider_content_missing", 502)?;
                    } else { missing_since = None; }
                }
                Err(error) if error.is(dispatch_core::Code::BrowserNavigationPending) => (),
                Err(error) => return Err(error),
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    metrics.page_stage(ordinal, "extraction");
    extract(page, employee, period, &source).await
}
