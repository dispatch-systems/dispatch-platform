//! Captured roster requests, complete employee validation and HTTP/browser selection.
use super::{collect::fallback, validate_workforce};
use crate::connection::{Driver, SEARCH};
use chrono::{Datelike, NaiveDate};
use dispatch_core::{
    Error, Result,
    collection::browser::http::Http,
    db::{self, s},
    ensure,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};
use tokio::time::{Instant, sleep};

const API: &str = "https://time-and-attendance.paycomonline.net/api/cl/timecard-search/employees";

/// A job's roster: the provider's response, its period and its validated employees.
pub(crate) struct Roster {
    pub raw: Value,
    pub period: Value,
    pub employees: Vec<Value>,
    pub http: Option<Http>,
}

/// The current source roster wins for listed people. Known DSP-local people omitted
/// from its active-only search still receive their individual timecard reads.
pub(super) fn with_known_employees(
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
/// The roster request the search page made, sent again with the selected period.
async fn read_roster(
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

impl Driver {
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
            selected_date.unwrap_or_else(|| dispatch_core::foundation::clock::today(zone)),
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
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/roster.rs"]
mod tests;
