use super::*;
use axum::{
    Router,
    http::{Response, Uri},
    routing::get,
};
use dispatch_core::collection::browser::http::FIXTURE;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// A driver's roster row at `area`, with `days` as the week view lists them.
fn driver(id: &str, area: &str, days: Value) -> Value {
    json!({"driverPersonId":id,"driverName":"Avery Example","serviceAreaId":area,
        "driverEmail":"avery@example.com","shiftAssignmentsMap":days,"reservationsMap":{}})
}
fn assignment(id: &str, date: &str) -> Value {
    json!({"mappingId":id,"mappingDate":date,"serviceAreaId":"area-1","assignmentType":"PRIMARY",
        "shiftId":"s-1","shiftName":"Standard Parcel","shiftType":"REGULAR","shiftPosition":"DRIVER",
        "startTimeMin":600,"startTimeMax":600,"durationMin":600,"durationMax":600,"mappingStatus":"ACTIVE"})
}
fn reply(body: Value) -> Response<String> {
    Response::builder()
        .header("content-type", "application/json")
        .body(body.to_string())
        .unwrap()
}

#[tokio::test]
async fn the_week_is_read_at_the_service_area_every_assignment_with_its_driver() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    // What the stand-in answers: the week, a row of another service area, a week with no shift
    // yet, a roster naming no driver, an error, or a session Amazon ended.
    let scenario = Arc::new(Mutex::new("week"));
    let answers = scenario.clone();
    let app = Router::new().route(
        "/schedulingV2/home/api/v4/rosters",
        get(move |uri: Uri| async move {
            let query: HashMap<String, String> =
                url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                    .into_owned()
                    .collect();
            // The Sunday-to-Saturday week, at the service area.
            assert_eq!(
                (
                    query["fromDate"].as_str(),
                    query["toDate"].as_str(),
                    query["serviceAreaId"].as_str()
                ),
                ("2026-11-15", "2026-11-21", "area-1")
            );
            let week = json!([
                driver("A1FIXTURE00001", "area-1", json!({
                    "2026-11-16": [assignment("m-1", "2026-11-16")],
                    "2026-11-17": [assignment("m-2", "2026-11-17"), assignment("m-3", "2026-11-17")],
                })),
                // A driver with nothing that week is on the roster still.
                driver("A1FIXTURE00002", "area-1", json!({})),
            ]);
            match *answers.lock().unwrap() {
                "week" => reply(json!({"data":week,"meta":{},"error":null})),
                "elsewhere" => reply(json!({"data":[driver("A1FIXTURE00003", "area-2", json!({}))],"error":null})),
                "unscheduled" => reply(json!({"data":[driver("A1FIXTURE00002", "area-1", json!({}))],"error":null})),
                "none" => reply(json!({"data":[],"error":null})),
                "error" => reply(json!({"data":[],"error":{"message":"failed"}})),
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
    let week = crate::shifts::week(dispatch_core::foundation::clock::date("2026-11-18").unwrap());
    let (assignments, drivers) = roster(&http, &origin, "area-1", week).await.unwrap();
    assert_eq!(drivers, 2);
    assert_eq!(
        assignments
            .iter()
            .map(|a| (
                a.mapping_id.as_str(),
                a.date.as_str(),
                a.transporter_id.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("m-1", "2026-11-16", "A1FIXTURE00001"),
            ("m-2", "2026-11-17", "A1FIXTURE00001"),
            ("m-3", "2026-11-17", "A1FIXTURE00001"),
        ]
    );
    *scenario.lock().unwrap() = "unscheduled";
    let (assignments, drivers) = roster(&http, &origin, "area-1", week).await.unwrap();
    assert_eq!((assignments.len(), drivers), (0, 1));
    for (what, code) in [
        ("elsewhere", cortex::codes::CORTEX_SCOPE_MISMATCH.as_str()),
        ("none", cortex::codes::CORTEX_SOURCE_CHANGED.as_str()),
        ("error", cortex::codes::CORTEX_SOURCE_CHANGED.as_str()),
        ("ended", "verification_required"),
    ] {
        *scenario.lock().unwrap() = what;
        assert_eq!(
            roster(&http, &origin, "area-1", week)
                .await
                .unwrap_err()
                .code,
            code,
            "{what}"
        );
    }
    server.abort();
}
