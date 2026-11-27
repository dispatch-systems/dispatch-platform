use super::*;
use crate::connection::companies;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Response},
    routing::get,
};
use dispatch_core::collection::browser::http::{Cookie, SignedIn};

#[test]
fn the_dsp_code_picks_its_one_company() {
    let company = |code: &str, records: serde_json::Value| {
        companies::company(&json!({ "records": records }), code)
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
    assert!(companies::company(&json!({"error":"signed out"}), "FXTR").is_err());
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
                assert_eq!(headers["cookie"], "session-token=fixture");
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
            "/workforce/refused",
            get(|| async { Response::builder().status(403).body(Body::empty()).unwrap() }),
        )
        .route(
            "/workforce/signed-out",
            get(|| async { Response::builder().status(401).body(Body::empty()).unwrap() }),
        )
        .route(
            "/workforce/placeholder",
            get(|| async {
                Response::builder()
                    .status(302)
                    .header(
                        "location",
                        "https://m.media-amazon.com/images/G/01/x/associate_photo_placeholder.png",
                    )
                    .body(Body::empty())
                    .unwrap()
            }),
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
    let session = SignedIn {
        origin: origin.clone(),
        cookies: vec![Cookie {
            name: "session-token".into(),
            value: "fixture".into(),
            domain: FIXTURE.into(),
            path: "/".into(),
            secure: false,
            expires: None,
        }],
        user_agent: "Fixture".into(),
    };
    let http = Http::with_session(&session, &cortex::HOSTS).unwrap();
    let downloads = Http::detached(&origin, &cortex::PHOTO_HOSTS).unwrap();
    let turn = Turn::of(&origin, "c-1", Box::new(|| Box::pin(async { Ok(()) })));
    let refusals = Refusals::default();
    let fetch = |address: &'static str, etag: Option<&'static str>| {
        photo(
            &http,
            &downloads,
            &turn,
            &refusals,
            &origin,
            "/workforce",
            address,
            etag,
        )
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
    // Only where Amazon serves photos; an answer that isn't a redirect is no photo. Amazon's
    // placeholder is a photo Amazon doesn't serve; a 403 is the refusals' to judge, and a 401
    // a session the application refuses, which would be refused every photo after.
    assert_eq!(
        fetch("/workforce/away", None).await.unwrap_err(),
        Missed::Skipped("photo_redirect_unexpected")
    );
    assert_eq!(
        fetch("/workforce/page", None).await.unwrap_err(),
        Missed::Skipped("http_not_redirected")
    );
    assert_eq!(
        fetch("/workforce/placeholder", None).await.unwrap_err(),
        Missed::Unserved("photo_placeholder")
    );
    assert_eq!(
        fetch("/workforce/refused", None).await.unwrap_err(),
        Missed::Forbidden
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

// A roster's photos, asked for sixteen at a time, ask Cortex for where each is served no
// more at once than the account's gate admits. One whose address Cortex throttles is asked
// again once the account has cooled, and comes with the rest.
#[cfg_attr(target_os = "macos", ignore = "paused clock races I/O; CI runs it")]
#[tokio::test(start_paused = true)]
async fn photos_ask_cortex_no_more_at_once_than_its_gate_admits_and_wait_out_a_throttle() {
    use axum::extract::{Path, State};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Default)]
    struct Stand {
        answering: AtomicUsize,
        most_at_once: AtomicUsize,
        asked: AtomicUsize,
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let origin = format!("http://{FIXTURE}:{port}");
    let stand = Arc::new(Stand::default());
    let app = Router::new()
        .route(
            "/workforce/photo/{n}",
            get(
                |State(stand): State<Arc<Stand>>, Path(n): Path<usize>| async move {
                    let answering = stand.answering.fetch_add(1, Ordering::SeqCst) + 1;
                    stand.most_at_once.fetch_max(answering, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    stand.answering.fetch_sub(1, Ordering::SeqCst);
                    // The fourth photo asked for is throttled once.
                    if stand.asked.fetch_add(1, Ordering::SeqCst) == 3 {
                        return Response::builder()
                            .status(429)
                            .header("retry-after", "5")
                            .body(Body::empty())
                            .unwrap();
                    }
                    Response::builder()
                        .status(302)
                        .header("location", format!("/s3/photo/{n}"))
                        .body(Body::empty())
                        .unwrap()
                },
            ),
        )
        .route(
            "/s3/photo/{n}",
            get(|Path(n): Path<usize>| async move { format!("photo {n}") }),
        )
        .with_state(stand.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let session = SignedIn {
        origin: origin.clone(),
        cookies: vec![Cookie {
            name: "session-token".into(),
            value: "fixture".into(),
            domain: FIXTURE.into(),
            path: "/".into(),
            secure: false,
            expires: None,
        }],
        user_agent: "Fixture".into(),
    };
    let http = Http::with_session(&session, &cortex::HOSTS).unwrap();
    let downloads = Http::detached(&origin, &cortex::PHOTO_HOSTS).unwrap();
    let turn = Turn::of(&origin, "c-1", Box::new(|| Box::pin(async { Ok(()) })));
    let refusals = Refusals::default();
    let addresses: Vec<String> = (0..LANES)
        .map(|n| format!("/workforce/photo/{n}"))
        .collect();
    let photos = futures_util::future::join_all((addresses.iter()).map(|address| {
        photo(
            &http,
            &downloads,
            &turn,
            &refusals,
            &origin,
            "/workforce",
            address,
            None,
        )
    }))
    .await;
    for (n, photo) in photos.into_iter().enumerate() {
        let (bytes, _) = photo.unwrap().unwrap();
        assert_eq!(bytes, format!("photo {n}").into_bytes());
    }
    assert_eq!(stand.asked.load(Ordering::SeqCst), LANES + 1);
    assert!(stand.most_at_once.load(Ordering::SeqCst) <= 4);
}

// Amazon refuses some offboarded associates' photo addresses alone: a 403 with Cortex
// answering otherwise after is that address's, kept as served nothing, and so is a last run
// of them once the read ends whole. As many in a row as are asked at once are a session
// Amazon ended: no more are asked for, and none of that run is kept as served nothing.
#[test]
fn a_photo_refused_alone_is_that_photos_and_a_run_of_refusals_ends_the_session() {
    let refusals = Refusals::default();
    assert!(refusals.refused("/workforce/photo/0"));
    refusals.answered();
    assert!(refusals.refused("/workforce/photo/2"));
    assert_eq!(
        refusals.alone(true),
        ["/workforce/photo/0", "/workforce/photo/2"]
    );
    let refusals = Refusals::default();
    assert!(refusals.refused("/workforce/photo/0"));
    refusals.answered();
    assert!(refusals.refused("/workforce/photo/2"));
    // A read a throttle ended can't tell the last run from a session's end.
    assert_eq!(refusals.alone(false), ["/workforce/photo/0"]);
    let refusals = Refusals::default();
    refusals.answered();
    for n in 0..LANES - 1 {
        assert!(refusals.refused(&format!("/workforce/photo/{n}")));
    }
    assert!(!refusals.refused("/workforce/photo/last"));
    assert!(!refusals.refused("/workforce/photo/after"));
    assert!(refusals.alone(true).is_empty());
}

// An address Amazon lately served nothing at waits only while the roster just read lists
// whoever it names as offboarded: someone back on the roster is asked for again at once.
#[test]
fn a_photo_amazon_served_none_of_waits_only_for_the_offboarded() {
    let read = |id: &str, status: &str| {
        Associate::read(
            json!({"transporter_id":id,"full_name":id,"operational_status":status,
            "photo_url":format!("/workforce/photo/{id}")}),
        )
        .unwrap()
        .unwrap()
    };
    let associates = [
        read("A1", "OFFBOARDED"),
        read("A2", "ACTIVE"),
        read("A3", "OFFBOARDED"),
    ];
    let kept = KeptPhotos {
        unserved: ["/workforce/photo/A1", "/workforce/photo/A2"]
            .map(String::from)
            .into(),
        ..KeptPhotos::default()
    };
    assert_eq!(
        wanted(&associates, &kept),
        [("/workforce/photo/A2", None), ("/workforce/photo/A3", None)]
    );
}
