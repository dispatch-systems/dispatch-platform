use super::*;
use crate::{
    codes,
    connection::{Driver, SEARCH},
};
use chrono::{Datelike, NaiveDate};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{
            Run,
            http::{Http, Refusal},
            page::{Page, call},
        },
        metrics::Recorder,
    },
    db::{self, s},
    ensure,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    future::Future,
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    time::Duration,
};
use tokio::time::{Instant, sleep};

mod proof;
mod reading;

use proof::*;
use reading::*;
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
const API: &str = "https://time-and-attendance.paycomonline.net/api/cl/timecard-search/employees";
/// Timecards read over HTTP at once. Paycom answered six together as fast as two
/// (3.6 s against 3.5 s each for 100 employees) and refused none.
const HTTP_LANES: usize = 6;

pub(crate) enum Selection<'a> {
    Date(Option<NaiveDate>),
    Period(&'a EmployeeTimecardPeriod, &'a [Value]),
}

/// The current source roster wins for listed people. Known DSP-local people omitted
/// from its active-only search still receive their individual timecard reads.
fn with_known_employees(
    mut roster: Vec<Value>,
    known: &[Value],
    period: &Value,
) -> Result<Vec<Value>> {
    let mut seen = roster
        .iter()
        .map(|employee| s(employee, "code").to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    for employee in known {
        if seen.insert(s(employee, "code").to_ascii_uppercase()) {
            let mut employee = employee.clone();
            employee["active"] = json!(false);
            roster.push(employee);
        }
    }
    validate_workforce(
        &json!({"employees":roster,"timecards":[],"from":period["start"],"to":period["end"],"collectedAt":db::iso()}),
    )?;
    Ok(roster)
}
const FIELDS: &[&str] = &[
    "allocationCategories",
    "approvalMode",
    "eeCodes",
    "endDate",
    "getCount",
    "highlighting",
    "isAdvancedFilterApplied",
    "loadTotals",
    "minWageUrl",
    "onlyBorrowedEmployees",
    "payClassCodes",
    "q",
    "selectedColumns",
    "selectedEarnings",
    "skip",
    "sortParams",
    "startDate",
    "take",
];
fn valid(ok: bool) -> Result<()> {
    ensure(ok, "roster_not_complete", 409)
}
fn codes(value: &Value) -> Result<BTreeSet<String>> {
    let values = value
        .as_array()
        .ok_or_else(|| Error::new("roster_not_complete", 409))?;
    valid(!values.is_empty() && values.len() <= 5000)?;
    let mut codes = BTreeSet::new();
    for value in values {
        let code = value.as_str().unwrap_or("");
        valid(
            code.len() == 4
                && code.bytes().all(|b| b.is_ascii_alphanumeric())
                && codes.insert(code.to_ascii_uppercase()),
        )?;
    }
    Ok(codes)
}
fn date(value: &str) -> Result<NaiveDate> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| Error::new("invalid_period", 409))?;
    ensure(parsed.to_string() == value, "invalid_period", 409)?;
    Ok(parsed)
}
fn selected_body(body: &Value, today: NaiveDate) -> Result<(Value, Value, BTreeSet<String>)> {
    let object = body
        .as_object()
        .ok_or_else(|| Error::new("roster_not_complete", 409))?;
    valid(object.len() == FIELDS.len() && FIELDS.iter().all(|key| object.contains_key(*key)))?;
    let codes = codes(&body["eeCodes"])?;
    valid(
        body["isAdvancedFilterApplied"].is_boolean()
            && (body["q"].is_null() || body["q"] == "")
            && body["onlyBorrowedEmployees"] == false
            && (body["skip"].is_null() || body["skip"] == 0)
            && (body["take"].is_null()
                || body["take"]
                    .as_u64()
                    .is_some_and(|n| n >= codes.len() as u64))
            && (body["getCount"].is_null() || body["getCount"] == true),
    )?;
    let start = date(s(body, "startDate"))?;
    let end = date(s(body, "endDate"))?;
    ensure(
        start.weekday() == chrono::Weekday::Sun
            && end.weekday() == chrono::Weekday::Sat
            && (end - start).num_days() == 13,
        "invalid_period",
        409,
    )?;
    let offset = (today - start).num_days().div_euclid(14) * 14;
    let start = start
        .checked_add_signed(chrono::Duration::days(offset))
        .ok_or_else(|| Error::new("invalid_period", 409))?;
    let dates = (0..14)
        .map(|i| (start + chrono::Duration::days(i)).to_string())
        .collect::<Vec<_>>();
    let period = json!({"start":dates[0],"end":dates[13],"key":format!("{}_{}",dates[0],dates[13]),"dates":dates});
    let mut selected = body.clone();
    selected["startDate"] = period["start"].clone();
    selected["endDate"] = period["end"].clone();
    selected["isAdvancedFilterApplied"] = json!(false);
    selected["selectedEarnings"] = json!([]);
    selected["approvalMode"] = Value::Null;
    Ok((selected, period, codes))
}
fn employees(raw: &Value, expected: &BTreeSet<String>) -> Result<Vec<Value>> {
    valid(codes(&raw["eeCodes"])? == *expected)?;
    let rows = raw["employees"]
        .as_array()
        .ok_or_else(|| Error::new("roster_not_complete", 409))?;
    valid(rows.len() == expected.len())?;
    let mut seen = BTreeSet::new();
    let mut employees = Vec::new();
    for row in rows {
        let code = s(row, "employeeCode");
        valid(
            expected.contains(&code.to_ascii_uppercase())
                && seen.insert(code.to_ascii_uppercase())
                && !s(row, "fullName").trim().is_empty()
                && row["eestatus"] == "A",
        )?;
        let selections = row["allocation"]["selections"]
            .as_array()
            .ok_or_else(|| Error::new("roster_not_complete", 409))?;
        valid(selections.len() == 2)?;
        let department = selections
            .iter()
            .find(|v| v["categoryName"] == "Department")
            .ok_or_else(|| Error::new("roster_not_complete", 409))?;
        let station = selections
            .iter()
            .find(|v| v["categoryName"] == "Delivery Station Code")
            .ok_or_else(|| Error::new("roster_not_complete", 409))?;
        valid(department["isDepartment"] == true && station["isDepartment"] == false)?;
        for item in [department, station] {
            valid(item["code"].is_string() && item["description"].is_string())?;
        }
        for key in [
            "position",
            "payClassCode",
            "terminalCode",
            "payType",
            "primarySupervisor",
        ] {
            valid(row[key].is_string())?;
        }
        valid(row["missingPunches"].is_i64())?;
        for number in [
            &row["totals"]["totalHours"],
            &row["totals"]["otHours"],
            &row["approvalPercentages"]["employee"],
            &row["approvalPercentages"]["supervisor"],
        ] {
            valid(number.as_f64().is_some_and(f64::is_finite))?;
        }
        employees.push(
            json!({"code":code,"name":row["fullName"],"department":department["description"],
            "position":row["position"],"station":station["code"],"active":true}),
        );
    }
    Ok(employees)
}
fn hours(value: &Value) -> Option<f64> {
    value["totalHours"]
        .as_f64()
        .or_else(|| value["hours"].as_f64())
}
pub(crate) fn project(record: &Value, employee: &str) -> Result<Vec<Value>> {
    let error = || Error::new("invalid_timecard_hours", 409);
    let days = record["days"].as_array().ok_or_else(error)?;
    let additional = record["additionalRows"].as_array().ok_or_else(error)?;
    ensure(
        days.len() == 14 && additional.len() <= 200,
        "invalid_timecard_hours",
        409,
    )?;
    let base = days
        .iter()
        .map(|day| {
            hours(day)
                .or_else(|| {
                    // DOM extraction folds all pay-code punches into the day.
                    // Only row zero determines whether the leading row is empty;
                    // additional rows contribute their own reported hours below.
                    let no_missing_leading = day["missingPunch"] == false
                        || day["unresolvedSlots"].as_array().is_some_and(|slots| {
                            !slots.is_empty()
                                && slots.iter().all(|slot| {
                                    slot.as_str().is_some_and(|slot| slot.contains(':'))
                                })
                        });
                    if no_missing_leading
                        && day["punches"].as_array().is_some_and(|punches| {
                            punches
                                .iter()
                                .all(|punch| punch["rowIndex"].as_u64().is_some_and(|row| row > 0))
                        })
                    {
                        Some(0.)
                    } else {
                        None
                    }
                })
                .ok_or_else(error)
        })
        .collect::<Result<Vec<_>>>()?;
    let rows = days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            base[index]
                + additional
                    .iter()
                    .filter(|r| r["date"] == day["date"])
                    .map(|r| hours(r).unwrap_or(0.))
                    .sum::<f64>()
        })
        .collect::<Vec<_>>();
    let reported = days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let totals = std::iter::once(day)
                .chain(additional.iter().filter(|r| r["date"] == day["date"]))
                .filter_map(|r| r["totalHours"].as_f64())
                .collect::<Vec<_>>();
            if totals.is_empty() {
                rows[index]
            } else {
                totals.iter().sum()
            }
        })
        .collect::<Vec<_>>();
    let matches = |values: &[f64]| {
        values.iter().all(|v| v.is_finite() && *v >= 0.)
            && record["weeklyTotals"].as_array().is_some_and(|totals| {
                totals.len() == 2
                    && totals.iter().enumerate().all(|(i, t)| {
                        t.as_f64().is_some_and(|v| {
                            (values[i * 7..i * 7 + 7].iter().sum::<f64>() - v).abs() < 0.011
                        })
                    })
            })
            && record["periodTotalHours"]
                .as_f64()
                .is_some_and(|v| (values.iter().sum::<f64>() - v).abs() < 0.011)
    };
    let values = [rows, base, reported]
        .into_iter()
        .find(|h| matches(h))
        .ok_or_else(|| Error::new("provider_hours_mismatch", 409))?;
    days.iter().enumerate().map(|(index,day)|{
        let mut punches=Vec::new(); let mut pending=Value::Null; let mut pending_kind=Value::Null; let mut row=Value::Null;
        for punch in day["punches"].as_array().ok_or_else(error)? {
            if row!=punch["rowIndex"] && !pending.is_null() {
                punches.push(json!({"in":pending,"out":null,"hours":null,"inKind":pending_kind,"outKind":null}));
                pending=Value::Null; pending_kind=Value::Null;
            }
            row=punch["rowIndex"].clone();
            let kind = if s(punch,"kind").is_empty() {Value::Null} else {punch["kind"].clone()};
            if s(punch,"slot").starts_with('i') {
                if !pending.is_null() {punches.push(json!({"in":pending,"out":null,"hours":null,"inKind":pending_kind,"outKind":null}));}
                pending=punch["displayTime"].clone(); pending_kind=kind;
            } else {
                punches.push(json!({"in":pending,"out":punch["displayTime"],"hours":null,"inKind":pending_kind,"outKind":kind}));
                pending=Value::Null; pending_kind=Value::Null;
            }
        }
        if !pending.is_null(){punches.push(json!({"in":pending,"out":null,"hours":null,"inKind":pending_kind,"outKind":null}));}
        Ok(json!({"employeeCode":employee,"date":day["date"],"hours":(values[index]*100.).round()/100.,
            "status":if day["missingPunch"]==true{"Missing punch"}else if punches.is_empty(){"No punches"}else{"Complete"},
                "punches":punches}))
    }).collect()
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
    /// The roster, fetched by the signed-in tab itself.
    async fn roster_in_page(&self, input: Value) -> Result<Value> {
        // Start a bounded fetch in the isolated world, then poll. No command holds
        // the browser transport for a network-length timeout.
        self.page.evaluate(&format!(r#"(()=>{{globalThis.dispatchRoster=null;(async input=>{{try{{
            const response=await fetch(input.url,{{method:'POST',credentials:'include',redirect:'error',
                cache:'no-store',headers:input.headers,body:input.body,signal:AbortSignal.timeout(55000)}});
            if(response.status!==200||!/^application\/json(?:;|$)/i.test(response.headers.get('content-type')||'')||!response.body)throw 0;
            const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{{fatal:true}});let size=0,text='';
            for(;;){{const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>2097152){{await reader.cancel();throw 0;}}text+=decoder.decode(part.value,{{stream:true}});}}
            text+=decoder.decode();globalThis.dispatchRoster={{ok:true,value:JSON.parse(text)}};
        }}catch{{globalThis.dispatchRoster={{ok:false}};}}}})({input});return true;}})()"#)).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let raw = loop {
            ensure(Instant::now() < deadline, "provider_timeout", 504)?;
            let value = self.page.evaluate("globalThis.dispatchRoster").await?;
            if !value.is_null() {
                ensure(value["ok"] == true, "provider_unavailable", 502)?;
                break value["value"].clone();
            }
            sleep(Duration::from_millis(200)).await;
        };
        self.page
            .evaluate("delete globalThis.dispatchRoster")
            .await?;
        Ok(raw)
    }
    /// The roster for the selected period, read the way the search page reads it,
    /// with a session for plain HTTP when `http` allows one and it works.
    pub(crate) async fn roster(
        &mut self,
        timezone: &str,
        selected_date: Option<NaiveDate>,
        http: bool,
    ) -> Result<Roster> {
        self.new_page().await?;
        let api = if self.fixture {
            format!("{}/api/cl/timecard-search/employees", self.origin)
        } else {
            API.into()
        };
        self.page
            .command(
                "Fetch.enable",
                json!({"patterns":[{"urlPattern":api,"requestStage":"Request"}]}),
            )
            .await?;
        self.navigate(SEARCH).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let observed = loop {
            ensure(Instant::now() < deadline, "provider_timeout", 504)?;
            let event = self.browser.event(&self.page.id).await?;
            if event.is_null() {
                continue;
            }
            self.page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await?;
            if event["request"]["url"] == api && event["request"]["method"] == "POST" {
                break event["request"].clone();
            }
        };
        self.page.command("Fetch.disable", json!({})).await?;
        let zone: chrono_tz::Tz = timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        let body: Value = serde_json::from_str(s(&observed, "postData"))
            .map_err(|_| Error::new("roster_not_complete", 409))?;
        let (body, period, codes) = selected_body(
            &body,
            selected_date.unwrap_or_else(|| chrono::Utc::now().with_timezone(&zone).date_naive()),
        )?;
        let mut headers = serde_json::Map::new();
        if let Some(values) = observed["headers"].as_object() {
            for (key, value) in values {
                if [
                    "accept",
                    "authorization",
                    "content-type",
                    "x-xsrf-token",
                    "x-csrf-token",
                    "x-requested-with",
                ]
                .contains(&key.to_ascii_lowercase().as_str())
                {
                    headers.insert(key.clone(), value.clone());
                }
            }
        }
        // The first attempt reads over plain HTTP from here once the browser has signed
        // in and the roster shows the session works; a retry reads as it always has.
        let mut reader = None;
        let mut raw = None;
        if http {
            match Http::signed_in(&self.browser, &self.origin, &crate::HOSTS).await {
                Ok(client) => match read_roster(&client, &api, &headers, body.to_string()).await {
                    Ok(value) => {
                        raw = Some(value);
                        reader = Some(client);
                    }
                    Err(error) => fallback("roster", &error.code),
                },
                Err(error) => fallback("session", &error.code),
            }
        }
        let raw = match raw {
            Some(raw) => raw,
            None => {
                self.roster_in_page(json!({"url":api,"body":body.to_string(),"headers":headers}))
                    .await?
            }
        };
        let employees = employees(&raw, &codes)?;
        Ok(Roster {
            raw,
            period,
            employees,
            http: reader,
        })
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

/// A job's roster: the provider's response, its period and its validated employees.
pub(crate) struct Roster {
    pub raw: Value,
    pub period: Value,
    pub employees: Vec<Value>,
    pub http: Option<Http>,
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
    project(&record, s(employee, "code"))
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/collect.rs"]
mod tests;
