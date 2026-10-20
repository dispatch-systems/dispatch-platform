use super::*;
use serde_json::json;

#[test]
fn fleet_rows_read_into_bounded_vans() {
    let row = json!({
        "vin": " 7fctgaaa1nn000214 ",
        "dspVehicleId": "  EDV   214 ",
        "registrationNo": "FXT0100",
        "status": "ACTIVE",
        "serviceTier": "ELECTRIC_RPV_MEDIUM",
        "serviceStation": {"stationCode": "TST1"},
    });
    let van = Van::read(row.clone()).unwrap().unwrap();
    assert_eq!(
        (van.vin.as_str(), van.name.as_deref(), van.status.as_str()),
        ("7FCTGAAA1NN000214", Some("EDV 214"), "ACTIVE")
    );
    assert_eq!(van.station.as_deref(), Some("TST1"));
    assert_eq!(van.row, row);
    let with = |key: &str, value: Value| {
        let mut row = row.clone();
        row[key] = value;
        Van::read(row)
    };
    // No VIN, or one with a letter a VIN never holds, names no van.
    assert!(with("vin", Value::Null).unwrap().is_none());
    assert!(with("vin", json!("7FCTGAAA1NN00021O")).unwrap().is_none());
    // A van the DSP never named keeps its VIN; one with no status is a row Amazon changed.
    assert_eq!(with("dspVehicleId", json!("")).unwrap().unwrap().name, None);
    assert_eq!(
        with("status", Value::Null).unwrap_err().code,
        "vans_row_invalid"
    );
}
