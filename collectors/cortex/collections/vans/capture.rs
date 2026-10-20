//! What a vans collection brings: every vehicle Amazon lists for the DSP's company, each row
//! kept as Amazon sent it beside the fields read from it. Every field is untrusted input,
//! bounded here.
use dispatch_core::{Error, Result, ensure, foundation::validate::token};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const JOB_KIND: &str = "cortex.vans.collect";
/// Far more vehicles than one company runs: a longer list is not one DSP's.
pub const MAX_VANS: usize = 2_000;
/// The largest row kept as Amazon sent it.
const MAX_ROW_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Collection {
    #[serde(rename = "vans")]
    Vans,
}

/// The vans of one DSP's company, as a job asks for them.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub collection: Collection,
    /// The DSP's short code, which picks its company out of those its sign-in sees.
    pub dsp_abbreviation: String,
}
impl Request {
    /// Whether a job request or a collection names the vans collection.
    pub fn is(value: &Value) -> bool {
        matches!(
            value.get("collection").and_then(Value::as_str),
            Some("vans")
        )
    }
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if !Self::is(value) {
            return Ok(None);
        }
        let request: Self =
            serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        ensure(token(&self.dsp_abbreviation, 32), "invalid_input", 400)
    }
}

/// One vehicle as the fleet lists it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Van {
    /// Its vehicle identification number, in capitals.
    pub vin: String,
    /// What the DSP calls it, as it named it in the fleet.
    pub name: Option<String>,
    /// The row as Amazon sent it.
    pub row: Value,
}
impl Van {
    /// One fleet row, read. `None` for a row naming no VIN, which nothing else Amazon sends
    /// could name either.
    pub fn read(row: Value) -> Result<Option<Self>> {
        ensure(row.is_object(), "vans_row_invalid", 502)?;
        ensure(
            row.to_string().len() <= MAX_ROW_BYTES,
            "vans_source_too_large",
            502,
        )?;
        let text = |key: &str, max: usize| -> Option<String> {
            let value = row.get(key)?.as_str()?.trim();
            (!value.is_empty()
                && value.chars().count() <= max
                && !value.chars().any(char::is_control))
            .then(|| value.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        let Some(vin) = text("vin", 17)
            .map(|vin| vin.to_ascii_uppercase())
            .filter(|vin| vin_shaped(vin))
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            name: text("vehicleName", 64),
            vin,
            row,
        }))
    }
}
/// Whether `vin` reads as a VIN: seventeen capitals and digits, never I, O or Q.
pub(crate) fn vin_shaped(vin: &str) -> bool {
    vin.len() == 17
        && vin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"IOQ".contains(&b)))
}

/// The DSP's fleet as collected, before publication.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub version: u32,
    pub collection: Collection,
    pub company_id: String,
    pub company_code: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub vans: Vec<Van>,
}
impl Capture {
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        ensure(self.version == 1, "vans_capture_invalid", 502)?;
        ensure(
            token(&self.company_id, 128)
                && token(&self.company_code, 32)
                && self
                    .company_code
                    .eq_ignore_ascii_case(&request.dsp_abbreviation),
            "vans_scope_mismatch",
            502,
        )?;
        ensure(
            self.started_at > 0 && self.finished_at >= self.started_at,
            "vans_capture_invalid",
            502,
        )?;
        ensure(self.vans.len() <= MAX_VANS, "vans_source_too_large", 502)?;
        let mut vins = BTreeSet::new();
        for van in &self.vans {
            ensure(
                vin_shaped(&van.vin) && vins.insert(van.vin.as_str()),
                "vans_capture_invalid",
                502,
            )?;
        }
        Ok(())
    }
}
