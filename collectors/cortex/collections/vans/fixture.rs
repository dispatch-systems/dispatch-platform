//! What fixture mode collects: a small made-up fleet, named as a DSP names its vans.
use super::capture::{Capture, Collection, Request, Van};
use dispatch_core::{Error, Result, db::now};
use serde_json::json;

/// VIN, the name the DSP gives the van, its status and kind.
const FLEET: [(&str, &str, &str, &str); 6] = [
    (
        "1FTBW3XM5LKA00101",
        "CDV 101",
        "ACTIVE",
        "EXTRA_LARGE_CARGO_VAN",
    ),
    (
        "1FTBW3XM5LKA00102",
        "CDV 102",
        "ACTIVE",
        "EXTRA_LARGE_CARGO_VAN",
    ),
    (
        "1FTBW3XM5LKA00103",
        "CDV 103",
        "MAINTENANCE",
        "EXTRA_LARGE_CARGO_VAN",
    ),
    (
        "7FCTGAAA1NN000214",
        "EDV 214",
        "ACTIVE",
        "ELECTRIC_RPV_MEDIUM",
    ),
    (
        "7FCTGAAA1NN000215",
        "EDV 215",
        "ACTIVE",
        "ELECTRIC_RPV_MEDIUM",
    ),
    (
        "3C6LRVDG0ME000301",
        "Rental 301",
        "INACTIVE",
        "LARGE_CARGO_VAN",
    ),
];

pub fn fixture(request: &Request) -> Result<Capture> {
    request.validate()?;
    let started_at = now();
    let vans = FLEET
        .iter()
        .enumerate()
        .map(|(index, (vin, name, status, tier))| {
            Van::read(json!({
                "vin": vin,
                "dspVehicleId": name,
                "registrationNo": format!("FXT{:04}", 100 + index),
                "status": status,
                "serviceTier": tier,
                "serviceStation": {"stationCode": "TST1"},
            }))?
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
