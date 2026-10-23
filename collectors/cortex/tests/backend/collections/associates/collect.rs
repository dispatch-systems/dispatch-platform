use super::*;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Response},
    routing::get,
};

#[test]
fn the_dsp_code_picks_its_one_company() {
    let company = |code: &str, records: serde_json::Value| {
        super::company(&json!({ "records": records }), code)
    };
    let fxtr = json!({"companyId":"c-1","shortCode":"FXTR","name":"Fixture","providerId":"p"});
    let other = json!({"companyId":"c-2","shortCode":"OTHR","name":"Other","providerId":"p"});
    assert_eq!(
        company("fxtr", json!([other, fxtr])).unwrap(),
        ("c-1".to_owned(), "FXTR".to_owned())
    );
    assert_eq!(
        company("NONE", json!([fxtr])).unwrap_err().code,
        "cortex_scope_mismatch"
    );
    let twin = json!({"companyId":"c-3","shortCode":"FXTR"});
    assert_eq!(
        company("FXTR", json!([fxtr, twin])).unwrap_err().code,
        "cortex_provider_ambiguous"
    );
    assert!(super::company(&json!({"error":"signed out"}), "FXTR").is_err());
}

#[test]
fn the_roster_reads_its_table_and_never_comes_back_empty() {
    let row = |id: &str, name: &str| {
        json!({"transporter_id":id,"full_name":name,"operational_status":"ACTIVE"}).to_string()
    };
    let reply = |rows: serde_json::Value| {
        json!({"tableData":{
            "dsp-associates-status-summations":{"rows":[]},
            "dsp-associates-table-data":{"rows":rows}}})
    };
    let read = roster(&reply(json!([
        row("A1", "First Name"),
        row("A1", "Second Spelling"),
        row("A2", "Other Name"),
        json!({"full_name":"No Transporter ID","operational_status":"ACTIVE"}),
    ])))
    .unwrap();
    assert_eq!(
        read.iter()
            .map(|a| (a.transporter_id.as_str(), a.name.as_str()))
            .collect::<Vec<_>>(),
        [("A1", "First Name"), ("A2", "Other Name")]
    );
    // A roster naming no one would say everyone left; a missing table, that the page changed.
    for broken in [reply(json!([])), json!({"tableData":{}}), json!([])] {
        assert_eq!(
            roster(&broken).unwrap_err().code,
            cortex::codes::CORTEX_SOURCE_CHANGED.as_str()
        );
    }
    // No one offboarded is a DSP that never offboarded anyone.
    assert!(rows(&reply(json!([]))).unwrap().is_empty());
    assert!(rows(&json!({"tableData":{}})).is_err());
}

#[test]
fn someone_offboarded_and_back_on_the_roster_is_as_the_roster_lists_them() {
    let read = |id: &str, status: &str| {
        Associate::read(json!({"transporter_id":id,"full_name":id,"operational_status":status}))
            .unwrap()
            .unwrap()
    };
    let listed = vec![read("A1", "ACTIVE"), read("A2", "INACTIVE")];
    let offboarded = vec![read("A1", "OFFBOARDED"), read("A3", "OFFBOARDED")];
    let all = everyone(listed, offboarded).unwrap();
    assert_eq!(
        all.iter()
            .map(|a| (a.transporter_id.as_str(), a.status.as_str()))
            .collect::<Vec<_>>(),
        [("A1", "ACTIVE"), ("A2", "INACTIVE"), ("A3", "OFFBOARDED")]
    );
}

#[tokio::test]
async fn a_photo_comes_from_where_its_address_redirects_without_amazons_cookies_unless_unchanged() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let origin = format!("http://{FIXTURE}:{port}");
    let elsewhere = format!("http://elsewhere.invalid:{port}/s3/photo");
    let app = Router::new()
        .route(
            "/workforce/photo",
            get(move |headers: HeaderMap| async move {
                assert!(headers.get("referer").is_some());
                Response::builder()
                    .status(302)
                    .header("location", "/s3/photo?X-Amz-Expires=180")
                    .body(Body::empty())
                    .unwrap()
            }),
        )
        .route(
            "/workforce/away",
            get(move || async move {
                Response::builder()
                    .status(302)
                    .header("location", elsewhere)
                    .body(Body::empty())
                    .unwrap()
            }),
        )
        .route("/workforce/page", get(|| async { "<html>a page</html>" }))
        .route(
            "/workforce/signed-out",
            get(|| async { Response::builder().status(403).body(Body::empty()).unwrap() }),
        )
        .route(
            "/s3/photo",
            get(|headers: HeaderMap| async move {
                assert!(headers.get("cookie").is_none());
                assert!(headers.get("referer").is_none());
                let current = headers
                    .get("if-none-match")
                    .is_some_and(|etag| etag == "\"v2\"");
                Response::builder()
                    .status(if current { 304 } else { 200 })
                    .header("etag", "\"v2\"")
                    .body(Body::from(if current {
                        &b""[..]
                    } else {
                        b"\xFF\xD8\xFF a jpeg"
                    }))
                    .unwrap()
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = Http::detached(&origin, &cortex::HOSTS).unwrap();
    let downloads = Http::detached(&origin, &cortex::PHOTO_HOSTS).unwrap();
    let fetch = |address: &'static str, etag: Option<&'static str>| {
        photo(&http, &downloads, &origin, "/workforce", address, etag)
    };
    let served = Some((b"\xFF\xD8\xFF a jpeg".to_vec(), Some("\"v2\"".to_owned())));
    assert_eq!(fetch("/workforce/photo", None).await.unwrap(), served);
    // Asked for by the ETag it was kept under, it comes again only once it changed.
    assert_eq!(
        fetch("/workforce/photo", Some("\"v2\"")).await.unwrap(),
        None
    );
    assert_eq!(
        fetch("/workforce/photo", Some("\"v1\"")).await.unwrap(),
        served
    );
    // Only where Amazon serves photos; an answer that isn't a redirect is no photo. A session
    // the application refuses would be refused every photo after.
    assert_eq!(
        fetch("/workforce/away", None).await.unwrap_err(),
        Missed::Skipped("photo_redirect_unexpected")
    );
    assert_eq!(
        fetch("/workforce/page", None).await.unwrap_err(),
        Missed::Skipped("http_not_redirected")
    );
    assert_eq!(
        fetch("/workforce/signed-out", None).await.unwrap_err(),
        Missed::Refused("http_signed_out")
    );
    let s3 = url::Url::parse(&format!("https://{PHOTO_HOST}/photo?X-Amz-Expires=180")).unwrap();
    assert!(cortex::PHOTO_HOSTS.allows(&s3));
    assert!(!cortex::HOSTS.allows(&s3));
    assert!(!(cortex::PHOTO_HOSTS.cookies)("amazon.com"));
    server.abort();
}
