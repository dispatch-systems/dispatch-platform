//! The companies a Cortex session sees and refusals from authenticated HTTP reads.
use crate as cortex;
use dispatch_core::{
    Error, Result, collection::browser::http::Refusal, foundation::validate::token,
};
use serde_json::Value;

pub(crate) const COMPANIES: &str = "/workforce/listCompanies";

pub(crate) fn refusal(error: Refusal) -> Error {
    match error {
        Refusal::Unavailable => Error::new("provider_unavailable", 502),
        Refusal::Unreadable("http_signed_out") => Error::new("verification_required", 409),
        Refusal::Unreadable(_) => Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502),
    }
}

/// The company whose short code is the DSP's, of those the sign-in sees.
pub(crate) fn company(reply: &Value, code: &str) -> Result<(String, String)> {
    let records = reply["records"]
        .as_array()
        .ok_or_else(|| Error::new(cortex::codes::CORTEX_SOURCE_CHANGED.as_str(), 502))?;
    let named: Vec<(String, String)> = records
        .iter()
        .filter_map(|record| {
            let id = record["companyId"].as_str()?;
            let short = record["shortCode"].as_str()?;
            (token(id, 128) && token(short, 32) && short.eq_ignore_ascii_case(code))
                .then(|| (id.to_owned(), short.to_owned()))
        })
        .collect();
    match named.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Error::new(
            cortex::codes::CORTEX_SCOPE_MISMATCH.as_str(),
            502,
        )),
        _ => Err(Error::new(
            cortex::codes::CORTEX_PROVIDER_AMBIGUOUS.as_str(),
            502,
        )),
    }
}
