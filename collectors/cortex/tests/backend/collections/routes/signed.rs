use super::*;

const ORIGIN: &str = "https://logistics.amazon.com";

fn scope() -> Scope {
    Scope {
        date: "2026-10-18".into(),
        station: "TST1".into(),
        service_area_id: "area-1".into(),
        provider: "company-1".into(),
        timezone: "America/Los_Angeles".into(),
    }
}
fn request(url: &str) -> Value {
    json!({"method":"GET","url":format!("{ORIGIN}{url}"),"headers":{
        "Accept":"application/json, text/plain, */*","Cookie":"session-id=secret",
        "Authorization":"Bearer secret","X-Cortex-Hmac-Signature":"c2lnbmF0dXJl",
        "X-Cortex-Timestamp":"1760800000000","X-Cortex-Session":"session-1",
        "user-ref":"cortex-webapp-user"}})
}
const FIRST: &str = concat!(
    "/operations/execution/api/itineraries/it-1",
    "?documentType=Itinerary&historicalDay=true&itineraryId=it-1&serviceAreaId=area-1"
);

#[test]
fn a_paused_request_is_used_only_as_the_read_asked_for_with_only_its_signature() {
    let scope = scope();
    let listed = [
        ("it-1".to_owned(), "t-1".to_owned()),
        ("it:2".to_owned(), "t-2".to_owned()),
    ];
    let day = Day::new(&scope, &listed);
    let (read, signed) = day.signed(ORIGIN, &request(FIRST)).unwrap();
    assert_eq!(read, Read::Itinerary(0));
    assert_eq!(signed.url, format!("{ORIGIN}{FIRST}"));
    // Never the page's cookies, credentials or anything else it sent.
    assert_eq!(
        signed.headers,
        [
            ("user-ref", "cortex-webapp-user"),
            ("x-cortex-hmac-signature", "c2lnbmF0dXJl"),
            ("x-cortex-session", "session-1"),
            ("x-cortex-timestamp", "1760800000000"),
        ]
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
    );
    // An identifier is matched as the application spells it in the address.
    let second = concat!(
        "/operations/execution/api/itineraries/it%3A2",
        "?documentType=Itinerary&itineraryId=it%3A2&serviceAreaId=area-1"
    );
    assert_eq!(
        day.signed(ORIGIN, &request(second)).map(|(r, _)| r),
        Some(Read::Itinerary(1))
    );
    // Or unescaped: the server reads both as the same itinerary.
    let unescaped = second.replacen("it%3A2?", "it:2?", 1);
    assert_eq!(
        day.signed(ORIGIN, &request(&unescaped)).map(|(r, _)| r),
        Some(Read::Itinerary(1))
    );
    let routes = concat!(
        "/operations/execution/api/route-summaries",
        "?historicalDay=false&localDate=2026-10-18&serviceAreaId=area-1&statsFromSummaries=true"
    );
    assert_eq!(
        day.signed(ORIGIN, &request(routes)).map(|(r, _)| r),
        Some(Read::Routes)
    );

    let query = "?documentType=Itinerary&itineraryId=it-1&serviceAreaId=area-1";
    let refused: Vec<(&str, Value)> = vec![
        (
            "a deeper path",
            request(&format!(
                "/operations/execution/api/itineraries/it-1/x{query}"
            )),
        ),
        (
            "a broken escape",
            request(&format!(
                "/operations/execution/api/itineraries/it%G1{query}"
            )),
        ),
        ("another origin", {
            let mut r = request(FIRST);
            r["url"] = json!(format!("https://logistics.amazon.co.uk{FIRST}"));
            r
        }),
        ("a POST", {
            let mut r = request(FIRST);
            r["method"] = json!("POST");
            r
        }),
        (
            "an itinerary not listed",
            request(
                "/operations/execution/api/itineraries/it-3?documentType=Itinerary&itineraryId=it-3&serviceAreaId=area-1",
            ),
        ),
        (
            "one itinerary's address asking for another",
            request(
                "/operations/execution/api/itineraries/it-1?documentType=Itinerary&itineraryId=it%3A2&serviceAreaId=area-1",
            ),
        ),
        (
            "another service area",
            request(
                "/operations/execution/api/itineraries/it-1?documentType=Itinerary&itineraryId=it-1&serviceAreaId=area-2",
            ),
        ),
        (
            "another day's routes",
            request(
                "/operations/execution/api/route-summaries?localDate=2026-10-17&serviceAreaId=area-1&statsFromSummaries=true",
            ),
        ),
        (
            "a missing parameter",
            request(
                "/operations/execution/api/itineraries/it-1?itineraryId=it-1&serviceAreaId=area-1",
            ),
        ),
        (
            "an extra parameter",
            request(&format!("{FIRST}&provider=company-2")),
        ),
        (
            "a repeated parameter",
            request(&format!("{FIRST}&serviceAreaId=area-1")),
        ),
        (
            "an unknown day flag",
            request(
                "/operations/execution/api/itineraries/it-1?documentType=Itinerary&historicalDay=1&itineraryId=it-1&serviceAreaId=area-1",
            ),
        ),
        ("a fragment", {
            let mut r = request(FIRST);
            r["url"] = json!(format!("{ORIGIN}{FIRST}#x"));
            r
        }),
        ("no signature", {
            let mut r = request(FIRST);
            r["headers"]
                .as_object_mut()
                .unwrap()
                .remove("X-Cortex-Hmac-Signature");
            r
        }),
        ("an empty session", {
            let mut r = request(FIRST);
            r["headers"]["X-Cortex-Session"] = json!("");
            r
        }),
        ("a header that is not text", {
            let mut r = request(FIRST);
            r["headers"]["X-Cortex-Timestamp"] = json!("1\r\nCookie: x");
            r
        }),
        ("a header sent twice", {
            let mut r = request(FIRST);
            r["headers"]["x-cortex-session"] = json!("session-2");
            r
        }),
    ];
    for (case, request) in refused {
        assert!(day.signed(ORIGIN, &request).is_none(), "{case}");
    }
}
