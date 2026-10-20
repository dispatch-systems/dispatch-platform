//! What fixture mode collects: a small made-up fleet, named as a DSP names its vans.
use super::capture::{Capture, Collection, Request, Van};
use dispatch_core::{Error, Result, db::now};
use serde_json::json;

/// VIN and the name the DSP gives the van.
const FLEET: [(&str, &str); 6] = [
    ("1FTBW3XM5LKA00101", "101"),
    ("1FTBW3XM5LKA00102", "102"),
    ("1FTBW3XM5LKA00103", "103"),
    ("7FCTGAAA1NN000214", "214"),
    ("7FCTGAAA1NN000215", "215"),
    ("3C6LRVDG0ME000301", "Rental 301"),
];

pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let vans = FLEET
        .iter()
        .map(|(vin, name)| {
            Van::read(json!({"vin": vin, "vehicleName": name}))?
                .ok_or_else(|| Error::new("vans_row_invalid", 502))
        })
        .collect::<Result<Vec<_>>>()?;
    let capture = Capture {
        version: 1,
        collection: Collection::Vans,
        company_id: "company-fixture".into(),
        company_code: request.dsp_abbreviation.clone(),
        started_at,
        finished_at: now().max(started_at),
        vans,
    };
    capture.validate(request)?;
    Ok(capture)
}
