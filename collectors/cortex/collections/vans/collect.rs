//! Reads the fleet over plain HTTP with the browser's session: the company the DSP's code
//! names, from the list the start's session check read when it had one, then every vehicle
//! the fleet lists, active or not, in as many pages as its continuation keys ask. Nothing
//! needs a page, so the browser closes as soon as its session is copied. Against the fleet
//! page's own request this took 2.3 s and 2.0 CPU-seconds, not 4.2 and 5.7.
use super::capture::{Capture, Collection, MAX_VANS, Request, Van};
use crate as cortex;
use crate::collections::associates::collect::{COMPANIES, company, refusal};
use crate::connection::Driver;
use dispatch_core::{
    Error, Result,
    collection::browser::{Run, http::Http},
    db::now,
    ensure,
};
use serde_json::Value;
use std::collections::BTreeSet;

const VEHICLES: &str = "/fleet-management/api/vehicles";
/// Every status the list accepts, as the fleet page's own "all vehicles" asks: it refuses
/// `OUT_OF_SERVICE` and a request naming none. Inactive vans are those the DSP returned, sold
/// or retired, which older routes still name; asking for them costs no time.
const STATUSES: &str = "ACTIVE,MAINTENANCE,PENDING,INACTIVE";
/// As much as one reply may hold. Three hundred vans are 430 KB.
const LIMIT: usize = 16 * 1024 * 1024;
/// The most pages one collection follows. One page held 318 vans.
const MAX_PAGES: usize = 50;

fn changed() -> Error {
    Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502)
}

/// The DSP's company, when it is the only one the sign-in sees: the fleet answers for the
/// account and takes no company to scope it by, so an account seeing several could answer
/// with another DSP's vans.
pub(crate) fn sole_company(reply: &Value, code: &str) -> Result<(String, String)> {
    let found = company(reply, code)?;
    ensure(
        reply["records"]
            .as_array()
            .is_some_and(|records| records.len() == 1),
        cortex::codes::CORTEX_PROVIDER_AMBIGUOUS.as_str(),
        502,
    )?;
    Ok(found)
}

/// The list's address, from where `key` left off.
fn address(origin: &str, key: Option<&str>) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("vehicleStatuses", STATUSES);
    if let Some(key) = key {
        query.append_pair("continuationKey", key);
    }
    format!("{origin}{VEHICLES}?{}", query.finish())
}

/// Every van the fleet lists, each VIN once, page after page. A fleet naming no van would say
/// every van left: it is a page that failed instead.
pub(crate) async fn fleet(http: &Http, origin: &str) -> Result<Vec<Van>> {
    let referer = format!("{origin}/fleet-management/");
    let mut seen = BTreeSet::new();
    let mut vans = Vec::new();
    let mut key: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let reply = http
            .json(&address(origin, key.as_deref()), &referer, LIMIT)
            .await
            .map_err(refusal)?;
        let rows = reply["data"]["vehicles"].as_array().ok_or_else(changed)?;
        for row in rows {
            if let Some(van) = Van::read(row.clone())?
                && seen.insert(van.vin.clone())
            {
                vans.push(van);
            }
        }
        ensure(vans.len() <= MAX_VANS, "vans_source_too_large", 502)?;
        key = match &reply["data"]["continuationKey"] {
            Value::Null => {
                ensure(
                    !vans.is_empty(),
                    cortex::codes::CORTEX_SOURCE_CHANGED.as_str(),
                    502,
                )?;
                return Ok(vans);
            }
            // A key that names the page just read would read it forever.
            Value::String(next) if !next.is_empty() && key.as_ref() != Some(next) => {
                Some(next.clone())
            }
            _ => return Err(changed()),
        };
    }
    Err(Error::new("vans_source_too_large", 502))
}

impl Driver {
    pub(crate) async fn collect_vans(
        &mut self,
        request: &Request,
        run: &Run<'_>,
    ) -> Result<Capture> {
        request.validate()?;
        let started_at = now();
        run.progress(15, "Finding the DSP's company".into()).await?;
        let http = Http::signed_in(&self.browser, &self.origin, &cortex::HOSTS).await?;
        self.browser.close().await;
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
        let (company_id, company_code) = sole_company(&companies, &request.dsp_abbreviation)?;
        run.progress(40, "Reading the fleet".into()).await?;
        let vans = fleet(&http, &self.origin).await?;
        run.progress(95, "Validating the fleet".into()).await?;
        let capture = Capture {
            version: 1,
            collection: Collection::Vans,
            company_id,
            company_code,
            started_at,
            finished_at: now().max(started_at),
            vans,
        };
        capture.validate(request)?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/vans/collect.rs"]
mod tests;
