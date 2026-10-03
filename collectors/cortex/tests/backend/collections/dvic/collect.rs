use super::*;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
};
use url::Url;
#[test]
fn listings_use_publication_dates_and_reject_foreign_hosts_paths_and_duplicate_objects() {
    let capture = dvic::fixture(&Request {
        collection: Collection::Dvic,
        station: "TST1".into(),
        weeks: vec!["2026-W39".into()],
        date: "2026-10-02".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Delivery".into(),
        dsp_abbreviation: "FXTR".into(),
    })
    .unwrap();
    let report = &capture.reports[0];
    let row = json!({"name":report.name,"type":"xlsx","date":"2026-W39",
        "downloadUrl":format!("https://{}{}?temporary=signature",REPORT_HOST,report.source_key),
        "creationDate":[2026,9,27,14,4,0]});
    let wrap = |rows: Value| json!({"tableData":{dvic::DATASET:{"rows":rows}}});
    let read = |value| {
        listing(
            value,
            "2026-W39",
            "FXTR",
            "TST1",
            "https://logistics.amazon.com",
        )
    };
    let parsed = read(wrap(json!([row]))).unwrap();
    assert_eq!(parsed[0].date, "2026-09-27");
    assert!(!parsed[0].path.contains('?'));
    assert_eq!(read(wrap(json!([row.to_string()]))).unwrap().len(), 1);
    assert!(read(wrap(json!([row, row]))).is_err());
    for target in [
        format!("https://evil.example{}", report.source_key),
        format!(
            "https://{}{}",
            REPORT_HOST,
            report.source_key.replace("/fxtr/", "/other/")
        ),
        format!("https://{}:8443{}", REPORT_HOST, report.source_key),
    ] {
        let mut wrong = row.clone();
        wrong["downloadUrl"] = json!(target);
        assert!(read(wrap(json!([wrong]))).is_err());
    }
    assert!(read(json!({"tableData":{}})).unwrap().is_empty());
    assert!(read(json!({"message":"signed out"})).is_err());
}
#[tokio::test]
async fn report_downloads_use_validators_bound_bodies_and_never_follow_redirects_or_forward_cookies()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new()
        .route(
            "/report",
            get(|headers: HeaderMap| async move {
                assert!(headers.get("cookie").is_none());
                assert!(headers.get("referer").is_none());
                let unchanged = headers.get("if-none-match").is_some_and(|h| h == "\"v1\"");
                Response::builder()
                    .status(if unchanged { 304 } else { 200 })
                    .header("etag", "\"v1\"")
                    .header("last-modified", "Sun, 27 Sep 2026 14:04:00 GMT")
                    .body(Body::from(if unchanged { "" } else { "workbook" }))
                    .unwrap()
            }),
        )
        .route(
            "/redirect",
            get(|| async {
                Response::builder()
                    .status(302)
                    .header("location", "/report")
                    .body(Body::empty())
                    .unwrap()
            }),
        )
        .route(
            "/throttled",
            get(|| async { StatusCode::TOO_MANY_REQUESTS }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let origin = format!("http://{FIXTURE}:{port}");
    let client = reports(&origin).unwrap();
    let url = format!("{origin}/report");
    let first = client.download(&url, None, 8).await.ok().unwrap();
    assert_eq!(first.body.unwrap(), b"workbook");
    assert!(first.modified_at.is_some());
    let cached = client
        .download(&url, first.etag.as_deref(), 8)
        .await
        .ok()
        .unwrap();
    assert!(cached.body.is_none());
    assert!(matches!(
        client.download(&url, None, 4).await,
        Err(Refusal::Unreadable("http_body_too_large"))
    ));
    assert!(matches!(
        client
            .download(&format!("{origin}/redirect"), None, 8)
            .await,
        Err(Refusal::Unreadable("http_download_refused"))
    ));
    assert!(matches!(
        client
            .download(&format!("{origin}/throttled"), None, 8)
            .await,
        Err(Refusal::Unavailable)
    ));
    let s3 = Url::parse(&format!("https://{}/report", REPORT_HOST)).unwrap();
    assert!(cortex::REPORT_HOSTS.allows(&s3));
    assert!(!cortex::HOSTS.allows(&s3));
    assert!(!(cortex::REPORT_HOSTS.cookies)("amazon.com"));
    assert!(
        !cortex::REPORT_HOSTS.allows(&Url::parse("https://logistics.amazon.com/report").unwrap())
    );
    server.abort();
}
