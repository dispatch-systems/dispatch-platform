use super::*;
use axum::{
    Router,
    http::{Response, Uri},
    routing::get,
};
use dispatch_core::collection::browser::http::FIXTURE;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[test]
fn the_fleet_is_read_only_for_an_account_seeing_the_dsps_company_alone() {
    let fxtr = json!({"companyId":"c-1","shortCode":"FXTR","providerId":"p-1"});
    let other = json!({"companyId":"c-2","shortCode":"OTHR","providerId":"p-2"});
    let sole = |records: Value| sole_company(&json!({ "records": records }), "fxtr");
    assert_eq!(
        sole(json!([fxtr])).unwrap(),
        ("c-1".to_owned(), "FXTR".to_owned())
    );
    // The fleet takes no company to scope it by: an account seeing another could answer with
    // its vans.
    assert_eq!(
        sole(json!([fxtr, other])).unwrap_err().code,
        cortex::codes::CORTEX_PROVIDER_AMBIGUOUS.as_str()
    );
    assert_eq!(
        sole(json!([other])).unwrap_err().code,
        cortex::codes::CORTEX_SCOPE_MISMATCH.as_str()
    );
}

/// A van's row as the fleet lists it.
fn row(vin: &str, name: &str) -> Value {
    json!({"vin":vin,"dspVehicleId":name,"status":"ACTIVE","serviceStation":{"stationCode":"TST1"}})
}
fn page(vans: Value, key: Value) -> Response<String> {
    Response::builder()
        .header("content-type", "application/json")
        .body(json!({"data":{"vehicles":vans,"continuationKey":key}}).to_string())
        .unwrap()
}

#[tokio::test]
async fn the_fleet_is_read_page_after_page_each_van_once() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    // What the stand-in answers: a fleet of two pages, a key naming its own page, a fleet
    // naming no van, or a session Amazon ended.
    let scenario = Arc::new(Mutex::new("paged"));
    let answers = scenario.clone();
    let app = Router::new().route(
        "/fleet-management/api/vehicles",
        get(move |uri: Uri| async move {
            let query: HashMap<String, String> =
                url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                    .into_owned()
                    .collect();
            // Every status the list accepts, inactive vans with the rest.
            assert_eq!(
                query["vehicleStatuses"],
                "ACTIVE,MAINTENANCE,PENDING,INACTIVE"
            );
            let key = query.get("continuationKey").map(String::as_str);
            match (*answers.lock().unwrap(), key) {
                ("paged", None) => page(
                    json!([row("1FTBW3XM5LKA00101", "CDV 101"), {"vin":null}]),
                    json!("second"),
                ),
                ("paged", Some("second")) => page(
                    json!([
                        row("1ftbw3xm5lka00101", "CDV 101"),
                        row("7FCTGAAA1NN000214", "EDV 214")
                    ]),
                    // An empty key ends the list as none does.
                    json!(""),
                ),
                ("again", _) => page(json!([row("1FTBW3XM5LKA00101", "CDV 101")]), json!("again")),
                ("empty", _) => page(json!([]), Value::Null),
                _ => Response::builder()
                    .status(302)
                    .header("location", "/ap/signin")
                    .body(String::new())
                    .unwrap(),
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = Http::detached(&origin, &cortex::HOSTS).unwrap();
    let (vans, unread) = fleet(&http, &origin).await.unwrap();
    assert_eq!(unread, 1);
    assert_eq!(
        vans.iter()
            .map(|van| (van.vin.as_str(), van.name.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("1FTBW3XM5LKA00101", Some("CDV 101")),
            ("7FCTGAAA1NN000214", Some("EDV 214"))
        ]
    );
    for (what, code) in [
        ("again", cortex::codes::CORTEX_SOURCE_CHANGED.as_str()),
        ("empty", cortex::codes::CORTEX_SOURCE_CHANGED.as_str()),
        ("ended", "verification_required"),
    ] {
        *scenario.lock().unwrap() = what;
        assert_eq!(
            fleet(&http, &origin).await.unwrap_err().code,
            code,
            "{what}"
        );
    }
    server.abort();
}
