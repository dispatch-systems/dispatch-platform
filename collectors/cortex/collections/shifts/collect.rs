//! Reads the station's week over plain HTTP with the browser's session: the roster the
//! Scheduling week view draws, every driver with the assignments of each day, for the
//! station's service area. Only finding the service area may need a page, the first time;
//! the browser closes once the week is read.
use super::capture::{Assignment, Capture, Collection, MAX_ASSIGNMENTS, MAX_DRIVERS, Request};
use crate as cortex;
use crate::{
    connection::{
        Driver,
        companies::{COMPANIES, refusal, sole_company},
        performance::{Found, mismatched},
    },
    discovery::Scope,
};
use dispatch_core::{
    Error, Result,
    collection::browser::{Run, http::Http},
    db::now,
    ensure,
};
use jiff::civil::Date;

/// The week view's roster: drivers, each with their assignments and work blocks by day.
/// Kept as its path's segments: the build's privacy check reads "/home/" followed by a name as
/// a home directory left in the binary.
const ROSTERS: [&str; 6] = ["", "schedulingV2", "home", "api", "v4", "rosters"];
/// The week view, which the roster is read from.
const WEEK: &str = "/scheduling/calendar-view/week";
/// As much as one reply may hold. One DSP's week, 127 drivers and 482 assignments, was 528 KB.
const LIMIT: usize = 32 * 1024 * 1024;

fn changed() -> Error {
    Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502)
}

/// The address of `path` for the week from `start` to `end` at `service_area`.
fn address(origin: &str, path: &str, service_area: &str, dates: Option<(Date, Date)>) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    match dates {
        Some((start, end)) => query
            .append_pair("fromDate", &start.to_string())
            .append_pair("serviceAreaId", service_area)
            .append_pair("toDate", &end.to_string()),
        None => query.append_pair("serviceAreaId", service_area),
    };
    format!("{origin}{path}?{}", query.finish())
}

/// Every assignment of the week from `start` to `end` the roster lists at `service_area`,
/// and how many drivers it listed.
pub(crate) async fn roster(
    http: &Http,
    origin: &str,
    service_area: &str,
    (start, end): (Date, Date),
) -> Result<(Vec<Assignment>, usize)> {
    let referer = address(origin, WEEK, service_area, None);
    let reply = http
        .json(
            &address(origin, &ROSTERS.join("/"), service_area, Some((start, end))),
            &referer,
            LIMIT,
        )
        .await
        .map_err(refusal)?;
    ensure(
        reply["error"].is_null(),
        cortex::codes::CORTEX_SOURCE_CHANGED.as_str(),
        502,
    )?;
    let drivers = reply["data"].as_array().ok_or_else(changed)?;
    // A roster naming no driver is a read that failed: the station's roster lists every
    // driver, scheduled that week or not.
    ensure(
        !drivers.is_empty(),
        cortex::codes::CORTEX_SOURCE_CHANGED.as_str(),
        502,
    )?;
    ensure(drivers.len() <= MAX_DRIVERS, "shifts_source_too_large", 502)?;
    let mut assignments = Vec::new();
    for driver in drivers {
        ensure(
            driver["serviceAreaId"] == service_area,
            cortex::codes::CORTEX_SCOPE_MISMATCH.as_str(),
            502,
        )?;
        let days = driver["shiftAssignmentsMap"]
            .as_object()
            .ok_or_else(changed)?;
        for (date, items) in days {
            let day = dispatch_core::foundation::clock::date(date).map_err(|_| changed())?;
            ensure(
                (start..=end).contains(&day) && day.to_string() == *date,
                "shifts_capture_invalid",
                502,
            )?;
            for item in items.as_array().ok_or_else(changed)? {
                assignments.push(Assignment::read(driver, date, item.clone(), service_area)?);
            }
            ensure(
                assignments.len() <= MAX_ASSIGNMENTS,
                "shifts_source_too_large",
                502,
            )?;
        }
    }
    Ok((assignments, drivers.len()))
}

impl Driver {
    pub(crate) async fn collect_shifts(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        request.validate()?;
        run.progress(10, "Finding the station".into()).await?;
        // A kept service area whose roster named another is found again.
        let (scope, found) = self.scope_for(&request.scope_request(), false, run).await?;
        let captured = match self.week_of_shifts(request, &scope, run).await {
            Err(error) if mismatched(&error) && found == Found::Known => {
                let _ = self.known.forget(&request.station);
                run.metrics.detail("known_scope_mismatched");
                let (scope, _) = self.scope_for(&request.scope_request(), true, run).await?;
                self.week_of_shifts(request, &scope, run).await
            }
            captured => captured,
        };
        self.browser.close().await;
        captured
    }
    async fn week_of_shifts(
        &mut self,
        request: &Request,
        scope: &Scope,
        run: &Run<'_>,
    ) -> Result<Capture> {
        let started_at = now();
        run.progress(30, "Checking the DSP's company".into())
            .await?;
        let http = Http::signed_in(&self.browser, &self.origin, &cortex::HOSTS).await?;
        let companies = match self.companies.take() {
            Some(companies) => companies,
            None => http
                .json(
                    &format!("{}{COMPANIES}", self.origin),
                    &format!("{}/workforce", self.origin),
                    LIMIT,
                )
                .await
                .map_err(refusal)?,
        };
        // The roster answers for the account at the service area: one seeing several
        // companies could answer with another DSP's drivers.
        sole_company(&companies, &request.dsp_abbreviation)?;
        run.progress(50, "Reading the week's shifts".into()).await?;
        let (start, end) = request.week()?;
        let (assignments, drivers) =
            roster(&http, &self.origin, &scope.service_area_id, (start, end)).await?;
        run.progress(95, "Validating the week's shifts".into())
            .await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Shifts,
            station: request.station.clone(),
            service_area_id: scope.service_area_id.clone(),
            week_start: start.to_string(),
            week_end: end.to_string(),
            started_at,
            finished_at: now().max(started_at),
            drivers,
            assignments,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/shifts/collect.rs"]
mod tests;
